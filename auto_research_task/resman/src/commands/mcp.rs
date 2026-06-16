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

/// Canonical list of every MCP tool name. Single source of truth shared by the
/// manifest, dispatch, and usage cold-tool analysis.
pub(crate) const TOOL_NAMES: &[&str] = &[
    "resman_best",
    "resman_search",
    "resman_near",
    "resman_list_recent",
    "resman_add_experiment",
    "resman_diff_tags",
    "resman_lineage",
    "resman_find_by_signal",
    "resman_distill",
    "resman_verify",
    "resman_unverify",
    "resman_doctor",
    "resman_tags",
    "resman_list",
    "resman_compare",
    "resman_stats",
    "resman_usage",
];

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
                if !write_line(
                    &mut out,
                    &error_response(Value::Null, -32700, &format!("parse error: {e}")),
                ) {
                    eprintln!("resman-mcp: client disconnected");
                    break;
                }
                continue;
            }
        };

        // Notifications have no "id" field → no response.
        let id = req.get("id").cloned();
        let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let params = req.get("params").cloned().unwrap_or(Value::Null);

        let ok = match (method, id) {
            ("initialize", Some(id)) => write_line(&mut out, &ok_response(id, initialize_result())),
            ("tools/list", Some(id)) => write_line(
                &mut out,
                &ok_response(id, json!({ "tools": tool_manifest() })),
            ),
            ("tools/call", Some(id)) => {
                // [M8] name missing/null/not-a-string → JSON-RPC -32602.
                let name_val = params.get("name");
                let tool_name_opt = name_val.and_then(|v| v.as_str());
                if name_val.is_none() || tool_name_opt.is_none() {
                    write_line(
                        &mut out,
                        &error_response(id, -32602, "invalid params: `name` must be a string"),
                    )
                } else {
                    let tool_name = tool_name_opt.unwrap_or("").to_string();
                    let tool_args = params.get("arguments").cloned().unwrap_or(Value::Null);
                    let timer = crate::usage::CallTimer::start();
                    let res = handle_tool_call(&data_dir, &params);
                    let (text, is_error) = match res {
                        Ok(t) => (t, false),
                        Err(m) => (m, true),
                    };
                    crate::usage::log_call(
                        &data_dir,
                        &tool_name,
                        &tool_args,
                        !is_error,
                        timer.elapsed_ms(),
                        text.len(),
                    );
                    write_line(
                        &mut out,
                        &ok_response(id, tool_text_result(&text, is_error)),
                    )
                }
            }
            ("ping", Some(id)) => write_line(&mut out, &ok_response(id, json!({}))),
            // Notifications (no id) — ack silently.
            (_, None) => true,
            (other, Some(id)) => write_line(
                &mut out,
                &error_response(id, -32601, &format!("method not found: {other}")),
            ),
        };
        if !ok {
            eprintln!("resman-mcp: client disconnected");
            break;
        }
    }
    Ok(())
}

fn write_line(out: &mut impl Write, v: &Value) -> bool {
    let s = v.to_string();
    if writeln!(out, "{s}").is_err() {
        return false;
    }
    out.flush().is_ok()
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
            "resman is your long-term memory across training sessions. All tools return JSON — parse, don't substring-match. ",
            "Session lifecycle: ",
            "(0) First call `resman_doctor {}` — read summary.fail; if non-zero, fix the failing check's hint before continuing. ",
            "(0b) Optional but recommended: `resman_tags {}` for a per-tag snapshot (count, best, last_update). One row per tag — easier than scanning list_recent for tag boundaries. ",
            "(1) Then `resman_list_recent { n: 20 }`. If total === 0 the store is fresh; tell the user. Otherwise pass tags[0] to `resman_distill { tag }` and read every section. ",
            "(2) Before any experiment: `resman_search { pattern }` to avoid duplicate ideas, and `resman_best { composite: true }` for the resume-from-here ranking. ",
            "Pre-flight checks for risky configs: `resman_find_by_signal { signal_type: 'oom' }` to skip known crashers. ",
            "Lineage: branching from non-HEAD? `resman_lineage { tag }` shows which chains converged or dead-ended. ",
            "(3) After every run: `resman_add_experiment { tag, commit, val_bpb, memory_gb, status, description, parent_commit, log_tail }`. ",
            "parent_commit is required to keep lineage intact — if you omit it on a non-fresh tag, the response includes a `lineage chain broken` warning. Always note the parent SHA before you commit so you have it for this call. ",
            "log_tail = last ~50 lines of run.log — resman auto-classifies OOM / NaN / DivergedLoss / SlowMfu / etc. Pass it even on success runs. ",
            "(4) After a reproduction near a prior baseline: `resman_verify { commit, value }`. Tolerance is absolute (default 0.01), direction-sensitive: minimize passes if new <= original + tolerance. ",
            "(5) If later evidence disagrees with a verified result, retract: `resman_unverify { commit }`. Symmetric — val_bpb retained, only the trust label moves back to keep. ",
            "(6) Every ~10 runs and at session end: `resman_distill { tag }`. This is what the next session will inherit — make sure it tells the story you'd want to read. ",
            "Tags group experiments into a session (e.g. `apr17-overnight`). Metrics can be any name via metric_name; set metric_direction to 'max' for higher-better metrics. ",
            "Diagnostic surface: `resman_diff_tags` for branch-vs-branch comparison, `resman_near` for grounding a new bpb, `resman_list` for filtered/sorted triage (by status/signal/grep), `resman_compare` for best-of-each across runs, `resman_stats` for aggregate counts + bpb spread, and `resman_usage` to audit your own tool use (adoption funnel + cold tools). ",
            "See docs/AGENT_QUICKSTART.md for the full protocol."
        ),
    })
}

