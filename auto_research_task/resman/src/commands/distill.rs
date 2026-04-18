//! `resman distill -t <tag>` — structured Markdown/JSON summary of a run.
//!
//! Produces a "what did we learn last night?" artifact: best result, lineage,
//! failure signals, unexplored neighbors, and heuristic suggestions. No LLM,
//! no extra crates — pure template rendering over the existing RunLog schema.

use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::model::{Direction, Experiment, RunLog, Status};
use crate::store::{load_run, truncate};

// ---------------------------------------------------------------------------
// Report data types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistillSummary {
    pub total: usize,
    pub keep: usize,
    pub discard: usize,
    pub crash: usize,
    pub best: usize,
    pub metric_name: String,
    pub direction: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistillBest {
    pub commit: String,
    pub value: f64,
    pub description: String,
    pub gpu: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageEntry {
    pub commit: String,
    pub status: String,
    pub metric: f64,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureSignalEntry {
    pub commit: String,
    pub description: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeighborEntry {
    pub commit: String,
    pub value: f64,
    pub delta: f64,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistillReport {
    pub tag: String,
    pub generated_at: String,
    pub summary: DistillSummary,
    pub best: Option<DistillBest>,
    pub lineage: Vec<LineageEntry>,
    pub failure_signals: HashMap<String, Vec<FailureSignalEntry>>,
    pub unexplored_neighbors: Vec<NeighborEntry>,
    pub suggestions: Vec<String>,
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

fn status_glyph(s: Status) -> &'static str {
    match s {
        Status::Keep => "✓",
        Status::Best => "★",
        Status::Discard => "✗",
        Status::Crash => "💥",
        Status::Verified => "✔",
    }
}

fn short_commit(c: &str) -> &str {
    let len = c.len().min(8);
    &c[..len]
}

pub fn build_distill(run: &RunLog) -> DistillReport {
    let tag = run.run_tag.clone();
    let generated_at = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    // --- summary counts ---
    let total = run.experiments.len();
    let keep = run
        .experiments
        .iter()
        .filter(|e| e.status == Status::Keep)
        .count();
    let discard = run
        .experiments
        .iter()
        .filter(|e| e.status == Status::Discard)
        .count();
    let crash = run
        .experiments
        .iter()
        .filter(|e| e.status == Status::Crash)
        .count();
    let best_count = run
        .experiments
        .iter()
        .filter(|e| e.status == Status::Best)
        .count();

    // Determine effective metric name and direction from run or first experiment.
    let direction = run
        .metric_direction
        .or_else(|| run.experiments.first().and_then(|e| e.metric_direction))
        .unwrap_or(Direction::Minimize);
    let metric_name = run
        .metric_name
        .clone()
        .or_else(|| run.experiments.first().and_then(|e| e.metric_name.clone()))
        .unwrap_or_else(|| "val_bpb".to_string());

    let summary = DistillSummary {
        total,
        keep,
        discard,
        crash,
        best: best_count,
        metric_name: metric_name.clone(),
        direction: direction.as_str().to_string(),
    };

    // --- best experiment ---
    let best_exp = run.best();
    let best = best_exp.map(|e| DistillBest {
        commit: e.commit.clone(),
        value: e.val_bpb,
        description: e.description.clone(),
        gpu: String::new(), // GPU info not stored in Experiment; leave empty
    });

    // --- lineage to best ---
    let lineage = if let Some(b) = best_exp {
        build_lineage(run, b)
    } else {
        vec![]
    };

    // --- failure signals ---
    let mut failure_signals: HashMap<String, Vec<FailureSignalEntry>> = HashMap::new();
    for kind in crate::signals::ALL_KINDS {
        failure_signals.insert(kind.to_string(), vec![]);
    }
    for e in &run.experiments {
        for sig in &e.signals {
            let kind = sig.kind();
            let detail = signal_detail(sig);
            let entry = FailureSignalEntry {
                commit: e.commit.clone(),
                description: truncate(&e.description, 60),
                detail,
            };
            failure_signals
                .entry(kind.to_string())
                .or_default()
                .push(entry);
        }
    }

    // --- unexplored neighbors (3 closest to best that aren't best) ---
    let unexplored_neighbors = if let Some(b) = best_exp {
        build_neighbors(run, b, direction, &metric_name)
    } else {
        vec![]
    };

    // --- suggestions ---
    let suggestions = build_suggestions(run, &failure_signals, total, crash, best_exp, &tag);

    DistillReport {
        tag,
        generated_at,
        summary,
        best,
        lineage,
        failure_signals,
        unexplored_neighbors,
        suggestions,
    }
}

fn signal_detail(sig: &crate::signals::Signal) -> String {
    use crate::signals::Signal::*;
    match sig {
        CudaError { hint } if !hint.is_empty() => format!("hint: {hint}"),
        AssertFail { location } if !location.is_empty() => format!("at {location}"),
        Unknown { pattern } if !pattern.is_empty() => format!("matched: {pattern}"),
        _ => String::new(),
    }
}

fn build_lineage(run: &RunLog, best: &Experiment) -> Vec<LineageEntry> {
    // Build commit → experiment index map for fast lookup.
    let commit_map: HashMap<&str, &Experiment> = run
        .experiments
        .iter()
        .map(|e| (e.commit.as_str(), e))
        .collect();

    let mut chain: Vec<&Experiment> = vec![best];
    let mut visited: HashSet<&str> = HashSet::new();
    visited.insert(&best.commit);

    // Walk backwards through parent_commit links.
    let mut current = best;
    let mut depth = 0;
    loop {
        if depth >= 64 {
            break; // cycle-guard / pathological depth
        }
        match &current.parent_commit {
            None => break,
            Some(parent) => {
                let parent_str = parent.as_str();
                if visited.contains(parent_str) {
                    break; // cycle detected
                }
                match commit_map.get(parent_str) {
                    None => break, // parent outside this run
                    Some(parent_exp) => {
                        visited.insert(parent_str);
                        chain.push(parent_exp);
                        current = parent_exp;
                        depth += 1;
                    }
                }
            }
        }
    }

    // chain is [best, parent, grandparent, ...]; reverse to root→best order.
    chain.reverse();
    chain
        .into_iter()
        .map(|e| LineageEntry {
            commit: e.commit.clone(),
            status: e.status.as_str().to_string(),
            metric: e.val_bpb,
            description: e.description.clone(),
        })
        .collect()
}

fn build_neighbors(
    run: &RunLog,
    best: &Experiment,
    direction: Direction,
    _metric_name: &str,
) -> Vec<NeighborEntry> {
    let best_val = best.val_bpb;
    let mut candidates: Vec<&Experiment> = run
        .experiments
        .iter()
        .filter(|e| e.status != Status::Best && e.val_bpb > 0.0)
        .collect();

    // Sort by absolute distance to best value.
    candidates.sort_by(|a, b| {
        (a.val_bpb - best_val)
            .abs()
            .partial_cmp(&(b.val_bpb - best_val).abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    candidates.truncate(3);

    candidates
        .into_iter()
        .map(|e| {
            let delta = match direction {
                Direction::Minimize => best_val - e.val_bpb,
                Direction::Maximize => e.val_bpb - best_val,
            };
            NeighborEntry {
                commit: e.commit.clone(),
                value: e.val_bpb,
                delta,
                description: truncate(&e.description, 60),
            }
        })
        .collect()
}

fn build_suggestions(
    run: &RunLog,
    failure_signals: &HashMap<String, Vec<FailureSignalEntry>>,
    total: usize,
    crash: usize,
    best_exp: Option<&Experiment>,
    tag: &str,
) -> Vec<String> {
    let mut suggestions: Vec<String> = Vec::new();

    let oom_count = failure_signals.get("oom").map(|v| v.len()).unwrap_or(0);
    let nan_count = failure_signals
        .get("nan_loss")
        .map(|v| v.len())
        .unwrap_or(0);

    // Suggestion 1: OOMs
    if oom_count >= 3 {
        let pct = if total > 0 {
            oom_count * 100 / total
        } else {
            0
        };
        suggestions.push(format!(
            "Consider reducing batch size or enabling gradient checkpointing — OOMs account for {pct}% of failures."
        ));
    }

    // Suggestion 2: NaN losses
    if nan_count >= 2 {
        suggestions.push(
            "Numerical instability detected — consider lowering LR, gradient clipping, or fp32 accumulations.".to_string(),
        );
    }

    // Suggestion 3: best has no parent_commit
    if let Some(b) = best_exp
        && b.parent_commit.is_none()
    {
        suggestions.push(
            "Best result has no recorded parent — consider running `resman add --parent <commit>` going forward to enable trend tracking.".to_string(),
        );
    }

    // Suggestion 4: run stalled (last half all discards, no keeps)
    if total >= 2 {
        let half_start = total / 2;
        let last_half = &run.experiments[half_start..];
        let recent_discards = last_half
            .iter()
            .filter(|e| e.status == Status::Discard)
            .count();
        let recent_keeps = last_half.iter().filter(|e| e.status.is_kept()).count();
        if recent_discards >= 5 && recent_keeps == 0 {
            suggestions.push(
                "Run has stalled — recent experiments all discarded. Consider a new direction or revisit the best commit.".to_string(),
            );
        }
    }

    // Suggestion 5: high crash rate
    if total > 0 && crash * 10 > total * 3 {
        // crash/total > 0.3
        let pct = crash * 100 / total;
        // Find most common signal kind.
        let most_common = crate::signals::ALL_KINDS
            .iter()
            .max_by_key(|k| failure_signals.get(**k).map(|v| v.len()).unwrap_or(0))
            .copied()
            .unwrap_or("unknown");
        suggestions.push(format!(
            "High crash rate ({pct}%). Investigate `resman list -t {tag} --signal {most_common}` before adding more experiments."
        ));
    }

    // Suggestion 6: duplicate descriptions
    if total > 1 {
        let unique_descs: HashSet<&str> = run
            .experiments
            .iter()
            .map(|e| e.description.as_str())
            .collect();
        if unique_descs.len() < total / 2 {
            suggestions.push(
                "Many duplicate descriptions — use `resman search` before adding to avoid repeating ideas.".to_string(),
            );
        }
    }

    suggestions
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

pub fn render_markdown(report: &DistillReport) -> String {
    let mut out = String::new();
    let s = &report.summary;

    out.push_str(&format!("# Distill: {}\n\n", report.tag));
    out.push_str(&format!(
        "_Generated from {} experiments ({} crashes, {} keep, {} discard, {} best). Metric: {} ({})._\n\n",
        s.total, s.crash, s.keep, s.discard, s.best, s.metric_name, s.direction
    ));

    // --- Best result ---
    out.push_str("## Best result\n");
    match &report.best {
        None => out.push_str("_no kept experiments in this run_\n"),
        Some(b) => {
            out.push_str(&format!("- **{}**: `{:.6}`\n", s.metric_name, b.value));
            out.push_str(&format!("- **commit**: `{}`\n", short_commit(&b.commit)));
            out.push_str(&format!("- **description**: {}\n", b.description));
            let gpu = if b.gpu.is_empty() {
                "unspecified"
            } else {
                &b.gpu
            };
            out.push_str(&format!("- **GPU**: {gpu}\n"));
        }
    }
    out.push('\n');

    // --- Lineage ---
    out.push_str("## Lineage to best\n");
    if report.lineage.is_empty() {
        out.push_str("_no lineage recorded_\n");
    } else {
        for entry in &report.lineage {
            let status: Status = entry.status.parse().unwrap_or(Status::Keep);
            let glyph = status_glyph(status);
            out.push_str(&format!(
                "  `{}` {} {}={:.6}  {}\n",
                short_commit(&entry.commit),
                glyph,
                s.metric_name,
                entry.metric,
                truncate(&entry.description, 60)
            ));
        }
    }
    out.push('\n');

    // --- Failure signals ---
    out.push_str("## Failure signals\n\n");
    let any_signals = report.failure_signals.values().any(|v| !v.is_empty());
    if !any_signals {
        out.push_str("_no crash signals recorded in this run_\n");
    } else {
        for kind in crate::signals::ALL_KINDS {
            let entries = match report.failure_signals.get(*kind) {
                Some(v) if !v.is_empty() => v,
                _ => continue,
            };
            out.push_str(&format!("### {} ({})\n", kind, entries.len()));
            for e in entries {
                let detail = if e.detail.is_empty() {
                    String::new()
                } else {
                    format!(" — {}", e.detail)
                };
                out.push_str(&format!(
                    "- `{}` — {}{}\n",
                    short_commit(&e.commit),
                    e.description,
                    detail
                ));
            }
            out.push('\n');
        }
    }
    out.push('\n');

    // --- Unexplored neighbors ---
    out.push_str("## Unexplored neighbors\n");
    if report.unexplored_neighbors.is_empty() {
        out.push_str("_no neighbors found_\n");
    } else {
        for n in &report.unexplored_neighbors {
            out.push_str(&format!(
                "- `{}` {}={:.6} (Δ={:+.4}) — {}\n",
                short_commit(&n.commit),
                s.metric_name,
                n.value,
                n.delta,
                n.description
            ));
        }
    }
    out.push('\n');

    // --- Suggestions ---
    out.push_str("## Suggestions\n");
    if report.suggestions.is_empty() {
        out.push_str("_no mechanical suggestions — run looks healthy._\n");
    } else {
        for (i, sug) in report.suggestions.iter().enumerate() {
            out.push_str(&format!("{}. {}\n", i + 1, sug));
        }
    }
    out.push('\n');

    out.push_str("---\n");
    out.push_str(&format!(
        "_resman distill v0.6 — {}_\n",
        report.generated_at
    ));

    out
}

// ---------------------------------------------------------------------------
// Command entry point
// ---------------------------------------------------------------------------

pub fn cmd_distill(
    data_dir: &Path,
    tag: &str,
    out_path: Option<&std::path::Path>,
    format: &DistillFormat,
) -> Result<()> {
    let run = load_run(data_dir, tag)?.ok_or_else(|| {
        Error::NotFound(crate::store::runs_dir(data_dir).join(format!("{tag}.json")))
    })?;

    let report = build_distill(&run);

    let rendered = match format {
        DistillFormat::Markdown => render_markdown(&report),
        DistillFormat::Json => serde_json::to_string_pretty(&report)?,
    };

    match out_path {
        Some(p) => std::fs::write(p, &rendered)?,
        None => print!("{rendered}"),
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// MCP helper (shared with mcp.rs)
// ---------------------------------------------------------------------------

/// Load a run and return distill output as a string. Used by the MCP tool.
pub fn distill_to_string(
    data_dir: &Path,
    tag: &str,
    json: bool,
) -> std::result::Result<String, String> {
    let run = load_run(data_dir, tag)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("no such tag: {tag}"))?;
    let report = build_distill(&run);
    if json {
        serde_json::to_string_pretty(&report).map_err(|e| e.to_string())
    } else {
        Ok(render_markdown(&report))
    }
}

// ---------------------------------------------------------------------------
// CLI format enum (used by cli.rs)
// ---------------------------------------------------------------------------

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum DistillFormat {
    /// Markdown report (default)
    Markdown,
    /// JSON — for programmatic use and MCP parity
    Json,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{RunLog, Status};
    use crate::signals::Signal;
    use std::collections::HashMap;

    fn make_exp(
        commit: &str,
        val_bpb: f64,
        status: Status,
        desc: &str,
        parent: Option<&str>,
        signals: Vec<Signal>,
    ) -> Experiment {
        Experiment {
            commit: commit.to_string(),
            val_bpb,
            memory_gb: 0.0,
            status,
            description: desc.to_string(),
            timestamp: String::new(),
            params: HashMap::new(),
            parent_commit: parent.map(|s| s.to_string()),
            crash_excerpt: None,
            metric_name: None,
            metric_direction: None,
            signals,
        }
    }

    fn make_run(tag: &str, exps: Vec<Experiment>) -> RunLog {
        RunLog {
            run_tag: tag.to_string(),
            created_at: String::new(),
            experiments: exps,
            metric_name: None,
            metric_direction: None,
        }
    }

    /// Test 1: empty run produces sensible zero-value report.
    /// Suggestions list may be empty (no heuristics trigger on zero experiments).
    #[test]
    fn build_distill_empty_run() {
        let run = make_run("empty", vec![]);
        let report = build_distill(&run);
        assert_eq!(report.summary.total, 0);
        assert!(report.best.is_none());
        assert!(report.lineage.is_empty());
        // No heuristics trigger on zero experiments — suggestions may be empty.
        // Document: suggestion 3 (no parent) only fires when best exists, so
        // no suggestions expected here.
        // Just verify it doesn't panic and returns a well-formed report.
        assert_eq!(report.tag, "empty");
    }

    /// Test 2: signal grouping — 3 OOMs and 1 NaN are classified correctly.
    #[test]
    fn build_distill_groups_signals() {
        let run = make_run(
            "signals",
            vec![
                make_exp("a1", 0.0, Status::Crash, "oom1", None, vec![Signal::Oom]),
                make_exp("a2", 0.0, Status::Crash, "oom2", None, vec![Signal::Oom]),
                make_exp("a3", 0.0, Status::Crash, "oom3", None, vec![Signal::Oom]),
                make_exp("a4", 0.0, Status::Crash, "nan", None, vec![Signal::NanLoss]),
            ],
        );
        let report = build_distill(&run);
        assert_eq!(report.failure_signals.get("oom").map(|v| v.len()), Some(3));
        assert_eq!(
            report.failure_signals.get("nan_loss").map(|v| v.len()),
            Some(1)
        );
    }

    /// Test 3: lineage walk returns 4 entries root→best in correct order.
    #[test]
    fn build_distill_lineage_to_best() {
        // Chain: root (a0) → a1 → a2 → best (a3), each pointing to the previous.
        let run = make_run(
            "lineage",
            vec![
                make_exp("a0", 1.0, Status::Keep, "root", None, vec![]),
                make_exp("a1", 0.9, Status::Keep, "step1", Some("a0"), vec![]),
                make_exp("a2", 0.8, Status::Keep, "step2", Some("a1"), vec![]),
                make_exp("a3", 0.7, Status::Best, "best", Some("a2"), vec![]),
            ],
        );
        let report = build_distill(&run);
        // Lineage should be [a0, a1, a2, a3] — root to best.
        assert_eq!(report.lineage.len(), 4, "expected 4 lineage entries");
        assert_eq!(report.lineage[0].commit, "a0");
        assert_eq!(report.lineage[3].commit, "a3");
    }

    /// Test 4: render_markdown produces all required section headers.
    #[test]
    fn render_markdown_contains_sections() {
        let run = make_run(
            "test",
            vec![
                make_exp("abc1234", 0.95, Status::Keep, "baseline", None, vec![]),
                make_exp(
                    "def5678",
                    0.0,
                    Status::Crash,
                    "oom run",
                    None,
                    vec![Signal::Oom],
                ),
            ],
        );
        let report = build_distill(&run);
        let md = render_markdown(&report);
        assert!(md.contains("## Best result"), "missing '## Best result'");
        assert!(
            md.contains("## Failure signals"),
            "missing '## Failure signals'"
        );
        assert!(md.contains("## Suggestions"), "missing '## Suggestions'");
        assert!(
            md.contains("## Lineage to best"),
            "missing '## Lineage to best'"
        );
        assert!(
            md.contains("## Unexplored neighbors"),
            "missing '## Unexplored neighbors'"
        );
    }
}
