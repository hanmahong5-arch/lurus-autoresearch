//! `resman distill -t <tag>` — structured Markdown/JSON summary of a run.
//!
//! Produces a "what did we learn last night?" artifact: best result, lineage,
//! failure signals, unexplored neighbors, and heuristic suggestions. No LLM,
//! no extra crates — pure template rendering over the existing RunLog schema.

use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::html::{BadgeKind, badge, html_escape, trend_svg};
use crate::model::{Direction, Experiment, RunLog, Status};
use crate::store::{load_run, load_run_or_suggest, truncate};

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

fn status_glyph(s: Status) -> String {
    crate::term::status_glyph(&s)
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
// HTML rendering
// ---------------------------------------------------------------------------

fn status_badge(status_str: &str) -> String {
    let kind = match status_str {
        "keep" => BadgeKind::Keep,
        "best" => BadgeKind::Best,
        "verified" => BadgeKind::Verified,
        "crash" => BadgeKind::Crash,
        "discard" => BadgeKind::Discard,
        _ => BadgeKind::Neutral,
    };
    badge(status_str, kind)
}

/// Render the distill report as a self-contained HTML string.
/// Pure function — no IO, no ANSI escapes.
pub fn render_html(report: &DistillReport) -> String {
    let s = &report.summary;

    // --- header badges ---
    let mut header_badges = String::new();
    if s.keep > 0 {
        header_badges.push_str(&badge(&format!("{} keep", s.keep), BadgeKind::Keep));
    }
    if s.best > 0 {
        header_badges.push_str(&badge(&format!("{} best", s.best), BadgeKind::Best));
    }
    if s.crash > 0 {
        header_badges.push_str(&badge(&format!("{} crash", s.crash), BadgeKind::Crash));
    }
    if s.discard > 0 {
        header_badges.push_str(&badge(
            &format!("{} discard", s.discard),
            BadgeKind::Discard,
        ));
    }
    let verified_count = report
        .lineage
        .iter()
        .filter(|e| e.status == "verified")
        .count();
    if verified_count > 0 {
        header_badges.push_str(&badge(
            &format!("{verified_count} verified"),
            BadgeKind::Verified,
        ));
    }
    header_badges.push_str(&badge(
        &format!("{} ({})", s.metric_name, s.direction),
        BadgeKind::Neutral,
    ));

    let mut body = format!(
        "<h1>Distill &mdash; {tag}</h1>\n\
         <div class=\"sub\">{gen} &middot; {total} experiments {badges}</div>\n",
        tag = html_escape(&report.tag),
        gen = html_escape(&report.generated_at),
        total = s.total,
        badges = header_badges,
    );

    // --- sparkline ---
    let kept_points: Vec<(usize, f64)> = report
        .lineage
        .iter()
        .enumerate()
        .filter(|(_, e)| e.metric > 0.0)
        .map(|(i, e)| (i, e.metric))
        .collect();
    if kept_points.len() >= 2 {
        let svg = trend_svg(&kept_points, 1040, 280);
        body.push_str("<h2>Metric trajectory</h2>\n");
        body.push_str(&format!("<div class=\"chart\">{svg}</div>\n"));
    }

    // --- best card ---
    body.push_str("<h2>Best result</h2>\n");
    match &report.best {
        None => {
            body.push_str("<div class=\"no-best\">No best experiment in this run.</div>\n");
        }
        Some(b) => {
            let gpu_line = if b.gpu.is_empty() {
                String::new()
            } else {
                format!("<div class=\"gpu\">GPU: {}</div>", html_escape(&b.gpu))
            };
            body.push_str(&format!(
                "<section class=\"best-card\">\
                   <div class=\"metric\">{metric_name}: {value:.6}</div>\
                   <div class=\"commit-hash\"><code>{commit}</code></div>\
                   <div class=\"desc\">{desc}</div>\
                   {gpu_line}\
                 </section>\n",
                metric_name = html_escape(&s.metric_name),
                value = b.value,
                commit = html_escape(&b.commit),
                desc = html_escape(&b.description),
            ));
        }
    }

    // --- lineage ---
    body.push_str("<h2>Lineage to best</h2>\n");
    if report.lineage.is_empty() {
        body.push_str("<p style=\"color:#6b7280\"><em>no lineage recorded</em></p>\n");
    } else {
        body.push_str("<ol>\n");
        for entry in &report.lineage {
            let sc = short_commit(&entry.commit);
            body.push_str(&format!(
                "<li>{status_badge} <code>{commit}</code> &mdash; \
                 {metric_name}={value:.6} &mdash; {desc}</li>\n",
                status_badge = status_badge(&entry.status),
                commit = html_escape(sc),
                metric_name = html_escape(&s.metric_name),
                value = entry.metric,
                desc = html_escape(&truncate(&entry.description, 80)),
            ));
        }
        body.push_str("</ol>\n");
    }

    // --- failure signals ---
    body.push_str("<h2>Failure signals</h2>\n");
    let any_signals = report.failure_signals.values().any(|v| !v.is_empty());
    if !any_signals {
        body.push_str(
            "<p style=\"color:#6b7280\"><em>no crash signals recorded in this run</em></p>\n",
        );
    } else {
        // Sort kinds by count desc.
        let mut kinds: Vec<(&String, &Vec<FailureSignalEntry>)> = report
            .failure_signals
            .iter()
            .filter(|(_, v)| !v.is_empty())
            .collect();
        kinds.sort_by(|a, b| b.1.len().cmp(&a.1.len()));

        for (kind, entries) in &kinds {
            let mut items = String::new();
            for e in *entries {
                let detail = if e.detail.is_empty() {
                    String::new()
                } else {
                    format!(" &mdash; {}", html_escape(&e.detail))
                };
                items.push_str(&format!(
                    "<li><code>{commit}</code> &mdash; {desc}{detail}</li>\n",
                    commit = html_escape(short_commit(&e.commit)),
                    desc = html_escape(&e.description),
                ));
            }
            body.push_str(&format!(
                "<div class=\"signal-cluster\">\
                   <details>\
                     <summary>{kind} &times; {count}</summary>\
                     <ul>{items}</ul>\
                   </details>\
                 </div>\n",
                kind = html_escape(kind),
                count = entries.len(),
            ));
        }
    }

    // --- unexplored neighbors ---
    body.push_str("<h2>Unexplored neighbors</h2>\n");
    if report.unexplored_neighbors.is_empty() {
        body.push_str("<p style=\"color:#6b7280\"><em>no neighbors found</em></p>\n");
    } else {
        body.push_str(
            "<table><thead><tr>\
               <th>commit</th><th>value</th><th>&Delta;</th><th>description</th>\
             </tr></thead><tbody>\n",
        );
        for n in &report.unexplored_neighbors {
            body.push_str(&format!(
                "<tr><td><code>{commit}</code></td>\
                     <td>{value:.6}</td>\
                     <td>{delta:+.4}</td>\
                     <td>{desc}</td></tr>\n",
                commit = html_escape(short_commit(&n.commit)),
                value = n.value,
                delta = n.delta,
                desc = html_escape(&n.description),
            ));
        }
        body.push_str("</tbody></table>\n");
    }

    // --- suggestions ---
    body.push_str("<h2>Suggestions</h2>\n");
    if report.suggestions.is_empty() {
        body.push_str("<p style=\"color:#6b7280\"><em>no mechanical suggestions — run looks healthy.</em></p>\n");
    } else {
        body.push_str("<ul>\n");
        for sug in &report.suggestions {
            body.push_str(&format!("<li>{}</li>\n", html_escape(sug)));
        }
        body.push_str("</ul>\n");
    }

    let page_title = format!("resman distill — {}", &report.tag);
    crate::html::page(&page_title, &body)
}

// ---------------------------------------------------------------------------
// Command entry point
// ---------------------------------------------------------------------------

pub fn cmd_distill(
    data_dir: &Path,
    tag: &str,
    out_path: Option<&std::path::Path>,
    format: &DistillFormat,
    html_path: Option<&std::path::Path>,
) -> Result<()> {
    let run = load_run_or_suggest(data_dir, tag)?;

    let report = build_distill(&run);

    // Write HTML artifact if requested.
    if let Some(hp) = html_path {
        let html = render_html(&report);
        std::fs::write(hp, &html)?;
        eprintln!("wrote HTML to {}", hp.display());
    }

    // Emit text/json output (to file or stdout) unless --html was the only flag.
    // Rule: --html is additive; text output happens unless suppressed by redirection.
    // Implementation: always emit text output (stdout or --out file).
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
    use std::collections::HashSet;

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

    /// HTML Test 1: output contains title with tag name and a <style> block.
    #[test]
    fn render_html_contains_title_and_tag() {
        let run = make_run(
            "my-tag",
            vec![make_exp(
                "abc1234",
                0.95,
                Status::Keep,
                "baseline",
                None,
                vec![],
            )],
        );
        let report = build_distill(&run);
        let html = render_html(&report);
        assert!(
            html.contains("my-tag"),
            "tag name must appear in HTML output"
        );
        assert!(html.contains("<style>"), "must have <style> block");
        // Self-contained: no external references
        assert!(!html.contains("http://"), "must not reference http://");
        assert!(!html.contains("src=\"http"), "must not have external src");
    }

    /// HTML Test 2: empty run renders without best card but still produces valid HTML.
    #[test]
    fn render_html_empty_run_has_no_best_card_but_still_renders() {
        let run = make_run("empty-run", vec![]);
        let report = build_distill(&run);
        let html = render_html(&report);
        // Should contain the "no best" fallback text
        assert!(
            html.contains("No best experiment"),
            "should render no-best placeholder"
        );
        // Should NOT contain best-card (only rendered when best is Some)
        assert!(
            !html.contains("class=\"best-card\""),
            "best-card should not appear when no best"
        );
        // Must still be valid HTML envelope
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("</html>"));
    }

    /// HTML Test 3: signal sections appear with <details> when signals present.
    #[test]
    fn render_html_with_signals_groups_by_kind() {
        let run = make_run(
            "sigs",
            vec![
                make_exp("c1", 0.0, Status::Crash, "oom1", None, vec![Signal::Oom]),
                make_exp("c2", 0.0, Status::Crash, "oom2", None, vec![Signal::Oom]),
                make_exp("c3", 0.0, Status::Crash, "nan", None, vec![Signal::NanLoss]),
            ],
        );
        let report = build_distill(&run);
        let html = render_html(&report);
        assert!(
            html.contains("<details>"),
            "must have <details> elements for signals"
        );
        assert!(html.contains("oom"), "oom kind must appear");
        assert!(html.contains("nan_loss"), "nan_loss kind must appear");
    }

    /// HTML Test 4: HTML-special chars in description are escaped.
    #[test]
    fn render_html_escapes_html_in_description() {
        let run = make_run(
            "xss-test",
            vec![make_exp(
                "abc1234",
                0.95,
                Status::Keep,
                "<script>alert(1)</script>",
                None,
                vec![],
            )],
        );
        let report = build_distill(&run);
        let html = render_html(&report);
        assert!(
            html.contains("&lt;script&gt;"),
            "< and > must be HTML-escaped"
        );
        assert!(
            !html.contains("<script>alert"),
            "raw <script> tag must not appear"
        );
    }

    /// HTML Test 5: output contains no HTTP references and has exactly one <style> block.
    #[test]
    fn render_html_no_external_refs() {
        let run = make_run(
            "netcheck",
            vec![make_exp("a1", 0.9, Status::Best, "best one", None, vec![])],
        );
        let report = build_distill(&run);
        let html = render_html(&report);
        assert!(!html.contains("http://"));
        assert!(!html.contains("https://"));
        // Count <style> occurrences — must be exactly 1
        let style_count = html.matches("<style>").count();
        assert_eq!(style_count, 1, "expected exactly 1 <style> block");
        // Must contain tag in output
        let tag_count = html.matches("netcheck").count();
        assert!(tag_count >= 1, "tag must appear at least once");
        // All badge classes referenced from CSS
        let badge_classes: HashSet<&str> = [
            "badge-keep",
            "badge-best",
            "badge-crash",
            "badge-discard",
            "badge-verified",
            "badge-neutral",
        ]
        .iter()
        .copied()
        .collect();
        for cls in &badge_classes {
            assert!(html.contains(cls), "CSS must define {cls}");
        }
    }
}