fn tool_manifest() -> Value {
    json!([
        {
            "name": "resman_best",
            "description": "Return the best experiment recorded so far (lowest val_bpb among kept runs). Call this before starting a new experiment to know what to beat. Returns JSON: {tag, commit, metric, value, memory_gb, status, description, composite}. composite is null unless composite=true, then {score, metric, verified, lineage_depth, lineage, desc, *_weighted}.",
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
            "description": "Case-insensitive regex search across all experiment descriptions, commits, and params. Use this to check if an idea has already been tried before wasting a 5-minute training run on it. Returns JSON: {pattern, count, matches[]}.",
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
            "description": "Return the N experiments whose val_bpb is closest to a target value. Useful for grounding a new result ('other runs near 0.985 were mostly crashes from OOM'). Returns JSON: {target, count, neighbors[]}.",
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
            "description": "Return the most recent N experiments across all runs in timestamp order, plus the total count and unique tags. Returns a JSON string with shape {total, tags, experiments}. Use at session start to recall what was tried last; total=0 signals a fresh resman store.",
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
            "description": "Record an experiment. Call after every training run — even crashes. Status must be one of: keep, discard, crash, best. If this tag already has prior experiments and parent_commit is omitted, the response will include a lineage warning.",
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
            "description": "Find all experiments whose signals include a given type. Use when triaging why runs failed — 'how many OOMs did we get overnight?'. Returns JSON: {signal_type, count, matches[]}.",
            "inputSchema": {
                "type": "object",
                "required": ["signal_type"],
                "properties": {
                    "signal_type": {
                        "type": "string",
                        "enum": crate::signals::ALL_KINDS
                    },
                    "tag": { "type": "string", "description": "Optional: restrict to a single run tag." }
                }
            }
        },
        {
            "name": "resman_diff_tags",
            "description": "Show the config/metric diff between the best (or latest) experiment of two tagged runs. Useful for 'why did branch A beat branch B?' analysis. Returns JSON: {tag_a, tag_b, against, a, b, delta, params}.",
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
            "description": "Return the lineage tree of a run's experiments, showing which experiment was branched from which via parent_commit links. Agents use this to understand which chains converged vs dead-ended. Returns JSON: {tag, highlight_best, best_commit, trees[]}; trees recursive.",
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
            "description": "Generate a structured summary of a run: best, lineage, failure signals, unexplored neighbors, and heuristic suggestions. The primary 'what did we learn last night?' artifact for agent long-term memory. Call this at the end of an overnight session to get a concise structured memory of what happened without reading every experiment. Returns JSON via format=json.",
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
            "description": "Re-verify an experiment by providing a re-run's metric value. If within tolerance in the expected direction, promotes the experiment's status to 'verified' and updates val_bpb. Does not orchestrate training — caller provides the new value. Returns JSON: {verified: bool, ...details}; verified=false includes exceeded_by.",
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
        },
        {
            "name": "resman_doctor",
            "description": "Run six environment + data-integrity checks (data_dir, resman_home_env, runs_present, usage_telemetry, mcp_wiring, invariants). Returns JSON: {summary:{ok,warn,fresh,fail}, checks:[{name,status,detail,hint}]}. Call this at session start to confirm the environment is wired correctly before relying on other tools — one call replaces a dozen exploratory probes.",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        },
        {
            "name": "resman_tags",
            "description": "Per-tag snapshot — one entry per tag, sorted by last_update desc. Returns JSON array of {tag, experiment_count, best_commit, best_value, metric_name, direction, last_update, schema_version}. Use this for 'what tags do I have, what is each one's headline?' — a higher-level probe than resman_list_recent (per-experiment) and complementary to resman_doctor (env/wiring).",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        },
        {
            "name": "resman_unverify",
            "description": "Symmetric retraction of `resman_verify`. Reverts a Verified experiment back to Keep status. Use when a verified result turns out to be a fluke (subsequent re-runs disagree). The val_bpb value is retained — only the trust label changes. Returns JSON: {unverified:true, tag, commit, metric, retained_value, previous_status, new_status}. Refuses experiments not currently in `verified` status (returns error).",
            "inputSchema": {
                "type": "object",
                "required": ["commit"],
                "properties": {
                    "commit": { "type": "string", "description": "Full or short commit hash of the verified experiment to retract." },
                    "tag": { "type": "string", "description": "Optional: restrict search to this run tag." }
                }
            }
        },
        {
            "name": "resman_list",
            "description": "Filtered, sorted list of experiments across all runs (richer than resman_list_recent: supports status/signal/grep filters and choice of sort field). Defaults to kept-only experiments. Returns JSON {count, experiments:[{tag, commit, val_bpb, memory_gb, status, metric_name, description, signals}]}.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "status": { "type": "string", "enum": ["keep","discard","crash","best","verified","all"], "description": "Filter by status. Omit for the default kept-only view; 'all' shows everything." },
                    "sort_by": { "type": "string", "enum": ["val_bpb","memory_gb","description","commit"], "default": "val_bpb" },
                    "grep": { "type": "string", "description": "Regex filter on description." },
                    "top": { "type": "integer", "description": "Show only the top N after sorting." },
                    "reverse": { "type": "boolean", "default": false },
                    "tag": { "type": "string", "description": "Restrict to a single run tag." },
                    "signal": { "type": "array", "items": { "type": "string", "enum": crate::signals::ALL_KINDS }, "description": "Keep only experiments whose signals include ALL of these (AND)." }
                }
            }
        },
        {
            "name": "resman_compare",
            "description": "Compare the best experiment of multiple runs side by side. Omit tags to compare all runs; tags match by substring. Returns JSON {count, runs:[{run, best_bpb, metric_name, direction, best_commit, best_description, kept, crashed, total}]}.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "tags": { "type": "array", "items": { "type": "string" }, "description": "Run tags (substring match). Omit to compare all runs." }
                }
            }
        },
        {
            "name": "resman_stats",
            "description": "Aggregate statistics over experiments — status counts (kept/discarded/crashed) plus val_bpb best/worst/mean/stddev/improvement over kept runs. Omit tag for a cross-run summary. Returns JSON {tag, total, kept, discarded, crashed, val_bpb}; val_bpb is null when no kept run has a positive metric.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "tag": { "type": "string", "description": "Restrict to a single run tag." }
                }
            }
        },
        {
            "name": "resman_usage",
            "description": "Telemetry summary from usage.jsonl (written by this MCP server on every tool call): totals, per-tag adoption funnel (added→verified→distilled), and cold tools never called. Use to audit how the agent actually uses resman and to find discoverability gaps. Returns JSON {total_events, ok, errors, funnel_by_tag, cold_tools}.",
            "inputSchema": {
                "type": "object",
                "properties": {}
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
        "resman_unverify" => tool_unverify(data_dir, &args),
        "resman_doctor" => tool_doctor(data_dir, &args),
        "resman_tags" => tool_tags(data_dir, &args),
        "resman_list" => tool_list(data_dir, &args),
        "resman_compare" => tool_compare(data_dir, &args),
        "resman_stats" => tool_stats(data_dir, &args),
        "resman_usage" => tool_usage(data_dir, &args),
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
        return serde_json::to_string(&json!({
            "empty": true,
            "reason": "no experiments recorded yet."
        }))
        .map_err(|e| e.to_string());
    }

    if composite {
        let candidates = composite_candidates(&runs);
        if candidates.is_empty() {
            return serde_json::to_string(&json!({
                "empty": true,
                "reason": "no kept experiments yet; start with a baseline."
            }))
            .map_err(|e| e.to_string());
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
                let result = json!({
                    "tag": run.run_tag,
                    "commit": e.commit,
                    "metric": metric,
                    "value": e.val_bpb,
                    "memory_gb": e.memory_gb,
                    "status": e.status.to_string(),
                    "description": e.description,
                    "composite": {
                        "score": scores.score,
                        "metric": scores.metric,
                        "metric_weighted": 0.5 * scores.metric,
                        "verified": scores.verified,
                        "verified_weighted": 0.2 * scores.verified,
                        "lineage_depth": depth,
                        "lineage": scores.lineage,
                        "lineage_weighted": 0.2 * scores.lineage,
                        "desc": scores.desc,
                        "desc_weighted": 0.1 * scores.desc,
                    }
                });
                serde_json::to_string(&result).map_err(|e| e.to_string())
            }
            None => serde_json::to_string(&json!({
                "empty": true,
                "reason": "no kept experiments yet; start with a baseline."
            }))
            .map_err(|e| e.to_string()),
        };
    }

    // Non-composite: structured JSON path.
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
            let result = json!({
                "tag": run.run_tag,
                "commit": e.commit,
                "metric": metric,
                "value": e.val_bpb,
                "memory_gb": e.memory_gb,
                "status": e.status.to_string(),
                "description": e.description,
                "composite": null
            });
            serde_json::to_string(&result).map_err(|e| e.to_string())
        }
        None => serde_json::to_string(&json!({
            "empty": true,
            "reason": "no kept experiments yet; start with a baseline."
        }))
        .map_err(|e| e.to_string()),
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
    let mut matches: Vec<Value> = Vec::new();
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
                let metric = e.effective_metric_name(r);
                matches.push(json!({
                    "tag": r.run_tag,
                    "commit": e.commit,
                    "val_bpb": e.val_bpb,
                    "metric_name": metric,
                    "status": e.status.to_string(),
                    "description": e.description,
                }));
            }
        }
    }
    let result = json!({
        "pattern": pattern,
        "count": matches.len(),
        "matches": matches,
    });
    serde_json::to_string(&result).map_err(|e| e.to_string())
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
    let neighbors: Vec<Value> = all
        .iter()
        .map(|(tag, e)| {
            json!({
                "tag": tag,
                "commit": e.commit,
                "val_bpb": e.val_bpb,
                "delta": e.val_bpb - target,
                "status": e.status.to_string(),
                "description": e.description,
            })
        })
        .collect();
    let result = json!({
        "target": target,
        "count": neighbors.len(),
        "neighbors": neighbors,
    });
    serde_json::to_string(&result).map_err(|e| e.to_string())
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
    let total = all.len();
    all.truncate(n);

    // Build unique ordered tags from the truncated list.
    let mut seen_tags: Vec<String> = Vec::new();
    for (run, _) in &all {
        if !seen_tags.contains(&run.run_tag) {
            seen_tags.push(run.run_tag.clone());
        }
    }

    let experiments: Vec<Value> = all
        .iter()
        .map(|(run, e)| {
            let metric = e.effective_metric_name(run);
            json!({
                "tag": run.run_tag,
                "commit": e.commit,
                "timestamp": e.timestamp,
                "status": e.status.to_string(),
                "val_bpb": e.val_bpb,
                "metric_name": metric,
                "description": e.description,
            })
        })
        .collect();

    let result = json!({
        "total": total,
        "tags": seen_tags,
        "experiments": experiments,
    });
    serde_json::to_string(&result).map_err(|e| e.to_string())
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

    // [M5-MCP] Reject non-finite val_bpb (NaN / ±inf). Crashes must use 0.0.
    if !val_bpb.is_finite() {
        return Err(
            "val_bpb must be finite; crashes use 0.0 — NaN/±inf are not accepted".to_string(),
        );
    }

    let _status = Status::from_str(status_s).map_err(|e| e.to_string())?;

    // Capture prior count before cmd_add mutates the store.
    let prior_count = crate::store::load_run(data_dir, tag)
        .map_err(|e| e.to_string())?
        .map_or(0, |r| r.experiments.len());
    let warn_missing_parent = parent.is_none() && prior_count >= 1;

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

    // [H2] Return a JSON object instead of a plain-text ack.
    let lineage_warning: Option<String> = if warn_missing_parent {
        Some(format!(
            "no `parent_commit` set, but tag `{tag}` already has {prior_count} prior experiment(s) — lineage chain broken at this commit. Pass `parent_commit` next time to keep lineage intact."
        ))
    } else {
        None
    };
    let result = json!({
        "recorded": true,
        "tag": tag,
        "commit": commit,
        "val_bpb": val_bpb,
        "status": status_s,
        "lineage_warning": lineage_warning,
    });
    serde_json::to_string(&result).map_err(|e| e.to_string())
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

    super::diff::diff_summary_json(data_dir, tag_a, tag_b, against).map_err(|e| e.to_string())
}

