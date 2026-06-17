//! `resman distill -t <tag>` — structured Markdown/JSON summary of a run.
//!
//! Produces a "what did we learn last night?" artifact: best result, lineage,
//! failure signals, unexplored neighbors, and heuristic suggestions. No LLM,
//! no extra crates — pure template rendering over the existing RunLog schema.

mod cluster;
mod neighbors;
mod render;
mod suggest;

use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::Result;
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
    pub verified: usize,
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

/// A cross-tag link: this tag's root has a parent_commit that lives in
/// some other tag, so this tag "continues from" that prior session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContinuationLink {
    pub from_tag: String,
    pub from_commit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchVerdict {
    pub root_commit: String,
    pub depth: usize,
    pub terminal_commit: String,
    pub terminal_status: String,
    /// "converged" (terminal is keep/best/verified) |
    /// "broke" (terminal is crash) |
    /// "abandoned" (terminal is discard)
    pub verdict: String,
    /// For "broke" verdicts, the first signal kind on the terminal commit.
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistillReport {
    pub tag: String,
    pub generated_at: String,
    pub summary: DistillSummary,
    pub best: Option<DistillBest>,
    pub lineage: Vec<LineageEntry>,
    /// Non-best branch root verdicts (added v0.11). Empty when every
    /// experiment is on the best lineage.
    #[serde(default)]
    pub branch_verdicts: Vec<BranchVerdict>,
    /// Cross-tag continuation (added v0.11): this tag's root experiment
    /// has a parent_commit that lives in another tag. None when this is
    /// a standalone session start.
    #[serde(default)]
    pub continues_from: Option<ContinuationLink>,
    pub failure_signals: HashMap<String, Vec<FailureSignalEntry>>,
    pub unexplored_neighbors: Vec<NeighborEntry>,
    pub suggestions: Vec<String>,
}

// ---------------------------------------------------------------------------
// Cross-run aggregation types (resman distill --all)
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
// Shared helpers (used by submodules)
// ---------------------------------------------------------------------------

pub(super) fn status_glyph(s: Status) -> String {
    crate::term::status_glyph(&s)
}

pub(super) fn short_commit(c: &str) -> &str {
    let len = c.len().min(8);
    &c[..len]
}

// Re-export render functions as public (used by mcp.rs and cmd_distill).
pub use render::{render_cross_html, render_cross_markdown, render_html, render_markdown};

// ---------------------------------------------------------------------------
// Builder — single-run
// ---------------------------------------------------------------------------

pub fn build_distill(
    run: &RunLog,
    usage: Option<&crate::commands::usage::TagFunnel>,
) -> DistillReport {
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
    let verified = run
        .experiments
        .iter()
        .filter(|e| e.status == Status::Verified)
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
        verified,
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
    let failure_signals = cluster::build_failure_signals(run);

    // --- unexplored neighbors (3 closest to best that aren't best) ---
    let unexplored_neighbors = if let Some(b) = best_exp {
        neighbors::build_neighbors(run, b, direction, &metric_name)
    } else {
        vec![]
    };

    // --- suggestions ---
    let suggestions =
        suggest::build_suggestions(run, &failure_signals, total, crash, best_exp, &tag, usage);

    // --- branch verdicts (non-best branches) ---
    let best_chain: HashSet<String> = lineage.iter().map(|e| e.commit.clone()).collect();
    let branch_verdicts = build_branch_verdicts(run, &best_chain);

    DistillReport {
        tag,
        generated_at,
        summary,
        best,
        lineage,
        branch_verdicts,
        continues_from: None,
        failure_signals,
        unexplored_neighbors,
        suggestions,
    }
}

/// Detect whether `run` has any root experiment whose `parent_commit`
/// is recorded in a *different* tag, and return the first such link.
///
/// `other_runs` is the full set of runs (including `run` itself — the
/// function filters by `run_tag` to skip self-matches). Pass an empty
/// slice for "no context" (this is the default used by `build_distill`).
pub fn find_continuation(run: &RunLog, other_runs: &[RunLog]) -> Option<ContinuationLink> {
    let commits_in_run: HashSet<&str> = run.experiments.iter().map(|e| e.commit.as_str()).collect();
    let external_parents: Vec<&str> = run
        .experiments
        .iter()
        .filter_map(|e| e.parent_commit.as_deref())
        .filter(|p| !commits_in_run.contains(p))
        .collect();
    if external_parents.is_empty() {
        return None;
    }

    for parent in external_parents {
        for other in other_runs {
            if other.run_tag == run.run_tag {
                continue;
            }
            for exp in &other.experiments {
                if exp.commit == parent
                    || exp.commit.starts_with(parent)
                    || parent.starts_with(&*exp.commit)
                {
                    return Some(ContinuationLink {
                        from_tag: other.run_tag.clone(),
                        from_commit: exp.commit.clone(),
                    });
                }
            }
        }
    }
    None
}

/// Walk descendants of every "root" commit (parent_commit absent or
/// pointing outside this run) that is NOT already on the best lineage.
/// Take the deepest single-thread descendant chain for each root, label
/// the terminus, and return one BranchVerdict per surfaced root.
fn build_branch_verdicts(run: &RunLog, best_chain: &HashSet<String>) -> Vec<BranchVerdict> {
    if run.experiments.is_empty() {
        return Vec::new();
    }

    // commit → first descendant index (children pointing to this commit
    // via parent_commit). For multi-child branches we take the deepest
    // single-child path, which is sufficient for a verdict summary.
    let mut children: HashMap<String, Vec<usize>> = HashMap::new();
    for (idx, exp) in run.experiments.iter().enumerate() {
        if let Some(p) = &exp.parent_commit {
            children.entry(p.clone()).or_default().push(idx);
        }
    }
    let by_commit: HashMap<&str, usize> = run
        .experiments
        .iter()
        .enumerate()
        .map(|(i, e)| (e.commit.as_str(), i))
        .collect();

    // Roots: commits whose parent is absent or not in this run.
    let mut roots: Vec<&Experiment> = run
        .experiments
        .iter()
        .filter(|e| match &e.parent_commit {
            None => true,
            Some(p) => !by_commit.contains_key(p.as_str()),
        })
        .filter(|e| !best_chain.contains(&e.commit))
        .collect();
    // Stable order for deterministic output: by commit string.
    roots.sort_by(|a, b| a.commit.cmp(&b.commit));

    roots
        .into_iter()
        .map(|root| {
            // Walk one chain from this root to its deepest descendant. Take
            // the first child each step (cycle/depth guarded at 50 hops).
            let mut chain_indices: Vec<usize> = vec![by_commit[root.commit.as_str()]];
            let mut current = root.commit.clone();
            let mut hops = 0usize;
            while let Some(kids) = children.get(&current) {
                if hops >= 50 {
                    break;
                }
                hops += 1;
                let next_idx = kids[0]; // deepest single path
                if chain_indices.contains(&next_idx) {
                    break; // cycle guard
                }
                chain_indices.push(next_idx);
                current = run.experiments[next_idx].commit.clone();
            }

            let terminal_idx = *chain_indices.last().unwrap();
            let terminal = &run.experiments[terminal_idx];
            let (verdict, note) = match terminal.status {
                Status::Keep | Status::Best | Status::Verified => ("converged".to_string(), None),
                Status::Crash => {
                    let kind = terminal.signals.first().map(|s| s.kind().to_string());
                    ("broke".to_string(), kind)
                }
                Status::Discard => ("abandoned".to_string(), None),
            };

            BranchVerdict {
                root_commit: root.commit.clone(),
                depth: chain_indices.len(),
                terminal_commit: terminal.commit.clone(),
                terminal_status: terminal.status.to_string(),
                verdict,
                note,
            }
        })
        .collect()
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

// ---------------------------------------------------------------------------
// Builder — cross-run
// ---------------------------------------------------------------------------

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

    let total_oom = signal_vec
        .iter()
        .find(|s| s.kind == "oom")
        .map(|s| s.count)
        .unwrap_or(0);

    let suggestions =
        suggest::build_cross_suggestions(runs, tags_with_unverified_best, total_oom, oom_by_tag);

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

// ---------------------------------------------------------------------------
// Command entry points
// ---------------------------------------------------------------------------

pub fn cmd_distill(
    data_dir: &Path,
    tag: &str,
    out_path: Option<&std::path::Path>,
    format: &DistillFormat,
    html_path: Option<&std::path::Path>,
) -> Result<()> {
    let run = load_run_or_suggest(data_dir, tag)?;
    let all_runs = load_all_runs(data_dir).unwrap_or_default();

    let events = crate::commands::usage::load_events(data_dir);
    let funnel = crate::commands::usage::tag_funnel(&events, tag);
    let mut report = build_distill(&run, Some(&funnel));
    report.continues_from = find_continuation(&run, &all_runs);

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

pub fn cmd_cross_distill(
    data_dir: &Path,
    out_path: Option<&std::path::Path>,
    format: &DistillFormat,
    html_path: Option<&std::path::Path>,
) -> Result<()> {
    let runs = load_all_runs(data_dir)?;
    if runs.is_empty() {
        eprintln!("warning: no runs found in data directory");
    }
    let report = build_cross_distill(&runs);

    // Write HTML artifact if requested (additive — text output still emitted).
    if let Some(hp) = html_path {
        let html = render_cross_html(&report);
        std::fs::write(hp, &html)?;
        eprintln!("wrote HTML to {}", hp.display());
    }

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
    let all_runs = load_all_runs(data_dir).map_err(|e| e.to_string())?;
    let events = crate::commands::usage::load_events(data_dir);
    let funnel = crate::commands::usage::tag_funnel(&events, tag);
    let mut report = build_distill(&run, Some(&funnel));
    report.continues_from = find_continuation(&run, &all_runs);
    if json {
        serde_json::to_string_pretty(&report).map_err(|e| e.to_string())
    } else {
        Ok(render_markdown(&report))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests;
