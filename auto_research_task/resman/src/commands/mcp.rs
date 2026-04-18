//! Minimal Model Context Protocol (MCP) server over stdio.
//!
//! MCP is how Claude Code, Cursor, Codex, and other agent harnesses expose
//! external tools to the LLM. Karpathy merged native MCP support upstream
//! (issue #98), so shipping a first-class MCP surface for resman means the
//! agent can call `get_best` / `search_tried` / `record_experiment` directly
//! without our CLI appearing in its context window.
//!
//! Protocol: JSON-RPC 2.0, one message per line on stdin/stdout.
//! Spec: https://modelcontextprotocol.io/specification — we implement the
//! subset needed by Claude Code 1.x: `initialize`, `tools/list`, `tools/call`.
//! Notifications (`notifications/initialized`, etc.) are accepted and ignored.

use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde_json::{Value, json};

use crate::error::Result;
use crate::model::Status;
use crate::store::{load_all_runs, load_run};

const PROTOCOL_VERSION: &str = "2024-11-05";
const SERVER_NAME: &str = "resman";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn cmd_mcp(data_dir: PathBuf) -> Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    // Log lifecycle to stderr so agent harnesses can surface it to the user,
    // while stdout stays pure JSON-RPC.
    eprintln!(
        "resman-mcp v{SERVER_VERSION}: listening on stdio (data_dir={})",
        data_dir.display()
    );

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) if !l.trim().is_empty() => l,
            Ok(_) => continue,
            Err(e) => {
                eprintln!("resman-mcp: stdin closed: {e}");
                break;
            }
        };

        let req: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                write_line(
                    &mut out,
                    &error_response(Value::Null, -32700, &format!("parse error: {e}")),
                );
                continue;
            }
        };

        // Notifications have no "id" field → no response.
        let id = req.get("id").cloned();
        let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let params = req.get("params").cloned().unwrap_or(Value::Null);

        match (method, id) {
            ("initialize", Some(id)) => {
                write_line(&mut out, &ok_response(id, initialize_result()));
            }
            ("tools/list", Some(id)) => {
                write_line(
                    &mut out,
                    &ok_response(id, json!({ "tools": tool_manifest() })),
                );
            }
            ("tools/call", Some(id)) => {
                let res = handle_tool_call(&data_dir, &params);
                match res {
                    Ok(text) => {
                        write_line(&mut out, &ok_response(id, tool_text_result(&text, false)))
                    }
                    Err(msg) => {
                        write_line(&mut out, &ok_response(id, tool_text_result(&msg, true)))
                    }
                }
            }
            ("ping", Some(id)) => write_line(&mut out, &ok_response(id, json!({}))),
            // Notifications (no id) — ack silently.
            (_, None) => {}
            (other, Some(id)) => {
                write_line(
                    &mut out,
                    &error_response(id, -32601, &format!("method not found: {other}")),
                );
            }
        }
    }
    Ok(())
}

fn write_line(out: &mut impl Write, v: &Value) {
    let s = v.to_string();
    let _ = writeln!(out, "{s}");
    let _ = out.flush();
}

fn ok_response(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error_response(id: Value, code: i32, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn tool_text_result(text: &str, is_error: bool) -> Value {
    json!({
        "content": [{ "type": "text", "text": text }],
        "isError": is_error,
    })
}

fn initialize_result() -> Value {
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": SERVER_NAME, "version": SERVER_VERSION },
        "instructions": concat!(
            "Use these tools to record and query ML training experiments. ",
            "Call `resman_best` before starting an experiment to know the current baseline. ",
            "Pass `composite: true` to get a multi-dim 'resume-from-here' score (metric + verification + lineage + description). ",
            "Call `resman_search` before trying an idea — it may already have been attempted. ",
            "Call `resman_add_experiment` after every run (keep, discard, or crash). ",
            "Tags group experiments into a session (e.g. `apr17-overnight`). ",
            "Metrics can be any name via `metric_name`; set `metric_direction` to `max` for higher-better metrics like accuracy. ",
            "If you have the last ~50 lines of the training log, pass it as `log_tail` in `resman_add_experiment` and resman will auto-classify crash signals (OOM, NaN, etc.). ",
            "Call `resman_verify` after a reproduction run to promote the experiment to verified if within tolerance. ",
            "At the end of a session, call `resman_distill` — it is the preferred end-of-session summary tool and gives the agent structured memory of what happened without reading every experiment."
        ),
    })
}

