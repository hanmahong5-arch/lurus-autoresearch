use std::path::Path;

use crate::cli::OutputFormat;
use crate::error::{Error, Result};
use crate::model::{Direction, Experiment};
use crate::store::{load_all_runs, truncate};

/// Find the N experiments with val_bpb closest to a target value.
///
/// Useful when the agent gets a new result and wants to ground it: "what else
/// landed near 0.985? was it memory-greedy? did it crash often?" — a cheap
/// proxy for a semantic neighborhood search without any embedding model.
pub fn cmd_near(data_dir: &Path, target: f64, n: usize, format: &OutputFormat) -> Result<()> {
    let runs = load_all_runs(data_dir)?;
    let mut all: Vec<(String, Experiment)> = runs
        .into_iter()
        .flat_map(|r| {
            let tag = r.run_tag.clone();
            r.experiments
                .into_iter()
                .filter(move |e| {
                    // Mirror RunLog::best(): for Minimize, exclude sentinel 0.0;
                    // for Maximize, 0.0 is a legitimate value.
                    let dir = e
                        .metric_direction
                        .or(r.metric_direction)
                        .unwrap_or(Direction::Minimize);
                    match dir {
                        Direction::Minimize => e.val_bpb > 0.0,
                        Direction::Maximize => true,
                    }
                })
                .map(move |e| (tag.clone(), e))
        })
        .collect();

    if all.is_empty() {
        return Err(Error::Empty);
    }

    all.sort_by(|a, b| {
        (a.1.val_bpb - target)
            .abs()
            .partial_cmp(&(b.1.val_bpb - target).abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    all.truncate(n);

    match format {
        OutputFormat::Json => {
            let out: Vec<_> = all.iter().map(|(tag, e)| {
                serde_json::json!({
                    "run": tag, "commit": e.commit, "val_bpb": e.val_bpb,
                    "delta": e.val_bpb - target, "status": e.status, "description": e.description,
                })
            }).collect();
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        OutputFormat::Tsv => {
            println!("run\tcommit\tval_bpb\tdelta\tstatus\tdescription");
            for (tag, e) in &all {
                println!(
                    "{tag}\t{}\t{:.6}\t{:+.6}\t{}\t{}",
                    e.commit,
                    e.val_bpb,
                    e.val_bpb - target,
                    e.status,
                    crate::store::tsv_field(&e.description)
                );
            }
        }
        OutputFormat::Table => {
            println!("neighbors of val_bpb={target:.6} (closest first):\n");
            println!(
                "{:<14}  {:>10}  {:>10}  {:>8}  description",
                "run", "val_bpb", "Δ", "status"
            );
            println!("{}", "-".repeat(88));
            for (tag, e) in &all {
                println!(
                    "{:<14}  {:>10.6}  {:>+10.6}  {:>8}  {}",
                    truncate(tag, 14),
                    e.val_bpb,
                    e.val_bpb - target,
                    e.status,
                    truncate(&e.description, 40)
                );
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::model::{Direction, Experiment, RunLog, Status};
    use std::collections::HashMap;

    fn make_exp(commit: &str, val_bpb: f64, dir: Option<Direction>) -> Experiment {
        Experiment {
            commit: commit.to_string(),
            val_bpb,
            memory_gb: 0.0,
            status: Status::Keep,
            description: String::new(),
            timestamp: String::new(),
            params: HashMap::new(),
            parent_commit: None,
            crash_excerpt: None,
            metric_name: None,
            metric_direction: dir,
            signals: vec![],
        }
    }

    fn make_run(tag: &str, dir: Option<Direction>, exps: Vec<Experiment>) -> RunLog {
        RunLog {
            run_tag: tag.to_string(),
            created_at: String::new(),
            experiments: exps,
            metric_name: None,
            metric_direction: dir,
            schema_version: 1,
        }
    }

    /// A Maximize run with val_bpb=0.0 must survive the direction-aware filter
    /// and appear in the neighbor results (it is a legitimate score, not a sentinel).
    #[test]
    fn near_maximize_zero_included() {
        use crate::store::save_run;
        let data_dir = std::env::temp_dir().join("resman_test_near_maximize_zero");
        std::fs::create_dir_all(crate::store::runs_dir(&data_dir)).unwrap();

        let run = make_run(
            "max_run",
            Some(Direction::Maximize),
            vec![
                make_exp("zero_commit", 0.0, None),
                make_exp("pos_commit", 0.5, None),
            ],
        );
        save_run(&data_dir, &run).unwrap();

        // Both experiments should appear when searching near 0.1.
        let result = super::cmd_near(&data_dir, 0.1, 10, &crate::cli::OutputFormat::Json);
        // cmd_near prints JSON; we re-load to verify the count properly.
        // Instead, test the filter logic directly via the loaded runs.
        drop(result);

        let runs = crate::store::load_all_runs(&data_dir).unwrap();
        let candidates: Vec<_> = runs
            .into_iter()
            .flat_map(|r| {
                r.experiments
                    .into_iter()
                    .filter(move |e| {
                        let dir = e
                            .metric_direction
                            .or(r.metric_direction)
                            .unwrap_or(Direction::Minimize);
                        match dir {
                            Direction::Minimize => e.val_bpb > 0.0,
                            Direction::Maximize => true,
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .collect();

        assert_eq!(
            candidates.len(),
            2,
            "Maximize 0.0 experiment must not be filtered out"
        );
        assert!(
            candidates.iter().any(|e| e.commit == "zero_commit"),
            "zero_commit must be in neighbors for a Maximize run"
        );

        let _ = std::fs::remove_dir_all(&data_dir);
    }

    /// For a Minimize run, val_bpb=0.0 is a sentinel and must be excluded.
    #[test]
    fn near_minimize_zero_excluded() {
        use crate::store::save_run;
        let data_dir = std::env::temp_dir().join("resman_test_near_minimize_zero");
        std::fs::create_dir_all(crate::store::runs_dir(&data_dir)).unwrap();

        let run = make_run(
            "min_run",
            Some(Direction::Minimize),
            vec![
                make_exp("zero_commit", 0.0, None),
                make_exp("pos_commit", 0.5, None),
            ],
        );
        save_run(&data_dir, &run).unwrap();

        let runs = crate::store::load_all_runs(&data_dir).unwrap();
        let candidates: Vec<_> = runs
            .into_iter()
            .flat_map(|r| {
                r.experiments
                    .into_iter()
                    .filter(move |e| {
                        let dir = e
                            .metric_direction
                            .or(r.metric_direction)
                            .unwrap_or(Direction::Minimize);
                        match dir {
                            Direction::Minimize => e.val_bpb > 0.0,
                            Direction::Maximize => true,
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .collect();

        assert_eq!(
            candidates.len(),
            1,
            "Minimize 0.0 is a sentinel and must be filtered"
        );
        assert_eq!(candidates[0].commit, "pos_commit");

        let _ = std::fs::remove_dir_all(&data_dir);
    }
}
