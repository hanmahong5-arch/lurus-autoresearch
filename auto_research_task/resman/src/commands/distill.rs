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
use crate::store::{load_all_runs, load_run, load_run_or_suggest, truncate};

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

    // --- Verified-gap suggestions (highest priority — prepended before heuristics) ---
    let keep_best_count = run
        .experiments
        .iter()
        .filter(|e| matches!(e.status, Status::Keep | Status::Best))
        .count();
    let verified_count = run
        .experiments
        .iter()
        .filter(|e| e.status == Status::Verified)
        .count();

    if keep_best_count >= 5 && verified_count == 0 {
        // Rule (b): no verified at all — subsumes (a)
        suggestions.push(format!(
            "No experiments have been verified yet. Pick the top {keep_best_count} candidates \
             and re-run them via `resman verify` — single-seed improvements often don't reproduce."
        ));
    } else if let Some(b) = best_exp {
        // Rule (a): best is unverified (not crash, not verified)
        if b.status != Status::Verified && b.status != Status::Crash {
            let sc = short_commit(&b.commit);
            suggestions.push(format!(
                "Best experiment is unverified — re-run and call \
                 `resman verify {sc} --value <new>` to promote to verified status before you rely on it."
            ));
        }
    }

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
// Cross-run aggregation (resman distill --all)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossSignalExample {
    pub tag: String,
    pub commit: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossSignalSummary {
    pub kind: String,
    pub count: usize,
    pub examples: Vec<CrossSignalExample>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossTagSummary {
    pub tag: String,
    pub best_value: f64,
    pub direction: String,
    pub metric_name: String,
    pub experiment_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossDistillReport {
    pub generated_at: String,
    pub total_runs: usize,
    pub total_experiments: usize,
    pub total_keep: usize,
    pub total_discard: usize,
    pub total_crash: usize,
    pub total_verified: usize,
    /// Top-5 failure signal kinds by count across all runs.
    pub top_failure_signals: Vec<CrossSignalSummary>,
    /// Top-3 tags ranked by best metric value (direction-aware per tag).
    pub top_tags: Vec<CrossTagSummary>,
    pub suggestions: Vec<String>,
}

pub fn build_cross_distill(runs: &[RunLog]) -> CrossDistillReport {
    let generated_at = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    let total_runs = runs.len();
    let mut total_experiments = 0usize;
    let mut total_keep = 0usize;
    let mut total_discard = 0usize;
    let mut total_crash = 0usize;
    let mut total_verified = 0usize;

    // Aggregate signal counts: kind -> (total_count, Vec<(tag, commit, desc)>)
    let mut signal_map: HashMap<&'static str, (usize, Vec<CrossSignalExample>)> = HashMap::new();
    for kind in crate::signals::ALL_KINDS {
        signal_map.insert(kind, (0, vec![]));
    }

    // Per-tag best values for ranking.
    let mut tag_summaries: Vec<CrossTagSummary> = Vec::new();

    // How many tags have an unverified best (for cross-distill suggestion).
    let mut tags_with_unverified_best = 0usize;

    // OOM-by-tag: tag -> oom_count (for concentration suggestion).
    let mut oom_by_tag: Vec<(String, usize)> = Vec::new();

    for run in runs {
        let n = run.experiments.len();
        total_experiments += n;
        total_keep += run
            .experiments
            .iter()
            .filter(|e| e.status == Status::Keep)
            .count();
        total_discard += run
            .experiments
            .iter()
            .filter(|e| e.status == Status::Discard)
            .count();
        total_crash += run
            .experiments
            .iter()
            .filter(|e| e.status == Status::Crash)
            .count();
        total_verified += run
            .experiments
            .iter()
            .filter(|e| e.status == Status::Verified)
            .count();

        // Collect signals from each experiment.
        let mut run_oom_count = 0usize;
        for exp in &run.experiments {
            for sig in &exp.signals {
                let kind = sig.kind();
                if kind == "oom" {
                    run_oom_count += 1;
                }
                let entry = signal_map.entry(kind).or_insert((0, vec![]));
                entry.0 += 1;
                if entry.1.len() < 3 {
                    entry.1.push(CrossSignalExample {
                        tag: run.run_tag.clone(),
                        commit: short_commit(&exp.commit).to_string(),
                        description: truncate(&exp.description, 60),
                    });
                }
            }
        }
        oom_by_tag.push((run.run_tag.clone(), run_oom_count));

        // Best for this tag.
        let direction = run
            .metric_direction
            .or_else(|| run.experiments.first().and_then(|e| e.metric_direction))
            .unwrap_or(Direction::Minimize);
        let metric_name = run
            .metric_name
            .clone()
            .or_else(|| run.experiments.first().and_then(|e| e.metric_name.clone()))
            .unwrap_or_else(|| "val_bpb".to_string());

        if let Some(best) = run.best() {
            if best.status != Status::Verified && best.status != Status::Crash {
                tags_with_unverified_best += 1;
            }
            tag_summaries.push(CrossTagSummary {
                tag: run.run_tag.clone(),
                best_value: best.val_bpb,
                direction: direction.as_str().to_string(),
                metric_name,
                experiment_count: n,
            });
        }
    }

    // Sort tag_summaries: each tag has its own direction — sort per tag's direction.
    // We want the globally "best" tags first.  Since directions may differ, compare
    // within same direction groups, then interleave. Simplification: sort by
    // (direction, value) where for Minimize lower=better (sort asc) and
    // Maximize higher=better (sort desc). We use a signed score:
    // Minimize → score = -value; Maximize → score = +value. Highest score = best.
    tag_summaries.sort_by(|a, b| {
        let score_a = if a.direction == "minimize" {
            -a.best_value
        } else {
            a.best_value
        };
        let score_b = if b.direction == "minimize" {
            -b.best_value
        } else {
            b.best_value
        };
        score_b
            .partial_cmp(&score_a)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    tag_summaries.truncate(3);

    // Build top-5 failure signals sorted by count desc, then kind name asc for stability.
    let mut signal_vec: Vec<CrossSignalSummary> = signal_map
        .into_iter()
        .filter(|(_, (count, _))| *count > 0)
        .map(|(kind, (count, examples))| CrossSignalSummary {
            kind: kind.to_string(),
            count,
            examples,
        })
        .collect();
    signal_vec.sort_by(|a, b| b.count.cmp(&a.count).then(a.kind.cmp(&b.kind)));
    signal_vec.truncate(5);

    let total_oom_start = signal_vec
        .iter()
        .find(|s| s.kind == "oom")
        .map(|s| s.count)
        .unwrap_or(0);

    // --- Cross-distill suggestions ---
    let mut suggestions: Vec<String> = Vec::new();

    // Verified-gap suggestion (cross-run version).
    if tags_with_unverified_best > 0 {
        let total_tags_with_best = runs.iter().filter(|r| r.best().is_some()).count();
        suggestions.push(format!(
            "{tags_with_unverified_best} of your {total_tags_with_best} tags have unverified bests \
             — consider re-run them via `resman verify` to confirm results."
        ));
    }

    // OOM-concentration suggestion: if one tag accounts for >50% of all OOMs.
    if total_oom_start >= 3 {
        oom_by_tag.sort_by(|a, b| b.1.cmp(&a.1));
        if let Some((top_tag, top_count)) = oom_by_tag.first()
            && *top_count * 2 > total_oom_start
        {
            suggestions.push(format!(
                "Tag `{top_tag}` accounts for {top_count}/{total_oom_start} OOMs \
                 — likely a memory leak or misconfigured batch size in that branch."
            ));
        }
    }

    CrossDistillReport {
        generated_at,
        total_runs,
        total_experiments,
        total_keep,
        total_discard,
        total_crash,
        total_verified,
        top_failure_signals: signal_vec,
        top_tags: tag_summaries,
        suggestions,
    }
}

pub fn render_cross_markdown(report: &CrossDistillReport) -> String {
    let mut out = String::new();
    out.push_str("# Distill: cross-run summary\n\n");
    out.push_str(&format!(
        "_Generated from {} runs, {} experiments total \
         ({} keep, {} discard, {} crash, {} verified)._\n\n",
        report.total_runs,
        report.total_experiments,
        report.total_keep,
        report.total_discard,
        report.total_crash,
        report.total_verified,
    ));

    // Top tags
    out.push_str("## Top tags by best metric\n");
    if report.top_tags.is_empty() {
        out.push_str("_no kept experiments across all runs_\n");
    } else {
        for (i, t) in report.top_tags.iter().enumerate() {
            out.push_str(&format!(
                "{}. **{}** — {}={:.6} ({}) — {} experiments\n",
                i + 1,
                t.tag,
                t.metric_name,
                t.best_value,
                t.direction,
                t.experiment_count,
            ));
        }
    }
    out.push('\n');

    // Top failure signals
    out.push_str("## Top failure signals\n");
    if report.top_failure_signals.is_empty() {
        out.push_str("_no failure signals recorded_\n");
    } else {
        for sig in &report.top_failure_signals {
            out.push_str(&format!("### {} ({})\n", sig.kind, sig.count));
            for ex in &sig.examples {
                out.push_str(&format!(
                    "- `{}` [{}] — {}\n",
                    ex.commit, ex.tag, ex.description
                ));
            }
            out.push('\n');
        }
    }

    // Suggestions
    out.push_str("## Suggestions\n");
    if report.suggestions.is_empty() {
        out.push_str("_no mechanical suggestions — runs look healthy._\n");
    } else {
        for (i, s) in report.suggestions.iter().enumerate() {
            out.push_str(&format!("{}. {}\n", i + 1, s));
        }
    }
    out.push('\n');

    out.push_str("---\n");
    out.push_str(&format!(
        "_resman distill --all v0.8 — {}_\n",
        report.generated_at
    ));
    out
}

pub fn cmd_cross_distill(
    data_dir: &Path,
    out_path: Option<&std::path::Path>,
    format: &DistillFormat,
) -> Result<()> {
    let runs = load_all_runs(data_dir)?;
    if runs.is_empty() {
        eprintln!("warning: no runs found in data directory");
    }
    let report = build_cross_distill(&runs);
    let rendered = match format {
        DistillFormat::Markdown => render_cross_markdown(&report),
        DistillFormat::Json => serde_json::to_string_pretty(&report)?,
    };
    match out_path {
        Some(p) => std::fs::write(p, &rendered)?,
        None => print!("{rendered}"),
    }
    Ok(())
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

    // -----------------------------------------------------------------------
    // Wave C tests
    // -----------------------------------------------------------------------

    /// Wave C Test 1: When best experiment is Status::Keep (unverified),
    /// suggestions must include the unverified-best prompt.
    #[test]
    fn suggestions_include_unverified_best_when_best_is_keep() {
        let run = make_run(
            "uvtest",
            vec![
                make_exp("aaa11111", 1.2, Status::Keep, "baseline", None, vec![]),
                make_exp("bbb22222", 0.8, Status::Best, "improved", None, vec![]),
            ],
        );
        let report = build_distill(&run);
        let has_verify_hint = report
            .suggestions
            .iter()
            .any(|s| s.contains("unverified") && s.contains("resman verify"));
        assert!(
            has_verify_hint,
            "expected unverified-best suggestion, got: {:?}",
            report.suggestions
        );
        // Must reference short commit of best
        let short = &"bbb22222"[..8];
        let has_commit = report.suggestions.iter().any(|s| s.contains(short));
        assert!(has_commit, "suggestion should contain short commit {short}");
    }

    /// Wave C Test 2: When ≥5 keep/best and zero verified, the bulk prompt
    /// fires and the single-best prompt does NOT.
    #[test]
    fn suggestions_prefer_bulk_unverified_prompt_over_single_when_no_verified() {
        let exps = (0..6)
            .map(|i| {
                make_exp(
                    &format!("c{i}aabbcc"),
                    1.0 - i as f64 * 0.05,
                    Status::Keep,
                    "desc",
                    None,
                    vec![],
                )
            })
            .collect();
        let run = make_run("bulktest", exps);
        let report = build_distill(&run);
        let bulk = report
            .suggestions
            .iter()
            .any(|s| s.contains("No experiments have been verified yet"));
        let single = report
            .suggestions
            .iter()
            .any(|s| s.contains("Best experiment is unverified"));
        assert!(bulk, "bulk prompt must fire when ≥5 keep and 0 verified");
        assert!(!single, "single-best prompt must NOT fire when bulk fires");
    }

    /// Wave C Test 3: build_cross_distill aggregates signal counts across runs.
    #[test]
    fn build_cross_distill_aggregates_signals_across_runs() {
        let run_a = make_run(
            "run_a",
            vec![
                make_exp("a1", 0.0, Status::Crash, "oom1", None, vec![Signal::Oom]),
                make_exp("a2", 0.0, Status::Crash, "oom2", None, vec![Signal::Oom]),
            ],
        );
        let run_b = make_run(
            "run_b",
            vec![
                make_exp("b1", 0.0, Status::Crash, "oom3", None, vec![Signal::Oom]),
                make_exp(
                    "b2",
                    0.0,
                    Status::Crash,
                    "nan1",
                    None,
                    vec![Signal::NanLoss],
                ),
            ],
        );
        let report = build_cross_distill(&[run_a, run_b]);
        // Total oom across both runs = 3
        let oom_summary = report
            .top_failure_signals
            .iter()
            .find(|s| s.kind == "oom")
            .expect("oom must be in top signals");
        assert_eq!(
            oom_summary.count, 3,
            "expected 3 OOMs across runs, got {}",
            oom_summary.count
        );
        // nan_loss = 1
        let nan_summary = report
            .top_failure_signals
            .iter()
            .find(|s| s.kind == "nan_loss");
        assert!(nan_summary.is_some());
        assert_eq!(nan_summary.unwrap().count, 1);
        // totals
        assert_eq!(report.total_runs, 2);
        assert_eq!(report.total_experiments, 4);
        assert_eq!(report.total_crash, 4);
    }

    /// Wave C Test 4: build_cross_distill ranks tags by direction.
    /// Minimize tag: lower is better. Maximize tag: higher is better.
    #[test]
    fn build_cross_distill_ranks_tags_by_direction() {
        let mut run_min = make_run(
            "min_tag",
            vec![
                make_exp("m1", 0.5, Status::Best, "best-min", None, vec![]),
                make_exp("m2", 0.9, Status::Keep, "worse", None, vec![]),
            ],
        );
        run_min.metric_direction = Some(Direction::Minimize);

        let mut run_max = make_run(
            "max_tag",
            vec![
                make_exp("x1", 0.95, Status::Best, "best-max", None, vec![]),
                make_exp("x2", 0.5, Status::Keep, "worse", None, vec![]),
            ],
        );
        run_max.metric_direction = Some(Direction::Maximize);

        let report = build_cross_distill(&[run_min, run_max]);
        // max_tag has best_value=0.95 with maximize => score=+0.95
        // min_tag has best_value=0.5 with minimize => score=-0.5
        // Highest score first: max_tag before min_tag
        assert!(!report.top_tags.is_empty());
        assert_eq!(
            report.top_tags[0].tag, "max_tag",
            "maximize tag with 0.95 should rank first"
        );
        assert_eq!(report.top_tags[1].tag, "min_tag");
    }

    /// Wave C Test 5: render_cross_markdown contains the Top failure signals section.
    #[test]
    fn render_cross_markdown_contains_top_failures_section() {
        let run = make_run(
            "sigrun",
            vec![
                make_exp("x1", 0.0, Status::Crash, "oom run", None, vec![Signal::Oom]),
                make_exp("x2", 0.8, Status::Keep, "good run", None, vec![]),
            ],
        );
        let report = build_cross_distill(&[run]);
        let md = render_cross_markdown(&report);
        assert!(
            md.contains("## Top failure signals"),
            "must have '## Top failure signals'"
        );
        assert!(
            md.contains("## Top tags by best metric"),
            "must have '## Top tags by best metric'"
        );
        assert!(md.contains("oom"), "oom must appear in cross markdown");
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