fn tool_manifest() -> Value {
    json!([
        {
            "name": "resman_best",
            "description": "Return the best experiment recorded so far (lowest val_bpb among kept runs). Call this before starting a new experiment to know what to beat.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "tag": { "type": "string", "description": "Optional: restrict to a single run tag." },
                    "composite": { "type": "boolean", "default": false, "description": "Pass true to get a multi-dim 'resume-from-here' score (metric + verification + lineage + description) instead of plain metric ranking." }
                }
            }
        },
        {
            "name": "resman_search",
            "description": "Case-insensitive regex search across all experiment descriptions, commits, and params. Use this to check if an idea has already been tried before wasting a 5-minute training run on it. Returns 'no matches' if the idea is unexplored.",
            "inputSchema": {
                "type": "object",
                "required": ["pattern"],
                "properties": {
                    "pattern": { "type": "string", "description": "Regex (e.g. 'GeLU|gelu', 'LR.*0\\.04')." },
                    "include_discarded": { "type": "boolean", "default": false }
                }
            }
        },
        {
            "name": "resman_near",
            "description": "Return the N experiments whose val_bpb is closest to a target value. Useful for grounding a new result ('other runs near 0.985 were mostly crashes from OOM').",
            "inputSchema": {
                "type": "object",
                "required": ["val_bpb"],
                "properties": {
                    "val_bpb": { "type": "number" },
                    "n": { "type": "integer", "default": 5 }
                }
            }
        },
        {
            "name": "resman_list_recent",
            "description": "Return the most recent N experiments across all runs, in timestamp order. Useful at session start to recall what was tried last.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "n": { "type": "integer", "default": 10 },
                    "tag": { "type": "string" }
                }
            }
        },
        {
            "name": "resman_add_experiment",
            "description": "Record an experiment. Call after every training run — even crashes. Status must be one of: keep, discard, crash, best.",
            "inputSchema": {
                "type": "object",
                "required": ["tag", "commit", "val_bpb", "status", "description"],
                "properties": {
                    "tag": { "type": "string" },
                    "commit": { "type": "string", "description": "Short git hash." },
                    "val_bpb": { "type": "number", "description": "0 for crashes." },
                    "memory_gb": { "type": "number", "default": 0 },
                    "status": { "type": "string", "enum": ["keep", "discard", "crash", "best"] },
                    "description": { "type": "string" },
                    "parent_commit": { "type": "string" },
                    "metric_name": { "type": "string", "description": "Primary metric name. Default 'val_bpb'." },
                    "metric_direction": { "type": "string", "enum": ["min", "max"], "description": "Lower-better (min, default) or higher-better (max)." },
                    "log_tail": { "type": "string", "description": "Optional: last ~50 lines of the training log. If provided, resman classifies it into typed signals automatically." }
                }
            }
        },
        {
            "name": "resman_find_by_signal",
            "description": "Find all experiments whose signals include a given type. Use when triaging why runs failed — 'how many OOMs did we get overnight?'. Returns a compact list with tag, commit, and brief signal context.",
            "inputSchema": {
                "type": "object",
                "required": ["signal_type"],
                "properties": {
                    "signal_type": {
                        "type": "string",
                        "enum": ["oom", "cuda_error", "nan_loss", "assert_fail", "timeout", "unknown"]
                    },
                    "tag": { "type": "string", "description": "Optional: restrict to a single run tag." }
                }
            }
        },
        {
            "name": "resman_diff_tags",
            "description": "Show the config/metric diff between the best (or latest) experiment of two tagged runs. Useful for 'why did branch A beat branch B?' analysis. Returns a compact text summary.",
            "inputSchema": {
                "type": "object",
                "required": ["tag_a", "tag_b"],
                "properties": {
                    "tag_a": { "type": "string" },
                    "tag_b": { "type": "string" },
                    "against": { "type": "string", "enum": ["best", "latest"], "default": "best" }
                }
            }
        },
        {
            "name": "resman_lineage",
            "description": "Return the lineage tree of a run's experiments, showing which experiment was branched from which via parent_commit links. Agents use this to understand which chains converged vs dead-ended. Marks nodes on the best-lineage with a star.",
            "inputSchema": {
                "type": "object",
                "required": ["tag"],
                "properties": {
                    "tag": { "type": "string" },
                    "highlight_best": { "type": "boolean", "default": false }
                }
            }
        },
        {
            "name": "resman_distill",
            "description": "Generate a structured summary of a run: best, lineage, failure signals, unexplored neighbors, and heuristic suggestions. The primary 'what did we learn last night?' artifact for agent long-term memory. Call this at the end of an overnight session to get a concise structured memory of what happened without reading every experiment.",
            "inputSchema": {
                "type": "object",
                "required": ["tag"],
                "properties": {
                    "tag": { "type": "string", "description": "The run tag to distill." },
                    "format": { "type": "string", "enum": ["markdown", "json"], "description": "Output format. Default markdown." }
                }
            }
        },
        {
            "name": "resman_verify",
            "description": "Re-verify an experiment by providing a re-run's metric value. If within tolerance in the expected direction, promotes the experiment's status to 'verified' and updates val_bpb. Does not orchestrate training — caller provides the new value.",
            "inputSchema": {
                "type": "object",
                "required": ["commit", "value"],
                "properties": {
                    "commit": { "type": "string", "description": "Full or short commit hash of the experiment to verify." },
                    "value":  { "type": "number", "description": "The new metric value from the re-run." },
                    "tolerance": { "type": "number", "description": "Absolute tolerance. Default 0.01." },
                    "tag":    { "type": "string", "description": "Optional: restrict search to this run tag." }
                }
            }
        }
    ])
}

