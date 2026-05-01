//! Heuristic suggestion generation for distill reports.

use std::collections::{HashMap, HashSet};

use crate::model::{Experiment, RunLog, Status};

use super::{FailureSignalEntry, short_commit};

pub(super) fn build_suggestions(
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

pub(super) fn build_cross_suggestions(
    runs: &[crate::model::RunLog],
    tags_with_unverified_best: usize,
    total_oom: usize,
    mut oom_by_tag: Vec<(String, usize)>,
) -> Vec<String> {
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
    if total_oom >= 3 {
        oom_by_tag.sort_by(|a, b| b.1.cmp(&a.1));
        if let Some((top_tag, top_count)) = oom_by_tag.first()
            && *top_count * 2 > total_oom
        {
            suggestions.push(format!(
                "Tag `{top_tag}` accounts for {top_count}/{total_oom} OOMs \
                 — likely a memory leak or misconfigured batch size in that branch."
            ));
        }
    }

    suggestions
}
