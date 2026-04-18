use std::collections::HashSet;
use std::path::Path;

use crate::error::{Error, Result};
use crate::model::{Direction, Experiment, RunLog, Status};
use crate::store::{load_all_runs, require_run};

/// Print the best experiment — designed for shell scripts and agent loops.
///
/// Formats:
///   - "table" (default): human-readable multi-line summary
///   - "value": single val_bpb float, nothing else (for `$(resman best -f value)`)
///   - "json": compact JSON line
///
/// When `composite=true`, selects by weighted multi-dim score instead of raw
/// metric. Default behavior (`composite=false`) is byte-identical to pre-v0.7.
pub fn cmd_best(data_dir: &Path, tag: Option<&str>, format: &str, composite: bool) -> Result<()> {
    let runs: Vec<RunLog> = match tag {
        Some(t) => vec![require_run(data_dir, t)?],
        None => load_all_runs(data_dir)?,
    };

    if runs.is_empty() || runs.iter().all(|r| r.experiments.is_empty()) {
        return Err(Error::Empty);
    }

    if composite {
        return cmd_best_composite(&runs, format);
    }

    // --- original non-composite path (unchanged) ---
    let mut global_best: Option<(&RunLog, &Experiment)> = None;
    for r in &runs {
        if let Some(b) = r.best() {
            let dir = b.effective_direction(r);
            match global_best {
                None => {
                    global_best = Some((r, b));
                }
                Some((gr, gb)) => {
                    let gdir = gb.effective_direction(gr);
                    if dir != gdir {
                        eprintln!(
                            "warning: comparing runs with different directions ({} vs {}); using first run's direction",
                            gdir.as_str(),
                            dir.as_str()
                        );
                    }
                    let better = match gdir {
                        Direction::Minimize => b.val_bpb < gb.val_bpb,
                        Direction::Maximize => b.val_bpb > gb.val_bpb,
                    };
                    if better {
                        global_best = Some((r, b));
                    }
                }
            }
        }
    }

    let (run, best) = global_best.ok_or(Error::Empty)?;
    let label = best.effective_metric_name(run);

    match format {
        "value" => println!("{:.6}", best.val_bpb),
        "json" => println!("{}", serde_json::to_string(best)?),
        _ => {
            let glyph = crate::term::status_glyph(&best.status);
            println!("best experiment:");
            println!("  {}:     {:.6}", label, best.val_bpb);
            println!("  memory_gb:   {:.1}", best.memory_gb);
            println!("  commit:      {}", best.commit);
            println!("  status:      {} {}", glyph, best.status);
            println!("  description: {}", best.description);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Composite scoring
// ---------------------------------------------------------------------------

pub struct CompositeScores {
    pub metric: f64,
    pub verified: f64,
    pub lineage: f64,
    pub desc: f64,
    pub score: f64,
}

impl CompositeScores {
    pub fn compute(
        exp: &Experiment,
        run: &RunLog,
        run_min: f64,
        run_max: f64,
        direction: Direction,
    ) -> Self {
        let metric = normalize_metric(exp.val_bpb, run_min, run_max, direction);
        let verified = verified_score(exp.status);
        let lineage = lineage_score(exp, run);
        let desc = (exp.description.chars().count() as f64 / 80.0).min(1.0);
        let score = 0.5 * metric + 0.2 * verified + 0.2 * lineage + 0.1 * desc;
        CompositeScores {
            metric,
            verified,
            lineage,
            desc,
            score,
        }
    }
}

fn normalize_metric(val: f64, run_min: f64, run_max: f64, direction: Direction) -> f64 {
    if (run_max - run_min).abs() < f64::EPSILON {
        return 1.0;
    }
    match direction {
        Direction::Minimize => (run_max - val) / (run_max - run_min),
        Direction::Maximize => (val - run_min) / (run_max - run_min),
    }
}

fn verified_score(status: Status) -> f64 {
    match status {
        Status::Verified => 1.0,
        Status::Best => 0.5,
        Status::Keep => 0.3,
        Status::Discard => 0.0,
        Status::Crash => 0.0,
    }
}

/// Count parent_commit hops from `exp` up to a root (no parent, or parent not
/// in this run). Cycle-safe via visited set. Depth capped at `run.len()`.
pub fn lineage_depth(exp: &Experiment, run: &RunLog) -> usize {
    let commit_set: HashSet<&str> = run.experiments.iter().map(|e| e.commit.as_str()).collect();
    let mut depth = 0usize;
    let mut current_commit: Option<&str> = exp.parent_commit.as_deref();
    let mut visited: HashSet<&str> = HashSet::new();
    visited.insert(exp.commit.as_str());
    let max_walk = run.experiments.len();

    while depth < max_walk {
        match current_commit {
            None => break,
            Some(p) if !commit_set.contains(p) => break,
            Some(p) if visited.contains(p) => break, // cycle guard
            Some(p) => {
                depth += 1;
                visited.insert(p);
                // Find parent experiment to follow its parent_commit.
                current_commit = run
                    .experiments
                    .iter()
                    .find(|e| e.commit.as_str() == p)
                    .and_then(|e| e.parent_commit.as_deref());
            }
        }
    }
    depth
}

fn lineage_score(exp: &Experiment, run: &RunLog) -> f64 {
    (lineage_depth(exp, run) as f64 / 5.0).min(1.0)
}

/// Collect valid candidate (run, experiment) pairs for composite scoring.
/// Applies the same filter as `RunLog::best()`: kept status, and for Minimize,
/// val_bpb > 0.0 and finite.
pub fn composite_candidates(runs: &[RunLog]) -> Vec<(&RunLog, &Experiment)> {
    let mut out = Vec::new();
    for r in runs {
        let direction = r
            .metric_direction
            .or_else(|| r.experiments.first().and_then(|e| e.metric_direction))
            .unwrap_or(Direction::Minimize);
        for e in &r.experiments {
            if !e.status.is_kept() {
                continue;
            }
            if !e.val_bpb.is_finite() {
                continue;
            }
            if direction == Direction::Minimize && e.val_bpb <= 0.0 {
                continue;
            }
            out.push((r, e));
        }
    }
    out
}

fn cmd_best_composite(runs: &[RunLog], format: &str) -> Result<()> {
    let candidates = composite_candidates(runs);
    if candidates.is_empty() {
        return Err(Error::Empty);
    }

    // Determine range over all candidates from all runs.
    // For simplicity when mixing runs use the first run's direction (matches
    // existing cross-run direction logic).
    let first_dir = {
        let (r, e) = candidates[0];
        e.effective_direction(r)
    };
    let values: Vec<f64> = candidates.iter().map(|(_, e)| e.val_bpb).collect();
    let run_min = values.iter().cloned().fold(f64::INFINITY, f64::min);
    let run_max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

    // Score every candidate; stable iteration → ties broken by insertion order.
    let scored: Vec<(CompositeScores, &RunLog, &Experiment)> = candidates
        .iter()
        .map(|(r, e)| {
            let s = CompositeScores::compute(e, r, run_min, run_max, first_dir);
            (s, *r, *e)
        })
        .collect();

    // Pick best: highest composite score, then highest metric_score, then first.
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
                // Earlier insertion wins on tie (reverse index comparison).
                .then_with(|| j.cmp(i))
        })
        .map(|(_, triple)| triple)
        .ok_or(Error::Empty)?;

    let (scores, run, best) = winner;
    let label = best.effective_metric_name(run);

    match format {
        "value" => println!("{:.6}", scores.score),
        "json" => {
            let mut v = serde_json::to_value(best)?;
            v["composite"] = serde_json::json!({
                "score":    scores.score,
                "metric":   scores.metric,
                "verified": scores.verified,
                "lineage":  scores.lineage,
                "desc":     scores.desc,
            });
            println!("{v}");
        }
        "tsv" => {
            // Existing columns first (matching plain best TSV would be a single
            // line; we emit the core fields then append composite columns).
            println!(
                "{}\t{}\t{:.6}\t{}\t{:.6}\t{:.6}\t{:.6}\t{:.6}",
                best.commit,
                label,
                best.val_bpb,
                best.status,
                scores.score,
                scores.metric,
                scores.verified,
                scores.lineage,
            );
        }
        _ => {
            // table
            let glyph = crate::term::status_glyph(&best.status);
            println!("best experiment:");
            println!("  {}:     {:.6}", label, best.val_bpb);
            println!("  memory_gb:   {:.1}", best.memory_gb);
            println!("  commit:      {}", best.commit);
            println!("  status:      {} {}", glyph, best.status);
            println!("  description: {}", best.description);
            println!("composite score: {:.3}", scores.score);
            println!(
                "  metric:    {:.3} × 0.5 = {:.3}",
                scores.metric,
                0.5 * scores.metric
            );
            println!(
                "  verified:  {:.3} × 0.2 = {:.3}",
                scores.verified,
                0.2 * scores.verified
            );
            println!(
                "  lineage:   {:.3} × 0.2 = {:.3}",
                scores.lineage,
                0.2 * scores.lineage
            );
            println!(
                "  desc:      {:.3} × 0.1 = {:.3}",
                scores.desc,
                0.1 * scores.desc
            );
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::model::{Direction, Experiment, RunLog, Status};

    fn make_run(direction: Option<Direction>, experiments: Vec<Experiment>) -> RunLog {
        RunLog {
            experiments,
            run_tag: "test".into(),
            created_at: String::new(),
            metric_name: None,
            metric_direction: direction,
        }
    }

    fn make_exp(
        commit: &str,
        val_bpb: f64,
        status: Status,
        description: &str,
        parent_commit: Option<&str>,
    ) -> Experiment {
        Experiment {
            commit: commit.into(),
            val_bpb,
            memory_gb: 0.0,
            status,
            description: description.into(),
            timestamp: String::new(),
            params: HashMap::new(),
            parent_commit: parent_commit.map(str::to_string),
            crash_excerpt: None,
            metric_name: None,
            metric_direction: None,
            signals: Vec::new(),
        }
    }

    // 1. Verified experiment with slightly worse raw metric beats Keep with
    //    better metric due to verified + lineage + desc bonuses.
    #[test]
    fn composite_prefers_verified_over_slightly_better_metric() {
        // Exp A: Verified, val_bpb=0.982, rich description (>80 chars), depth=2
        let desc_a = "a".repeat(90); // 90 chars → desc_score = 1.0
        let exp_a = make_exp("aaa", 0.982, Status::Verified, &desc_a, Some("bbb"));
        // Exp C: parent of A (depth chain: A→B→C root)
        let exp_c = make_exp("ccc", 0.990, Status::Keep, "root", None);

        // Build run so that depth(A)=2: A.parent=bbb, B.parent=ccc (root).
        let exp_b2 = make_exp("bbb", 0.980, Status::Keep, "short", Some("ccc"));

        let run = make_run(Some(Direction::Minimize), vec![exp_a, exp_b2, exp_c]);
        let run_arr = [run.clone()];
        let candidates = composite_candidates(&run_arr);
        assert!(!candidates.is_empty());

        // Verify sub-scores for each candidate.
        // run_min=0.980, run_max=0.990
        let run_min = 0.980_f64;
        let run_max = 0.990_f64;

        // Exp B (bbb): metric_score = (0.990 - 0.980)/(0.990-0.980) = 1.0
        let s_b = CompositeScores::compute(
            candidates
                .iter()
                .find(|(_, e)| e.commit == "bbb")
                .unwrap()
                .1,
            &run,
            run_min,
            run_max,
            Direction::Minimize,
        );
        assert!(
            (s_b.metric - 1.0).abs() < 1e-9,
            "B metric_score should be 1.0, got {}",
            s_b.metric
        );
        assert!(
            (s_b.verified - 0.3).abs() < 1e-9,
            "B verified_score should be 0.3"
        );

        // Exp A (aaa): metric_score = (0.990 - 0.982)/(0.990 - 0.980) = 0.008/0.010 = 0.8
        let s_a = CompositeScores::compute(
            candidates
                .iter()
                .find(|(_, e)| e.commit == "aaa")
                .unwrap()
                .1,
            &run,
            run_min,
            run_max,
            Direction::Minimize,
        );
        assert!(
            (s_a.metric - 0.8).abs() < 1e-9,
            "A metric_score should be 0.8, got {}",
            s_a.metric
        );
        assert!(
            (s_a.verified - 1.0).abs() < 1e-9,
            "A verified_score should be 1.0"
        );
        // depth(A) = 2 hops (A→bbb→ccc); lineage_score = (2/5).min(1) = 0.4
        assert!(
            (s_a.lineage - 0.4).abs() < 1e-9,
            "A lineage_score should be 0.4, got {}",
            s_a.lineage
        );

        // Composite A should beat composite B.
        assert!(
            s_a.score > s_b.score,
            "A composite {:.4} should beat B composite {:.4}",
            s_a.score,
            s_b.score
        );

        // Use the real selection logic.
        let scored: Vec<_> = candidates
            .iter()
            .map(|(r, e)| {
                let s = CompositeScores::compute(e, r, run_min, run_max, Direction::Minimize);
                (s, *r, *e)
            })
            .collect();
        let winner_commit = scored
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
            .map(|(_, (_, _, e))| e.commit.clone())
            .unwrap();
        assert_eq!(
            winner_commit, "aaa",
            "composite winner should be A (verified)"
        );
    }

    // 2. When all experiments are identical in status/desc/lineage, composite
    //    falls back to raw metric.
    #[test]
    fn composite_falls_back_to_metric_when_all_equal() {
        let exp1 = make_exp("c1", 0.990, Status::Keep, "x", None);
        let exp2 = make_exp("c2", 0.985, Status::Keep, "x", None);
        let exp3 = make_exp("c3", 0.995, Status::Keep, "x", None);
        let run = make_run(Some(Direction::Minimize), vec![exp1, exp2, exp3]);
        let run_arr = [run.clone()];
        let candidates = composite_candidates(&run_arr);
        let run_min = 0.985_f64;
        let run_max = 0.995_f64;

        let scored: Vec<_> = candidates
            .iter()
            .map(|(r, e)| {
                let s = CompositeScores::compute(e, r, run_min, run_max, Direction::Minimize);
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
            .map(|(_, (_, _, e))| e.commit.clone())
            .unwrap();
        // c2 has the best raw metric (0.985, lowest under Minimize).
        assert_eq!(winner, "c2");
    }

    // 3. Crash experiment with val_bpb=0 under Minimize is skipped by
    //    composite_candidates (same filter as RunLog::best).
    #[test]
    fn composite_skips_invalid_metrics_same_as_plain_best() {
        let crash = make_exp("crash", 0.0, Status::Crash, "crashed run", None);
        let good = make_exp("good", 0.985, Status::Keep, "valid run", None);
        let run = make_run(Some(Direction::Minimize), vec![crash, good]);

        let run_arr = [run];
        let candidates = composite_candidates(&run_arr);
        // Crash must not appear in candidates.
        assert!(
            candidates.iter().all(|(_, e)| e.commit != "crash"),
            "crash experiment must be skipped"
        );
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].1.commit, "good");
    }

    // 4. A chain of 7 experiments: lineage_score for the deepest should be
    //    capped at 1.0 (depth=6 → 6/5 → capped to 1.0).
    #[test]
    fn composite_lineage_depth_capped_at_5() {
        // Chain: e0 (root) ← e1 ← e2 ← e3 ← e4 ← e5 ← e6
        let exps: Vec<Experiment> = (0..7)
            .map(|i| {
                let parent = if i == 0 {
                    None
                } else {
                    Some(format!("c{}", i - 1).as_str().to_string())
                };
                Experiment {
                    commit: format!("c{i}"),
                    val_bpb: 0.98 + i as f64 * 0.001,
                    memory_gb: 0.0,
                    status: Status::Keep,
                    description: "d".into(),
                    timestamp: String::new(),
                    params: HashMap::new(),
                    parent_commit: parent,
                    crash_excerpt: None,
                    metric_name: None,
                    metric_direction: None,
                    signals: Vec::new(),
                }
            })
            .collect();

        let run = make_run(Some(Direction::Minimize), exps);

        // e6 is 6 hops from root → depth=6 → lineage_score = (6/5).min(1) = 1.0
        let e6 = run.experiments.iter().find(|e| e.commit == "c6").unwrap();
        let depth = lineage_depth(e6, &run);
        assert_eq!(depth, 6, "depth of c6 should be 6");
        let ls = (depth as f64 / 5.0).min(1.0);
        assert!(
            (ls - 1.0).abs() < f64::EPSILON,
            "lineage_score should be capped at 1.0, got {ls}"
        );
    }
}