fn handle_tool_call(data_dir: &Path, params: &Value) -> std::result::Result<String, String> {
    let name = params
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or("missing tool name")?;
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or(Value::Object(Default::default()));

    match name {
        "resman_best" => tool_best(data_dir, &args),
        "resman_search" => tool_search(data_dir, &args),
        "resman_near" => tool_near(data_dir, &args),
        "resman_list_recent" => tool_list_recent(data_dir, &args),
        "resman_add_experiment" => tool_add(data_dir, &args),
        "resman_diff_tags" => tool_diff_tags(data_dir, &args),
        "resman_lineage" => tool_lineage(data_dir, &args),
        "resman_find_by_signal" => tool_find_by_signal(data_dir, &args),
        "resman_distill" => tool_distill(data_dir, &args),
        "resman_verify" => tool_verify(data_dir, &args),
        other => Err(format!("unknown tool: {other}")),
    }
}

fn tool_best(data_dir: &Path, args: &Value) -> std::result::Result<String, String> {
    use crate::commands::best::{CompositeScores, composite_candidates, lineage_depth};
    use crate::model::Direction;

    let tag = args.get("tag").and_then(|v| v.as_str());
    let composite = args
        .get("composite")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let runs = match tag {
        Some(t) => match load_run(data_dir, t).map_err(|e| e.to_string())? {
            Some(r) => vec![r],
            None => return Err(format!("no such tag: {t}")),
        },
        None => load_all_runs(data_dir).map_err(|e| e.to_string())?,
    };
    if runs.is_empty() {
        return Ok("no experiments recorded yet.".into());
    }

    if composite {
        let candidates = composite_candidates(&runs);
        if candidates.is_empty() {
            return Ok("no kept experiments yet; start with a baseline.".into());
        }
        let first_dir = {
            let (r, e) = candidates[0];
            e.effective_direction(r)
        };
        let values: Vec<f64> = candidates.iter().map(|(_, e)| e.val_bpb).collect();
        let run_min = values.iter().cloned().fold(f64::INFINITY, f64::min);
        let run_max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

        let scored: Vec<(
            CompositeScores,
            &crate::model::RunLog,
            &crate::model::Experiment,
        )> = candidates
            .iter()
            .map(|(r, e)| {
                let s = CompositeScores::compute(e, r, run_min, run_max, first_dir);
                (s, *r, *e)
            })
            .collect();

        let winner = scored
            .iter()
            .enumerate()
            .max_by(|(i, (sa, _, _)), (j, (sb, _, _))| {
                sa.score
                    .partial_cmp(&sb.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| {
                        sa.metric
                            .partial_cmp(&sb.metric)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .then_with(|| j.cmp(i))
            })
            .map(|(_, triple)| triple);

        return match winner {
            Some((scores, run, e)) => {
                let metric = e.effective_metric_name(run);
                let depth = lineage_depth(e, run);
                Ok(format!(
                    "best (composite {:.3}): {metric}={:.6} (commit {}, memory_gb={:.1}) — {}\n  metric: {:.3}×0.5={:.3}  verified: {:.3}×0.2={:.3}  lineage: depth={} score={:.3}×0.2={:.3}  desc: {:.3}×0.1={:.3}",
                    scores.score,
                    e.val_bpb,
                    e.commit,
                    e.memory_gb,
                    e.description,
                    scores.metric,
                    0.5 * scores.metric,
                    scores.verified,
                    0.2 * scores.verified,
                    depth,
                    scores.lineage,
                    0.2 * scores.lineage,
                    scores.desc,
                    0.1 * scores.desc,
                ))
            }
            None => Ok("no kept experiments yet; start with a baseline.".into()),
        };
    }

    // Non-composite: original path.
    let mut global_best: Option<(&crate::model::RunLog, &crate::model::Experiment)> = None;
    for r in &runs {
        if let Some(b) = r.best() {
            let dir = b.effective_direction(r);
            match global_best {
                None => {
                    global_best = Some((r, b));
                }
                Some((gr, gb)) => {
                    let gdir = gb.effective_direction(gr);
                    let better = match gdir {
                        Direction::Minimize => b.val_bpb < gb.val_bpb,
                        Direction::Maximize => b.val_bpb > gb.val_bpb,
                    };
                    // Suppress the direction mismatch warning here (MCP is stdio).
                    let _ = dir;
                    if better {
                        global_best = Some((r, b));
                    }
                }
            }
        }
    }

    match global_best {
        Some((run, e)) => {
            let metric = e.effective_metric_name(run);
            Ok(format!(
                "best so far: {metric}={:.6} (commit {}, memory_gb={:.1}) — {}",
                e.val_bpb, e.commit, e.memory_gb, e.description
            ))
        }
        None => Ok("no kept experiments yet; start with a baseline.".into()),
    }
}

fn tool_search(data_dir: &Path, args: &Value) -> std::result::Result<String, String> {
    let pattern = args
        .get("pattern")
        .and_then(|v| v.as_str())
        .ok_or("missing `pattern`")?;
    let include_discarded = args
        .get("include_discarded")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let re = regex::RegexBuilder::new(pattern)
        .case_insensitive(true)
        .build()
        .map_err(|e| e.to_string())?;

    let runs = load_all_runs(data_dir).map_err(|e| e.to_string())?;
    let mut hits: Vec<String> = Vec::new();
    for r in &runs {
        for e in &r.experiments {
            if !include_discarded && e.status == Status::Discard {
                continue;
            }
            let hay = format!(
                "{} {} {}",
                e.description,
                e.commit,
                e.params
                    .iter()
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            );
            if re.is_match(&hay) {
                hits.push(format!(
                    "[{}] {} val_bpb={:.6} — {}",
                    r.run_tag, e.status, e.val_bpb, e.description
                ));
            }
        }
    }
    if hits.is_empty() {
        Ok(format!(
            "no matches for `{pattern}`. idea is unexplored — safe to try."
        ))
    } else {
        Ok(format!(
            "{} prior match(es):\n{}",
            hits.len(),
            hits.join("\n")
        ))
    }
}

fn tool_near(data_dir: &Path, args: &Value) -> std::result::Result<String, String> {
    let target = args
        .get("val_bpb")
        .and_then(|v| v.as_f64())
        .ok_or("missing `val_bpb`")?;
    let n = args.get("n").and_then(|v| v.as_u64()).unwrap_or(5) as usize;
    let runs = load_all_runs(data_dir).map_err(|e| e.to_string())?;
    let mut all: Vec<(String, &crate::model::Experiment)> = runs
        .iter()
        .flat_map(|r| r.experiments.iter().map(move |e| (r.run_tag.clone(), e)))
        .filter(|(_, e)| e.val_bpb > 0.0)
        .collect();
    all.sort_by(|a, b| {
        (a.1.val_bpb - target)
            .abs()
            .partial_cmp(&(b.1.val_bpb - target).abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    all.truncate(n);
    if all.is_empty() {
        return Ok("no prior experiments to compare against.".into());
    }
    let lines: Vec<_> = all
        .iter()
        .map(|(tag, e)| {
            format!(
                "[{tag}] {:.6} (Δ{:+.6}) {} — {}",
                e.val_bpb,
                e.val_bpb - target,
                e.status,
                e.description
            )
        })
        .collect();
    Ok(format!("neighbors of {target:.6}:\n{}", lines.join("\n")))
}

fn tool_list_recent(data_dir: &Path, args: &Value) -> std::result::Result<String, String> {
    let n = args.get("n").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
    let tag = args.get("tag").and_then(|v| v.as_str());
    let runs = match tag {
        Some(t) => match load_run(data_dir, t).map_err(|e| e.to_string())? {
            Some(r) => vec![r],
            None => return Err(format!("no such tag: {t}")),
        },
        None => load_all_runs(data_dir).map_err(|e| e.to_string())?,
    };
    // Keep &RunLog alongside &Experiment for metric name resolution.
    let mut all: Vec<(&crate::model::RunLog, &crate::model::Experiment)> = runs
        .iter()
        .flat_map(|r| r.experiments.iter().map(move |e| (r, e)))
        .collect();
    // Sort by timestamp descending; fall back to array order when timestamp is empty.
    all.sort_by(|a, b| b.1.timestamp.cmp(&a.1.timestamp));
    all.truncate(n);
    if all.is_empty() {
        return Ok("no experiments recorded yet.".into());
    }
    let lines: Vec<_> = all
        .iter()
        .map(|(run, e)| {
            let metric = e.effective_metric_name(run);
            format!(
                "[{}] {} {} {metric}={:.6} — {}",
                run.run_tag, e.timestamp, e.status, e.val_bpb, e.description
            )
        })
        .collect();
    Ok(lines.join("\n"))
}

fn tool_add(data_dir: &Path, args: &Value) -> std::result::Result<String, String> {
    let tag = args
        .get("tag")
        .and_then(|v| v.as_str())
        .ok_or("missing `tag`")?;
    let commit = args
        .get("commit")
        .and_then(|v| v.as_str())
        .ok_or("missing `commit`")?;
    let val_bpb = args
        .get("val_bpb")
        .and_then(|v| v.as_f64())
        .ok_or("missing `val_bpb`")?;
    let status_s = args
        .get("status")
        .and_then(|v| v.as_str())
        .ok_or("missing `status`")?;
    let description = args
        .get("description")
        .and_then(|v| v.as_str())
        .ok_or("missing `description`")?;
    let memory_gb = args
        .get("memory_gb")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let parent = args.get("parent_commit").and_then(|v| v.as_str());
    let metric_name = args.get("metric_name").and_then(|v| v.as_str());
    let metric_direction = args.get("metric_direction").and_then(|v| v.as_str());
    let log_tail = args.get("log_tail").and_then(|v| v.as_str());

    let _status = Status::from_str(status_s).map_err(|e| e.to_string())?;

    // Classify log_tail server-side if provided; bypass file I/O path.
    let preclassified = log_tail.map(crate::signals::classify);

    super::add::cmd_add(
        data_dir,
        super::add::AddOpts {
            tag,
            commit,
            val_bpb,
            memory_gb,
            status: status_s,
            description,
            params: &[],
            parent,
            log: None,
            no_gpu_probe: true,
            metric_name,
            metric_direction,
            preclassified_signals: preclassified,
        },
    )
    .map_err(|e| e.to_string())?;

    Ok(format!(
        "recorded: [{tag}] {commit} val_bpb={val_bpb:.6} {status_s}"
    ))
}

fn tool_diff_tags(data_dir: &Path, args: &Value) -> std::result::Result<String, String> {
    let tag_a = args
        .get("tag_a")
        .and_then(|v| v.as_str())
        .ok_or("missing `tag_a`")?;
    let tag_b = args
        .get("tag_b")
        .and_then(|v| v.as_str())
        .ok_or("missing `tag_b`")?;
    let against = args
        .get("against")
        .and_then(|v| v.as_str())
        .unwrap_or("best");

    super::diff::diff_summary_text(data_dir, tag_a, tag_b, against).map_err(|e| e.to_string())
}

fn tool_lineage(data_dir: &Path, args: &Value) -> std::result::Result<String, String> {
    let tag = args
        .get("tag")
        .and_then(|v| v.as_str())
        .ok_or("missing `tag`")?;
    let highlight_best = args
        .get("highlight_best")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    super::tree::tree_text(data_dir, tag, highlight_best).map_err(|e| e.to_string())
}

fn tool_find_by_signal(data_dir: &Path, args: &Value) -> std::result::Result<String, String> {
    let want = args
        .get("signal_type")
        .and_then(|v| v.as_str())
        .ok_or("missing `signal_type`")?;
    if !crate::signals::ALL_KINDS.contains(&want) {
        return Err(format!(
            "unknown signal_type `{want}`. expected one of: {}",
            crate::signals::ALL_KINDS.join(", ")
        ));
    }
    let tag = args.get("tag").and_then(|v| v.as_str());
    let runs = match tag {
        Some(t) => match load_run(data_dir, t).map_err(|e| e.to_string())? {
            Some(r) => vec![r],
            None => return Err(format!("no such tag: {t}")),
        },
        None => load_all_runs(data_dir).map_err(|e| e.to_string())?,
    };
    let mut hits: Vec<String> = Vec::new();
    for r in &runs {
        for e in &r.experiments {
            if e.signals.iter().any(|s| s.kind() == want) {
                let ctx = e
                    .signals
                    .iter()
                    .find(|s| s.kind() == want)
                    .map(signal_context)
                    .unwrap_or_default();
                hits.push(format!(
                    "[{}] {} {} — {}{}",
                    r.run_tag, e.status, e.commit, e.description, ctx
                ));
            }
        }
    }
    if hits.is_empty() {
        Ok(format!("no experiments with signal `{want}` found."))
    } else {
        Ok(format!(
            "{} experiment(s) with signal `{want}`:\n{}",
            hits.len(),
            hits.join("\n")
        ))
    }
}

fn tool_distill(data_dir: &Path, args: &Value) -> std::result::Result<String, String> {
    let tag = args
        .get("tag")
        .and_then(|v| v.as_str())
        .ok_or("missing `tag`")?;
    let format_str = args
        .get("format")
        .and_then(|v| v.as_str())
        .unwrap_or("markdown");
    let use_json = format_str == "json";
    super::distill::distill_to_string(data_dir, tag, use_json)
}

fn tool_verify(data_dir: &Path, args: &Value) -> std::result::Result<String, String> {
    let commit = args
        .get("commit")
        .and_then(|v| v.as_str())
        .ok_or("missing `commit`")?;
    let new_value = args
        .get("value")
        .and_then(|v| v.as_f64())
        .ok_or("missing `value`")?;
    let tolerance = args
        .get("tolerance")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.01);
    let tag = args.get("tag").and_then(|v| v.as_str());

    super::verify::verify_inner(
        data_dir,
        &super::verify::VerifyOpts {
            commit,
            new_value,
            tolerance,
            tag,
        },
    )
    .map_err(|e| e.to_string())
}

fn signal_context(s: &crate::signals::Signal) -> String {
    use crate::signals::Signal::*;
    match s {
        CudaError { hint } if !hint.is_empty() => format!("  [{hint}]"),
        AssertFail { location } if !location.is_empty() => format!("  [at {location}]"),
        Unknown { pattern } if !pattern.is_empty() => format!("  [pattern: {pattern}]"),
        _ => String::new(),
    }
}
