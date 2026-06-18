use std::path::Path;

use crate::error::Result;
use crate::model::{Direction, Experiment, Status};
use crate::store::{load_all_runs, require_run};

pub(crate) struct BpbStats {
    pub best: f64,
    pub worst: f64,
    pub mean: f64,
    pub stddev: f64,
    pub improvement: f64,
    pub pct_improve: f64,
    pub improvement_rate: f64,
}

pub(crate) struct StatsData {
    pub total: usize,
    pub kept: usize,
    pub discarded: usize,
    pub crashed: usize,
    pub bpb: Option<BpbStats>,
}

/// Compute counts and val_bpb statistics from a slice of experiments.
/// `bpb` is `None` when there are no kept experiments or no positive val_bpb values.
pub(crate) fn compute_stats(experiments: &[Experiment]) -> StatsData {
    let kept_exps: Vec<&Experiment> = experiments.iter().filter(|e| e.status.is_kept()).collect();
    let crashed = experiments
        .iter()
        .filter(|e| e.status == Status::Crash)
        .count();
    let discarded = experiments
        .iter()
        .filter(|e| e.status == Status::Discard)
        .count();
    let total = experiments.len();
    let kept = kept_exps.len();

    let bpb = if kept == 0 {
        None
    } else {
        let bpbs: Vec<f64> = kept_exps
            .iter()
            .map(|e| e.val_bpb)
            .filter(|v| *v > 0.0)
            .collect();
        if bpbs.is_empty() {
            None
        } else {
            // Effective direction: first experiment's override, else Minimize.
            let direction = experiments
                .first()
                .and_then(|e| e.metric_direction)
                .unwrap_or(Direction::Minimize);
            let (best, worst) = match direction {
                Direction::Minimize => (
                    bpbs.iter().copied().fold(f64::INFINITY, f64::min),
                    bpbs.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                ),
                Direction::Maximize => (
                    bpbs.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                    bpbs.iter().copied().fold(f64::INFINITY, f64::min),
                ),
            };
            let mean = bpbs.iter().sum::<f64>() / bpbs.len() as f64;
            let variance = bpbs.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / bpbs.len() as f64;
            let stddev = variance.sqrt();
            let improvement = worst - best;
            let pct_improve = if worst > 0.0 {
                improvement / worst * 100.0
            } else {
                0.0
            };
            let scored_count = bpbs.len();
            let improvement_rate = if improvement > 0.0 {
                improvement / scored_count as f64
            } else {
                0.0
            };
            Some(BpbStats {
                best,
                worst,
                mean,
                stddev,
                improvement,
                pct_improve,
                improvement_rate,
            })
        }
    };

    StatsData {
        total,
        kept,
        discarded,
        crashed,
        bpb,
    }
}

pub(crate) fn pct(part: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        part as f64 / total as f64 * 100.0
    }
}