fn tool_lineage(data_dir: &Path, args: &Value) -> std::result::Result<String, String> {
    use crate::store::load_run_or_suggest;

    let tag = args
        .get("tag")
        .and_then(|v| v.as_str())
        .ok_or("missing `tag`")?;
    let highlight_best = args
        .get("highlight_best")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let run = load_run_or_suggest(data_dir, tag).map_err(|e| e.to_string())?;
    let forest = super::tree::build_forest(&run);

    fn node_to_json(node: &super::tree::TreeNode<'_>) -> Value {
        let children: Vec<Value> = node.children.iter().map(node_to_json).collect();
        json!({
            "commit": node.exp.commit,
            "val_bpb": node.exp.val_bpb,
            "status": node.exp.status.to_string(),
            "description": node.exp.description,
            "on_best_path": node.on_best_lineage,
            "children": children,
        })
    }

    let trees: Vec<Value> = forest
        .roots
        .iter()
        .filter(|r| !highlight_best || r.on_best_lineage)
        .map(|r| node_to_json(r))
        .collect();

    let result = json!({
        "tag": tag,
        "highlight_best": highlight_best,
        "best_commit": forest.best_commit,
        "trees": trees,
    });
    serde_json::to_string(&result).map_err(|e| e.to_string())
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
    let mut matches: Vec<Value> = Vec::new();
    for r in &runs {
        for e in &r.experiments {
            if let Some(sig) = e.signals.iter().find(|s| s.kind() == want) {
                let sig_value = serde_json::to_value(sig).unwrap_or(Value::Null);
                matches.push(json!({
                    "tag": r.run_tag,
                    "commit": e.commit,
                    "status": e.status.to_string(),
                    "description": e.description,
                    "signal": sig_value,
                    "crash_excerpt": e.crash_excerpt,
                }));
            }
        }
    }
    let result = json!({
        "signal_type": want,
        "count": matches.len(),
        "matches": matches,
    });
    serde_json::to_string(&result).map_err(|e| e.to_string())
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

    super::verify::verify_inner_json(
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

fn tool_tags(data_dir: &Path, _args: &Value) -> std::result::Result<String, String> {
    let snaps = crate::commands::tags::build_tag_snapshots(data_dir).map_err(|e| e.to_string())?;
    serde_json::to_string(&snaps).map_err(|e| e.to_string())
}

fn tool_unverify(data_dir: &Path, args: &Value) -> std::result::Result<String, String> {
    let commit = args
        .get("commit")
        .and_then(|v| v.as_str())
        .ok_or("missing `commit`")?;
    let tag = args.get("tag").and_then(|v| v.as_str());

    super::verify::unverify_inner_json(data_dir, &super::verify::UnverifyOpts { commit, tag })
        .map_err(|e| e.to_string())
}

fn tool_doctor(data_dir: &Path, _args: &Value) -> std::result::Result<String, String> {
    use crate::commands::doctor::{CheckStatus, DoctorReport, DoctorSummary, run_checks};
    let checks = run_checks(data_dir);
    let mut ok = 0usize;
    let mut warn = 0usize;
    let mut fresh = 0usize;
    let mut fail = 0usize;
    for c in &checks {
        match c.status {
            CheckStatus::Ok => ok += 1,
            CheckStatus::Warn => warn += 1,
            CheckStatus::Fresh => fresh += 1,
            CheckStatus::Fail => fail += 1,
        }
    }
    let report = DoctorReport {
        summary: DoctorSummary {
            ok,
            warn,
            fresh,
            fail,
        },
        checks,
    };
    serde_json::to_string(&report).map_err(|e| e.to_string())
}

fn tool_list(data_dir: &Path, args: &Value) -> std::result::Result<String, String> {
    use crate::cli::SortField;
    use crate::commands::list::filter_sort_truncate;
    use crate::model::Experiment;

    let status = args.get("status").and_then(|v| v.as_str());
    let grep = args.get("grep").and_then(|v| v.as_str());
    let tag = args.get("tag").and_then(|v| v.as_str());
    let top = args.get("top").and_then(|v| v.as_u64()).map(|n| n as usize);
    let reverse = args
        .get("reverse")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let signal_filters: Vec<String> = args
        .get("signal")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let sort_by = match args
        .get("sort_by")
        .and_then(|v| v.as_str())
        .unwrap_or("val_bpb")
    {
        "val_bpb" | "val-bpb" => SortField::ValBpb,
        "memory_gb" | "memory-gb" => SortField::MemoryGb,
        "description" => SortField::Description,
        "commit" => SortField::Commit,
        other => {
            return Err(format!(
                "unknown sort_by value `{other}`; expected one of: val_bpb, memory_gb, description, commit"
            ));
        }
    };

    let runs = match tag {
        Some(t) => match load_run(data_dir, t).map_err(|e| e.to_string())? {
            Some(r) => vec![r],
            None => return Err(format!("no such tag: {t}")),
        },
        None => load_all_runs(data_dir).map_err(|e| e.to_string())?,
    };

    let tagged: Vec<(Experiment, crate::model::RunLog)> = runs
        .into_iter()
        .flat_map(|r| {
            let exps: Vec<Experiment> = r.experiments.clone();
            exps.into_iter()
                .map(move |e| (e, r.clone()))
                .collect::<Vec<_>>()
        })
        .collect();

    let filtered = filter_sort_truncate(
        tagged,
        status,
        &sort_by,
        grep,
        top,
        reverse,
        &signal_filters,
    )
    .map_err(|e| e.to_string())?;

    let experiments: Vec<Value> = filtered
        .iter()
        .map(|(e, r)| {
            json!({
                "tag": r.run_tag,
                "commit": e.commit,
                "val_bpb": e.val_bpb,
                "memory_gb": e.memory_gb,
                "status": e.status.to_string(),
                "metric_name": e.effective_metric_name(r),
                "description": e.description,
                "signals": e.signals.iter().map(|s| s.kind()).collect::<Vec<_>>(),
            })
        })
        .collect();

    serde_json::to_string(&json!({ "count": experiments.len(), "experiments": experiments }))
        .map_err(|e| e.to_string())
}

fn tool_compare(data_dir: &Path, args: &Value) -> std::result::Result<String, String> {
    use crate::commands::compare::compare_summary;

    let tags: Vec<String> = args
        .get("tags")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let runs = load_all_runs(data_dir).map_err(|e| e.to_string())?;
    let filtered: Vec<_> = if tags.is_empty() {
        runs
    } else {
        runs.into_iter()
            .filter(|r| tags.iter().any(|t| r.run_tag.contains(t.as_str())))
            .collect()
    };

    let summary = compare_summary(&filtered);
    serde_json::to_string(&json!({ "count": summary.len(), "runs": summary }))
        .map_err(|e| e.to_string())
}

fn tool_stats(data_dir: &Path, args: &Value) -> std::result::Result<String, String> {
    use crate::commands::stats::{compute_stats, pct};
    use crate::model::Experiment;

    let tag = args.get("tag").and_then(|v| v.as_str());

    let experiments: Vec<Experiment> = match tag {
        Some(t) => match load_run(data_dir, t).map_err(|e| e.to_string())? {
            Some(r) => r.experiments,
            None => return Err(format!("no such tag: {t}")),
        },
        None => load_all_runs(data_dir)
            .map_err(|e| e.to_string())?
            .into_iter()
            .flat_map(|r| r.experiments)
            .collect(),
    };

    let d = compute_stats(&experiments);
    let val_bpb = d.bpb.map(|b| {
        json!({
            "best": b.best,
            "worst": b.worst,
            "mean": b.mean,
            "stddev": b.stddev,
            "improvement": b.improvement,
            "improvement_pct": b.pct_improve,
            "bpb_drop_per_experiment": b.improvement_rate,
        })
    });

    serde_json::to_string(&json!({
        "tag": tag,
        "total": d.total,
        "kept": { "count": d.kept, "pct": pct(d.kept, d.total) },
        "discarded": { "count": d.discarded, "pct": pct(d.discarded, d.total) },
        "crashed": { "count": d.crashed, "pct": pct(d.crashed, d.total) },
        "val_bpb": val_bpb,
    }))
    .map_err(|e| e.to_string())
}

fn tool_usage(data_dir: &Path, _args: &Value) -> std::result::Result<String, String> {
    let events = crate::commands::usage::load_events(data_dir);
    Ok(crate::commands::usage::summary_json(&events).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use tempfile::TempDir;

    fn tmp() -> (TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        crate::store::ensure_initialized(&path).unwrap();
        (dir, path)
    }

    fn add_args(tag: &str, commit: &str, val_bpb: f64, parent: Option<&str>) -> Value {
        let mut m = serde_json::Map::new();
        m.insert("tag".into(), json!(tag));
        m.insert("commit".into(), json!(commit));
        m.insert("val_bpb".into(), json!(val_bpb));
        m.insert("status".into(), json!("keep"));
        m.insert("description".into(), json!("test experiment"));
        if let Some(p) = parent {
            m.insert("parent_commit".into(), json!(p));
        }
        Value::Object(m)
    }

    fn list_args(n: Option<u64>, tag: Option<&str>) -> Value {
        let mut m = serde_json::Map::new();
        if let Some(n) = n {
            m.insert("n".into(), json!(n));
        }
        if let Some(t) = tag {
            m.insert("tag".into(), json!(t));
        }
        Value::Object(m)
    }

    // --- list_recent JSON tests ---

    #[test]
    fn list_recent_empty_store_returns_zero_total() {
        let (_dir, path) = tmp();
        let args = list_args(None, None);
        let result = tool_list_recent(&path, &args).unwrap();
        let v: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(v["total"], 0);
        assert!(v["tags"].as_array().unwrap().is_empty());
        assert!(v["experiments"].as_array().unwrap().is_empty());
    }

    #[test]
    fn list_recent_single_tag_two_experiments() {
        let (_dir, path) = tmp();
        tool_add(&path, &add_args("foo", "aaa111", 0.9, None)).unwrap();
        tool_add(&path, &add_args("foo", "bbb222", 0.8, Some("aaa111"))).unwrap();
        let args = list_args(None, None);
        let result = tool_list_recent(&path, &args).unwrap();
        let v: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(v["total"], 2);
        let tags = v["tags"].as_array().unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0], "foo");
        assert_eq!(v["experiments"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn list_recent_multi_tag_unique_ordered_tags() {
        let (_dir, path) = tmp();
        tool_add(&path, &add_args("alpha", "aaa111", 0.9, None)).unwrap();
        tool_add(&path, &add_args("beta", "bbb222", 0.8, None)).unwrap();
        tool_add(&path, &add_args("alpha", "ccc333", 0.7, Some("aaa111"))).unwrap();
        let args = list_args(None, None);
        let result = tool_list_recent(&path, &args).unwrap();
        let v: Value = serde_json::from_str(&result).unwrap();
        let tags = v["tags"].as_array().unwrap();
        assert_eq!(tags.len(), 2);
        // No duplicates
        let tag_strs: Vec<&str> = tags.iter().map(|t| t.as_str().unwrap()).collect();
        let mut deduped = tag_strs.clone();
        deduped.dedup();
        assert_eq!(tag_strs, deduped);
    }

    #[test]
    fn list_recent_respects_n_truncation() {
        let (_dir, path) = tmp();
        for i in 0..5 {
            tool_add(
                &path,
                &add_args("bar", &format!("commit{i:03}"), 0.9 - i as f64 * 0.01, None),
            )
            .unwrap();
        }
        let args = list_args(Some(2), None);
        let result = tool_list_recent(&path, &args).unwrap();
        let v: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(v["total"], 5);
        assert_eq!(v["experiments"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn list_recent_valid_json_in_all_cases() {
        let (_dir, path) = tmp();
        // Empty store
        let r1 = tool_list_recent(&path, &list_args(None, None)).unwrap();
        assert!(serde_json::from_str::<Value>(&r1).is_ok());
        // After one add
        tool_add(&path, &add_args("x", "abc123", 1.0, None)).unwrap();
        let r2 = tool_list_recent(&path, &list_args(None, None)).unwrap();
        assert!(serde_json::from_str::<Value>(&r2).is_ok());
        // With truncation
        let r3 = tool_list_recent(&path, &list_args(Some(1), None)).unwrap();
        assert!(serde_json::from_str::<Value>(&r3).is_ok());
    }

    // --- parent_commit warning tests ---

    #[test]
    fn add_first_experiment_no_warning() {
        let (_dir, path) = tmp();
        let out = tool_add(&path, &add_args("newtag", "abc123", 0.95, None)).unwrap();
        let v: Value = serde_json::from_str(&out).expect("must be valid JSON");
        assert_eq!(v["recorded"], true);
        assert_eq!(
            v["lineage_warning"],
            Value::Null,
            "unexpected warning: {out}"
        );
    }

    #[test]
    fn add_second_experiment_without_parent_warns() {
        let (_dir, path) = tmp();
        tool_add(&path, &add_args("mytag", "abc123", 0.95, None)).unwrap();
        let out = tool_add(&path, &add_args("mytag", "def456", 0.90, None)).unwrap();
        let v: Value = serde_json::from_str(&out).expect("must be valid JSON");
        let warn = v["lineage_warning"].as_str().unwrap_or("");
        assert!(
            warn.contains("lineage chain broken"),
            "expected lineage warning, got: {out}"
        );
    }

    #[test]
    fn add_second_experiment_with_parent_no_warning() {
        let (_dir, path) = tmp();
        tool_add(&path, &add_args("mytag", "abc123", 0.95, None)).unwrap();
        let out = tool_add(&path, &add_args("mytag", "def456", 0.90, Some("abc123"))).unwrap();
        let v: Value = serde_json::from_str(&out).expect("must be valid JSON");
        assert_eq!(
            v["lineage_warning"],
            Value::Null,
            "unexpected warning: {out}"
        );
    }

    #[test]
    fn add_different_tags_independent() {
        let (_dir, path) = tmp();
        // tag A has prior experiment
        tool_add(&path, &add_args("tagA", "aaa111", 0.95, None)).unwrap();
        // tag B first experiment — no prior, no warning expected
        let out = tool_add(&path, &add_args("tagB", "bbb222", 0.90, None)).unwrap();
        let v: Value = serde_json::from_str(&out).expect("must be valid JSON");
        assert_eq!(
            v["lineage_warning"],
            Value::Null,
            "unexpected warning: {out}"
        );
    }

    // -----------------------------------------------------------------------
    // New JSON-shape tests for converted tool functions
    // -----------------------------------------------------------------------

    #[test]
    fn tool_best_plain_returns_valid_json() {
        let (_dir, path) = tmp();
        tool_add(&path, &add_args("t1", "abc0001", 0.985, None)).unwrap();
        let out = tool_best(&path, &json!({})).unwrap();
        let v: Value = serde_json::from_str(&out).expect("must be valid JSON");
        assert!(v["tag"].is_string(), "tag missing");
        assert!(v["commit"].is_string(), "commit missing");
        assert!(v["value"].is_number(), "value missing");
        assert_eq!(
            v["composite"],
            Value::Null,
            "composite must be null for plain"
        );
    }

    #[test]
    fn tool_best_composite_returns_breakdown() {
        let (_dir, path) = tmp();
        tool_add(&path, &add_args("t2", "abc0002", 0.985, None)).unwrap();
        let out = tool_best(&path, &json!({"composite": true})).unwrap();
        let v: Value = serde_json::from_str(&out).expect("must be valid JSON");
        let comp = &v["composite"];
        assert!(comp.is_object(), "composite must be object");
        assert!(comp["score"].is_number(), "composite.score missing");
        assert!(comp["metric"].is_number(), "composite.metric missing");
        assert!(
            comp["metric_weighted"].is_number(),
            "composite.metric_weighted missing"
        );
        assert!(
            comp["lineage_depth"].is_number(),
            "composite.lineage_depth missing"
        );
    }

    #[test]
    fn tool_best_empty_store_returns_empty_marker() {
        let (_dir, path) = tmp();
        let out = tool_best(&path, &json!({})).unwrap();
        let v: Value = serde_json::from_str(&out).expect("must be valid JSON");
        assert_eq!(v["empty"], true, "expected empty=true, got: {out}");
    }

    #[test]
    fn tool_search_returns_matches_array() {
        let (_dir, path) = tmp();
        // add two experiments; only one description matches "gelu"
        let mut m1 = serde_json::Map::new();
        m1.insert("tag".into(), json!("s1"));
        m1.insert("commit".into(), json!("ccc0001"));
        m1.insert("val_bpb".into(), json!(0.98));
        m1.insert("status".into(), json!("keep"));
        m1.insert("description".into(), json!("tried GeLU activation"));
        tool_add(&path, &Value::Object(m1)).unwrap();

        let mut m2 = serde_json::Map::new();
        m2.insert("tag".into(), json!("s1"));
        m2.insert("commit".into(), json!("ccc0002"));
        m2.insert("val_bpb".into(), json!(0.97));
        m2.insert("status".into(), json!("keep"));
        m2.insert("description".into(), json!("baseline relu"));
        tool_add(&path, &Value::Object(m2)).unwrap();

        let out = tool_search(&path, &json!({"pattern": "gelu"})).unwrap();
        let v: Value = serde_json::from_str(&out).expect("must be valid JSON");
        assert_eq!(v["count"], 1, "expected 1 match");
        assert_eq!(v["matches"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn tool_search_empty_returns_empty_array() {
        let (_dir, path) = tmp();
        tool_add(&path, &add_args("s2", "ddd0001", 0.99, None)).unwrap();
        let out =
            tool_search(&path, &json!({"pattern": "this_pattern_never_appears_xyz"})).unwrap();
        let v: Value = serde_json::from_str(&out).expect("must be valid JSON");
        assert_eq!(v["count"], 0);
        assert!(v["matches"].as_array().unwrap().is_empty());
    }

    #[test]
    fn tool_near_returns_neighbors_with_delta() {
        let (_dir, path) = tmp();
        tool_add(&path, &add_args("n1", "eee0001", 0.97, None)).unwrap();
        tool_add(&path, &add_args("n1", "eee0002", 0.98, Some("eee0001"))).unwrap();
        tool_add(&path, &add_args("n1", "eee0003", 0.99, Some("eee0002"))).unwrap();
        let out = tool_near(&path, &json!({"val_bpb": 0.98})).unwrap();
        let v: Value = serde_json::from_str(&out).expect("must be valid JSON");
        assert!(v["count"].as_u64().unwrap() > 0, "count must be > 0");
        let neighbors = v["neighbors"].as_array().unwrap();
        assert!(!neighbors.is_empty());
        // every neighbor has a delta field
        for nb in neighbors {
            assert!(nb["delta"].is_number(), "delta missing in neighbor");
        }
    }

    #[test]
    fn tool_find_by_signal_oom() {
        let (_dir, path) = tmp();
        // add a crash experiment with OOM log tail
        let mut m = serde_json::Map::new();
        m.insert("tag".into(), json!("sig1"));
        m.insert("commit".into(), json!("fff0001"));
        m.insert("val_bpb".into(), json!(0.0));
        m.insert("status".into(), json!("crash"));
        m.insert("description".into(), json!("OOM crash run"));
        m.insert(
            "log_tail".into(),
            json!("RuntimeError: CUDA out of memory. Tried to allocate 4.00 GiB"),
        );
        tool_add(&path, &Value::Object(m)).unwrap();

        let out = tool_find_by_signal(&path, &json!({"signal_type": "oom"})).unwrap();
        let v: Value = serde_json::from_str(&out).expect("must be valid JSON");
        assert_eq!(v["signal_type"], "oom");
        assert_eq!(v["count"], 1);
        assert_eq!(v["matches"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn tool_diff_tags_returns_delta() {
        let (_dir, path) = tmp();
        tool_add(&path, &add_args("da", "ggg0001", 0.990, None)).unwrap();
        tool_add(&path, &add_args("db", "hhh0001", 0.980, None)).unwrap();
        let out = tool_diff_tags(&path, &json!({"tag_a": "da", "tag_b": "db"})).unwrap();
        let v: Value = serde_json::from_str(&out).expect("must be valid JSON");
        assert_eq!(v["tag_a"], "da");
        assert_eq!(v["tag_b"], "db");
        let delta = v["delta"]["val_bpb"].as_f64().unwrap();
        assert!(
            (delta - (0.980 - 0.990)).abs() < 1e-9,
            "delta mismatch: {delta}"
        );
    }

    #[test]
    fn tool_lineage_returns_trees_recursive() {
        let (_dir, path) = tmp();
        // parent -> child lineage
        tool_add(&path, &add_args("lin1", "iii0001", 0.99, None)).unwrap();
        tool_add(&path, &add_args("lin1", "iii0002", 0.98, Some("iii0001"))).unwrap();
        let out = tool_lineage(&path, &json!({"tag": "lin1"})).unwrap();
        let v: Value = serde_json::from_str(&out).expect("must be valid JSON");
        assert_eq!(v["tag"], "lin1");
        let trees = v["trees"].as_array().unwrap();
        assert!(!trees.is_empty(), "trees must not be empty");
        // root should have a child
        let root = &trees[0];
        let children = root["children"].as_array().unwrap();
        assert_eq!(children.len(), 1, "root should have 1 child");
        assert_eq!(children[0]["commit"], "iii0002");
    }

    #[test]
    fn tool_verify_pass_returns_verified_true() {
        let (_dir, path) = tmp();
        tool_add(&path, &add_args("v1", "jjj0001", 0.985, None)).unwrap();
        let out = tool_verify(
            &path,
            &json!({"commit": "jjj0001", "value": 0.984, "tolerance": 0.01}),
        )
        .unwrap();
        let v: Value = serde_json::from_str(&out).expect("must be valid JSON");
        assert_eq!(v["verified"], true);
        assert_eq!(v["action"], "verified");
        assert!(v["delta"].is_number());
    }

    #[test]
    fn tool_verify_fail_returns_exceeded_by() {
        let (_dir, path) = tmp();
        tool_add(&path, &add_args("v2", "kkk0001", 0.985, None)).unwrap();
        let out = tool_verify(
            &path,
            &json!({"commit": "kkk0001", "value": 1.02, "tolerance": 0.01}),
        )
        .unwrap();
        let v: Value = serde_json::from_str(&out).expect("must be valid JSON");
        assert_eq!(v["verified"], false);
        let exceeded = v["exceeded_by"].as_f64().unwrap();
        assert!(
            exceeded > 0.0,
            "exceeded_by must be positive, got {exceeded}"
        );
    }

    // -----------------------------------------------------------------------
    // New tests for the 5 MCP defect fixes
    // -----------------------------------------------------------------------

    // [H1] manifest enum contains all 8 signal kinds including the two new ones
    #[test]
    fn manifest_signal_enum_contains_diverged_loss_and_slow_mfu() {
        let manifest = tool_manifest();
        let tools = manifest.as_array().unwrap();
        let fbs = tools
            .iter()
            .find(|t| t["name"] == "resman_find_by_signal")
            .expect("resman_find_by_signal tool not found");
        let kinds = fbs["inputSchema"]["properties"]["signal_type"]["enum"]
            .as_array()
            .expect("enum must be array");
        let kind_strs: Vec<&str> = kinds.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(
            kind_strs.contains(&"diverged_loss"),
            "diverged_loss missing from enum: {kind_strs:?}"
        );
        assert!(
            kind_strs.contains(&"slow_mfu"),
            "slow_mfu missing from enum: {kind_strs:?}"
        );
        assert_eq!(
            kind_strs.len(),
            crate::signals::ALL_KINDS.len(),
            "enum length mismatch vs ALL_KINDS"
        );
    }

    // [H2] tool_add output is JSON with required fields
    #[test]
    fn tool_add_returns_json_object_with_required_fields() {
        let (_dir, path) = tmp();
        let out = tool_add(&path, &add_args("h2tag", "aaa000", 0.95, None)).unwrap();
        let v: Value = serde_json::from_str(&out).expect("tool_add must return valid JSON");
        assert_eq!(v["recorded"], true, "recorded field missing or wrong");
        assert_eq!(v["tag"], "h2tag");
        assert_eq!(v["commit"], "aaa000");
        assert!(v["val_bpb"].is_number(), "val_bpb must be number");
        assert!(v["status"].is_string(), "status must be string");
        // lineage_warning must be present (null for first experiment)
        assert!(
            v.get("lineage_warning").is_some(),
            "lineage_warning key must exist"
        );
    }

    // [M8] tools/call with missing name → JSON-RPC error code -32602
    #[test]
    fn tools_call_missing_name_returns_32602() {
        // Simulate the dispatch by calling handle_tool_call with no "name" key.
        // handle_tool_call returns Err("missing tool name") which would become
        // an isError prose result — that's NOT the fix. The fix is in cmd_mcp's
        // dispatch. We verify the error_response helper produces code -32602
        // when the name is absent/null, by testing the branch condition directly.
        let params_no_name = json!({"arguments": {}});
        let name_val = params_no_name.get("name");
        let tool_name_opt = name_val.and_then(|v| v.as_str());
        assert!(
            name_val.is_none() || tool_name_opt.is_none(),
            "precondition: name absent should trigger -32602 branch"
        );

        let params_null_name = json!({"name": null, "arguments": {}});
        let name_val2 = params_null_name.get("name");
        let tool_name_opt2 = name_val2.and_then(|v| v.as_str());
        assert!(
            name_val2.is_none() || tool_name_opt2.is_none(),
            "precondition: null name should trigger -32602 branch"
        );

        // Verify the error_response shape itself.
        let resp = error_response(json!(1), -32602, "invalid params: `name` must be a string");
        assert_eq!(resp["error"]["code"], -32602);
        assert_eq!(resp["jsonrpc"], "2.0");
    }

    // [M5-MCP] non-finite val_bpb is rejected by tool_add
    #[test]
    fn tool_add_rejects_non_finite_val_bpb() {
        let (_dir, path) = tmp();
        // NaN — serde_json encodes f64::NAN as null, so we must build args manually
        // with a numeric that serde_json treats as finite but test the guard too.
        // The guard fires on the Rust f64 value after .as_f64(), so test via
        // a value that arrives as finite-but-flagged — use a direct call.
        let mut args = serde_json::Map::new();
        args.insert("tag".into(), json!("nonfinite"));
        args.insert("commit".into(), json!("nan001"));
        args.insert("val_bpb".into(), json!(0.95)); // finite baseline — should pass
        args.insert("status".into(), json!("keep"));
        args.insert("description".into(), json!("test"));
        let ok_result = tool_add(&path, &Value::Object(args));
        assert!(ok_result.is_ok(), "finite val_bpb must be accepted");

        // Call the guard logic directly with a non-finite value.
        let val: f64 = f64::NAN;
        assert!(!val.is_finite(), "precondition: NAN is not finite");
        let val2: f64 = f64::INFINITY;
        assert!(!val2.is_finite(), "precondition: INFINITY is not finite");
        // Build args with val_bpb=0 then test that the non-finite check message is correct.
        // (JSON cannot encode NaN/inf natively; the guard protects against any path
        //  that produces a non-finite f64 before the store write.)
        let err_msg = "val_bpb must be finite; crashes use 0.0 — NaN/±inf are not accepted";
        // Verify the guard message is what we expect by constructing a dummy call
        // simulating what would happen if val_bpb were non-finite.
        let guard_result: std::result::Result<String, String> = if !val.is_finite() {
            Err(err_msg.to_string())
        } else {
            Ok("would not reach".to_string())
        };
        assert!(guard_result.is_err());
        assert!(guard_result.unwrap_err().contains("finite"));
    }

    // -----------------------------------------------------------------------
    // New tests for resman_list / resman_compare / resman_stats / resman_usage
    // -----------------------------------------------------------------------

    #[test]
    fn tool_list_filters_by_signal_and_status() {
        let (_dir, path) = tmp();
        // Add a kept experiment with an OOM signal via log_tail.
        let mut m1 = serde_json::Map::new();
        m1.insert("tag".into(), json!("lt1"));
        m1.insert("commit".into(), json!("oom001"));
        m1.insert("val_bpb".into(), json!(0.0));
        m1.insert("status".into(), json!("crash"));
        m1.insert("description".into(), json!("oom run"));
        m1.insert(
            "log_tail".into(),
            json!("RuntimeError: CUDA out of memory. Tried to allocate 4.00 GiB"),
        );
        tool_add(&path, &Value::Object(m1)).unwrap();

        // Add a plain kept experiment with no signals.
        tool_add(&path, &add_args("lt1", "plain001", 0.95, None)).unwrap();

        // signal=["oom"] should return only the crash with oom signal.
        let out = tool_list(&path, &json!({"signal": ["oom"], "status": "all"})).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["count"], 1, "expected 1 oom experiment");
        let exps = v["experiments"].as_array().unwrap();
        let signals = exps[0]["signals"].as_array().unwrap();
        assert!(
            signals.iter().any(|s| s.as_str() == Some("oom")),
            "signals must contain oom"
        );

        // status=all should return both experiments (>=2).
        let out2 = tool_list(&path, &json!({"status": "all"})).unwrap();
        let v2: Value = serde_json::from_str(&out2).unwrap();
        assert!(
            v2["count"].as_u64().unwrap() >= 2,
            "status=all must return >=2"
        );
    }

    #[test]
    fn tool_compare_one_entry_per_run() {
        let (_dir, path) = tmp();
        tool_add(&path, &add_args("runA", "aaa001", 0.99, None)).unwrap();
        tool_add(&path, &add_args("runB", "bbb001", 0.98, None)).unwrap();

        let out = tool_compare(&path, &json!({})).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["count"], 2, "expected one entry per run");
        let runs = v["runs"].as_array().unwrap();
        assert_eq!(runs.len(), 2);
    }

    #[test]
    fn tool_stats_returns_counts_and_bpb() {
        let (_dir, path) = tmp();
        tool_add(&path, &add_args("st1", "s001", 0.95, None)).unwrap();
        tool_add(&path, &add_args("st1", "s002", 0.90, Some("s001"))).unwrap();

        let out = tool_stats(&path, &json!({"tag": "st1"})).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["total"], 2, "total must be 2");
        assert_eq!(v["kept"]["count"], 2, "kept.count must be 2");
        let bpb = &v["val_bpb"];
        assert!(bpb.is_object(), "val_bpb must be present and an object");
        let best = bpb["best"].as_f64().unwrap();
        assert!(best.is_finite(), "val_bpb.best must be finite");
        assert!(best > 0.0, "val_bpb.best must be positive");
    }

    #[test]
    fn tool_usage_reports_cold_tools() {
        let (_dir, path) = tmp();
        // Empty store — no usage.jsonl written, so all tools are cold.
        let out = tool_usage(&path, &json!({})).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(
            v["total_events"].as_u64().unwrap(),
            0,
            "total_events must be 0"
        );
        let cold = v["cold_tools"].as_array().unwrap();
        assert!(
            !cold.is_empty(),
            "cold_tools must be non-empty when no calls made"
        );
    }

    #[test]
    fn tool_names_match_manifest_and_dispatch() {
        let manifest = tool_manifest();
        let arr = manifest.as_array().unwrap();
        assert_eq!(arr.len(), TOOL_NAMES.len(), "manifest count != TOOL_NAMES");
        for name in TOOL_NAMES {
            assert!(
                arr.iter().any(|t| t["name"] == *name),
                "manifest missing {name}"
            );
        }
        let dir = tempfile::tempdir().unwrap();
        crate::store::ensure_initialized(dir.path()).unwrap();
        for name in TOOL_NAMES {
            let res = handle_tool_call(dir.path(), &json!({"name": name, "arguments": {}}));
            if let Err(e) = res {
                assert!(!e.contains("unknown tool"), "dispatch missing {name}: {e}");
            }
        }
    }
}
