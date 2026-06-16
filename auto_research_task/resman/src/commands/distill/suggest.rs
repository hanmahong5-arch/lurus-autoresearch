//! Heuristic suggestion generation for distill reports.

use std::collections::{HashMap, HashSet};

use crate::model::{Direction, Experiment, RunLog, Status};

use super::{FailureSignalEntry, short_commit};

pub(super) fn build_suggestions(
    run: &RunLog,
    failure_signals: &HashMap<String, Vec<FailureSignalEntry>>,
    total: usize,
    crash: usize,
    best_exp: Option<&Experiment>,
    tag: &str,
    usage: Option<&crate::commands::usage::TagFunnel>,
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

    // Usage-grounded verified-gap: MCP telemetry shows many adds but zero verifies.
    // Guard: suppress when there are no verifiable runs (e.g. all crashes).
    let has_verifiable = keep_best_count > 0 || verified_count > 0;
    if let Some(f) = usage
        && f.added >= 10
        && f.verified == 0
        && has_verifiable
    {
        suggestions.push(format!(
            "Agent usage shows {} experiments added for this tag but zero verify calls \
             — a reproducibility gap. Re-run your top candidates through `resman verify` before \
             trusting them.",
            f.added
        ));
    }

    let oom_count = failure_signals.get("oom").map(|v| v.len()).unwrap_or(0);
    let nan_count = failure_signals
        .get("nan_loss")
        .map(|v| v.len())
        .unwrap_or(0);

    // Suggestion 1: OOMs
    if oom_count >= 3 {
        let pct = (oom_count * 100).checked_div(total).unwrap_or(0);
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

    // Suggestion 7: stagnation — N consecutive runs since the last metric improvement.
    // Fires when there are >= 10 total experiments and >= 8 of the most-recent kept ones
    // have not advanced the rolling best.
    if total >= 10 {
        let direction = run
            .experiments
            .first()
            .map(|e| e.effective_direction(run))
            .unwrap_or(Direction::Minimize);

        let mut sorted: Vec<&Experiment> = run.experiments.iter().collect();
        sorted.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

        let mut running_best: Option<f64> = None;
        let mut last_improvement_idx: Option<usize> = None;
        for (i, e) in sorted.iter().enumerate() {
            if !e.status.is_kept() {
                continue;
            }
            let v = e.val_bpb;
            let is_better = match (running_best, direction) {
                (None, _) => true,
                (Some(b), Direction::Minimize) => v < b,
                (Some(b), Direction::Maximize) => v > b,
            };
            if is_better {
                running_best = Some(v);
                last_improvement_idx = Some(i);
            }
        }

        if let Some(last_imp) = last_improvement_idx {
            let runs_since = sorted.len().saturating_sub(1).saturating_sub(last_imp);
            if runs_since >= 8 {
                let anchor = sorted[last_imp];
                suggestions.push(format!(
                    "Tag is stagnant — {runs_since} consecutive experiments since the last improvement \
                     (commit {sc}, val_bpb={val:.4}). Try a radically different direction or \
                     revisit a non-best lineage branch via `resman_lineage`.",
                    sc = short_commit(&anchor.commit),
                    val = anchor.val_bpb,
                ));
            }
        }
    }

    // Suggestion 8: keep-but-reverted — a `keep` ancestor of a strictly-better
    // verified descendant is an under-explored direction that was abandoned
    // before being combined with the verified insight.
    if total >= 2 {
        let direction = run
            .experiments
            .first()
            .map(|e| e.effective_direction(run))
            .unwrap_or(Direction::Minimize);

        let by_commit: HashMap<&str, &Experiment> = run
            .experiments
            .iter()
            .map(|e| (e.commit.as_str(), e))
            .collect();

        let mut under_explored: Vec<(&Experiment, &Experiment)> = Vec::new();
        for verified in run
            .experiments
            .iter()
            .filter(|e| e.status == Status::Verified)
        {
            let mut current = verified.parent_commit.as_deref();
            let mut hops = 0usize;
            while let Some(parent_commit) = current {
                hops += 1;
                if hops > 50 {
                    break; // Defensive: parent_commit cycle / runaway chain
                }
                let parent = match by_commit.get(parent_commit) {
                    Some(p) => *p,
                    None => break, // crosses tags or commit missing — stop
                };
                if matches!(parent.status, Status::Keep) {
                    let strictly_better = match direction {
                        Direction::Minimize => verified.val_bpb < parent.val_bpb,
                        Direction::Maximize => verified.val_bpb > parent.val_bpb,
                    };
                    if strictly_better {
                        under_explored.push((parent, verified));
                        break;
                    }
                }
                current = parent.parent_commit.as_deref();
            }
        }

        if let Some((keep, ver)) = under_explored.first() {
            suggestions.push(format!(
                "Under-explored: `keep` experiment {kc} ({kv:.4}) was surpassed on its lineage \
                 by verified {vc} ({vv:.4}). The kept direction was abandoned but never recombined \
                 with verified-tier insights — re-run combining both.",
                kc = short_commit(&keep.commit),
                kv = keep.val_bpb,
                vc = short_commit(&ver.commit),
                vv = ver.val_bpb,
            ));

            if under_explored.len() > 1 {
                let more = under_explored.len() - 1;
                suggestions.push(format!(
                    "{more} additional under-explored keep experiment(s) on verified lineages — see `resman_lineage` for the chains."
                ));
            }
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
        oom_by_tag.sort_by_key(|x| std::cmp::Reverse(x.1));
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