pub fn cmd_stats(data_dir: &Path, tag: Option<&str>) -> Result<()> {
    let experiments: Vec<Experiment> = match tag {
        Some(t) => require_run(data_dir, t)?.experiments,
        None => {
            let runs = load_all_runs(data_dir)?;
            if crate::commands::compare::runs_are_mixed_metric(&runs) {
                let names: Vec<String> = {
                    let mut seen: Vec<String> = Vec::new();
                    let mut set: std::collections::HashSet<String> =
                        std::collections::HashSet::new();
                    for r in &runs {
                        if let Some(b) = r.best() {
                            let n = b.effective_metric_name(r).to_string();
                            if set.insert(n.clone()) {
                                seen.push(n);
                            }
                        }
                    }
                    seen
                };
                eprintln!(
                    "warning: aggregating runs with different metrics ({}); cross-run stats may not be comparable — use --tag to scope to one run",
                    names.join(", ")
                );
            }
            runs.into_iter().flat_map(|r| r.experiments).collect()
        }
    };

    if experiments.is_empty() {
        println!(
            "{}",
            crate::term::empty_state(
                "no experiments to summarize yet — add or import some first (`resman add ...` or `resman import <file>`)."
            )
        );
        return Ok(());
    }

    let s = compute_stats(&experiments);

    println!("{}\n", crate::term::section_header("stats", tag));
    println!("total:       {}", s.total);
    println!("kept:        {}  ({:.1}%)", s.kept, pct(s.kept, s.total));
    println!(
        "discarded:   {}  ({:.1}%)",
        s.discarded,
        pct(s.discarded, s.total)
    );
    println!(
        "crashed:     {}  ({:.1}%)",
        s.crashed,
        pct(s.crashed, s.total)
    );

    if s.kept == 0 {
        println!("\nno kept experiments — nothing to summarize.");
        return Ok(());
    }

    let bpb = match s.bpb {
        None => return Ok(()),
        Some(b) => b,
    };

    println!();
    println!("val_bpb:");
    println!("  best:        {:.6}", bpb.best);
    println!("  worst:       {:.6}", bpb.worst);
    println!("  mean:        {:.6}", bpb.mean);
    println!("  stddev:      {:.6}", bpb.stddev);
    println!(
        "  improvement: {:.6}  ({:.2}%)",
        bpb.improvement, bpb.pct_improve
    );
    println!("  bpb-drop per experiment: {:.6}", bpb.improvement_rate);
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::model::{Direction, Experiment, RunLog, Status};
    use std::collections::HashMap;

    fn make_run_with_metric(
        tag: &str,
        metric_name: Option<&str>,
        direction: Option<Direction>,
        exps: Vec<Experiment>,
    ) -> RunLog {
        RunLog {
            run_tag: tag.to_string(),
            created_at: String::new(),
            experiments: exps,
            metric_name: metric_name.map(str::to_string),
            metric_direction: direction,
            schema_version: 1,
        }
    }

    #[test]
    fn stats_mixed_metric_detection_via_helper() {
        use crate::commands::compare::runs_are_mixed_metric;

        // Two runs with the same metric → not mixed.
        let uniform = vec![
            make_run_with_metric(
                "a",
                Some("val_bpb"),
                Some(Direction::Minimize),
                vec![make_exp(1.0, Status::Keep)],
            ),
            make_run_with_metric(
                "b",
                Some("val_bpb"),
                Some(Direction::Minimize),
                vec![make_exp(1.1, Status::Keep)],
            ),
        ];
        assert!(!runs_are_mixed_metric(&uniform), "uniform → false");

        // Two runs with different metric names → mixed.
        let mixed = vec![
            make_run_with_metric(
                "a",
                Some("val_bpb"),
                Some(Direction::Minimize),
                vec![make_exp(1.0, Status::Keep)],
            ),
            make_run_with_metric(
                "b",
                Some("accuracy"),
                Some(Direction::Maximize),
                vec![make_exp(0.9, Status::Keep)],
            ),
        ];
        assert!(runs_are_mixed_metric(&mixed), "mixed names → true");
    }

    fn make_exp(val_bpb: f64, status: Status) -> Experiment {
        Experiment {
            commit: "abc".to_string(),
            val_bpb,
            memory_gb: 0.0,
            status,
            description: String::new(),
            timestamp: String::new(),
            params: HashMap::new(),
            parent_commit: None,
            crash_excerpt: None,
            metric_name: None,
            metric_direction: None,
            signals: vec![],
        }
    }

    #[test]
    fn compute_stats_arithmetic() {
        // 3 kept with val_bpb 1.0, 2.0, 3.0; 1 discarded; 1 crashed
        let exps = vec![
            make_exp(1.0, Status::Keep),
            make_exp(2.0, Status::Keep),
            make_exp(3.0, Status::Keep),
            make_exp(9.9, Status::Discard),
            make_exp(0.0, Status::Crash),
        ];
        let s = super::compute_stats(&exps);

        assert_eq!(s.total, 5);
        assert_eq!(s.kept, 3);
        assert_eq!(s.discarded, 1);
        assert_eq!(s.crashed, 1);

        let bpb = s.bpb.expect("bpb should be Some");
        assert!((bpb.best - 1.0).abs() < 1e-9, "best={}", bpb.best);
        assert!((bpb.worst - 3.0).abs() < 1e-9, "worst={}", bpb.worst);
        assert!((bpb.mean - 2.0).abs() < 1e-9, "mean={}", bpb.mean);
        // improvement = worst - best = 2.0
        assert!((bpb.improvement - 2.0).abs() < 1e-9);
        // pct_improve = 2/3 * 100
        assert!((bpb.pct_improve - (2.0 / 3.0 * 100.0)).abs() < 1e-6);
        // improvement_rate = 2.0 / 3 scored (kept finite) = 0.666...
        assert!((bpb.improvement_rate - (2.0 / 3.0)).abs() < 1e-9);
    }

    #[test]
    fn compute_stats_bpb_none_when_all_kept_zero_or_negative() {
        // All kept experiments have val_bpb <= 0 → bpb should be None
        let exps = vec![make_exp(0.0, Status::Keep), make_exp(-1.0, Status::Keep)];
        let s = super::compute_stats(&exps);
        assert_eq!(s.kept, 2);
        assert!(
            s.bpb.is_none(),
            "bpb should be None when no positive val_bpb"
        );
    }

    #[test]
    fn compute_stats_bpb_none_when_no_kept() {
        let exps = vec![make_exp(1.0, Status::Discard), make_exp(2.0, Status::Crash)];
        let s = super::compute_stats(&exps);
        assert_eq!(s.kept, 0);
        assert!(s.bpb.is_none(), "bpb should be None when kept=0");
    }

    fn make_exp_with_direction(
        val_bpb: f64,
        status: Status,
        direction: Option<Direction>,
    ) -> Experiment {
        Experiment {
            commit: "abc".to_string(),
            val_bpb,
            memory_gb: 0.0,
            status,
            description: String::new(),
            timestamp: String::new(),
            params: HashMap::new(),
            parent_commit: None,
            crash_excerpt: None,
            metric_name: None,
            metric_direction: direction,
            signals: vec![],
        }
    }

    #[test]
    fn compute_stats_maximize_best_is_max_worst_is_min() {
        // Maximize metric: [0.7, 0.8, 0.9] → best=0.9, worst=0.7
        let exps = vec![
            make_exp_with_direction(0.7, Status::Keep, Some(Direction::Maximize)),
            make_exp_with_direction(0.8, Status::Keep, Some(Direction::Maximize)),
            make_exp_with_direction(0.9, Status::Keep, Some(Direction::Maximize)),
        ];
        let s = super::compute_stats(&exps);
        let bpb = s.bpb.expect("bpb should be Some");
        assert!(
            (bpb.best - 0.9).abs() < 1e-9,
            "Maximize best should be 0.9, got {}",
            bpb.best
        );
        assert!(
            (bpb.worst - 0.7).abs() < 1e-9,
            "Maximize worst should be 0.7, got {}",
            bpb.worst
        );
    }

    #[test]
    fn compute_stats_improvement_rate_uses_kept_count_not_total() {
        // 3 kept (bpbs=[1.0,2.0,3.0]), 2 non-kept; improvement=2.0; rate must be 2.0/3, not 2.0/5
        let exps = vec![
            make_exp(1.0, Status::Keep),
            make_exp(2.0, Status::Keep),
            make_exp(3.0, Status::Keep),
            make_exp(9.9, Status::Discard),
            make_exp(0.0, Status::Crash), // val_bpb=0 filtered from bpbs; crash not kept
        ];
        let s = super::compute_stats(&exps);
        assert_eq!(s.total, 5);
        let bpb = s.bpb.expect("bpb should be Some");
        let expected = 2.0 / 3.0;
        assert!(
            (bpb.improvement_rate - expected).abs() < 1e-9,
            "improvement_rate should be {expected} (kept count=3), got {}",
            bpb.improvement_rate
        );
    }
}
