use std::collections::HashSet;
use std::path::Path;

use crate::cli::OutputFormat;
use crate::error::Result;
use crate::model::{RunLog, Status};
use crate::store::{load_all_runs, truncate};

/// Returns `true` when the runs span more than one distinct effective metric
/// name OR more than one distinct direction, meaning cross-run aggregation
/// would mix incompatible quantities.  Runs with no best experiment are
/// skipped because they contribute nothing to the aggregate.
pub(crate) fn runs_are_mixed_metric(runs: &[RunLog]) -> bool {
    let mut names: HashSet<String> = HashSet::new();
    let mut dirs: HashSet<String> = HashSet::new();
    for r in runs {
        if let Some(b) = r.best() {
            names.insert(b.effective_metric_name(r).to_string());
            dirs.insert(b.effective_direction(r).as_str().to_string());
        }
    }
    names.len() > 1 || dirs.len() > 1
}

/// Build the per-run JSON summary objects used by the Json output branch.
pub(crate) fn compare_summary(filtered: &[RunLog]) -> Vec<serde_json::Value> {
    filtered
        .iter()
        .map(|r| {
            let best = r.best();
            let metric = best
                .map(|b| b.effective_metric_name(r))
                .unwrap_or("val_bpb");
            let direction = best
                .map(|b| b.effective_direction(r).as_str())
                .unwrap_or("minimize");
            serde_json::json!({
                "run": r.run_tag,
                "best_bpb": best.map(|e| e.val_bpb),
                "metric_name": metric,
                "direction": direction,
                "best_commit": best.map(|e| e.commit.as_str()),
                "best_description": best.map(|e| e.description.as_str()),
                "kept": r.kept().count(),
                "crashed": r.experiments.iter().filter(|e| e.status == Status::Crash).count(),
                "total": r.experiments.len(),
            })
        })
        .collect()
}

pub fn cmd_compare(data_dir: &Path, run_tags: &[String], format: &OutputFormat) -> Result<()> {
    let runs = load_all_runs(data_dir)?;
    let filtered: Vec<_> = if run_tags.is_empty() {
        runs
    } else {
        runs.into_iter()
            .filter(|r| run_tags.iter().any(|t| r.run_tag.contains(t)))
            .collect()
    };

    if filtered.is_empty() {
        println!(
            "{}",
            crate::term::empty_state(
                "no runs to compare yet — add or import experiments first (`resman add ...` or `resman import <file>`)."
            )
        );
        return Ok(());
    }

    // Warn when no tag filter was applied (cross-run) and runs disagree on metric/direction.
    if run_tags.is_empty() && runs_are_mixed_metric(&filtered) {
        let names: Vec<String> = {
            let mut seen: Vec<String> = Vec::new();
            let mut set: HashSet<String> = HashSet::new();
            for r in &filtered {
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

    // Determine the column header: use the common effective metric name if all
    // runs agree (and have a best experiment), else "best_metric".
    let col_label: String = {
        let names: Vec<&str> = filtered
            .iter()
            .filter_map(|r| r.best().map(|b| b.effective_metric_name(r)))
            .collect();
        if names.is_empty() {
            "best_metric".to_string()
        } else {
            let first = names[0];
            if names.iter().all(|n| *n == first) {
                format!("best_{first}")
            } else {
                "best_metric".to_string()
            }
        }
    };

    match format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&compare_summary(&filtered))?
            );
        }
        OutputFormat::Tsv => {
            println!("run\t{col_label}\tmem_gb\tkept\tcrashed\ttop_description");
            for r in &filtered {
                let best = r.best();
                println!(
                    "{}\t{:.6}\t{:.1}\t{}\t{}\t{}",
                    r.run_tag,
                    best.map(|e| e.val_bpb).unwrap_or(0.0),
                    best.map(|e| e.memory_gb).unwrap_or(0.0),
                    r.kept().count(),
                    r.experiments
                        .iter()
                        .filter(|e| e.status == Status::Crash)
                        .count(),
                    best.map(|e| crate::store::tsv_field(&e.description))
                        .unwrap_or_default()
                );
            }
        }
        OutputFormat::Table => {
            let n = filtered.len();
            println!(
                "{}",
                crate::term::section_header("compare", Some(&format!("{n} run(s)")))
            );
            println!(
                "{:<20}  {:>10}  {:>7}  {:>5}  {:>7}  {:<10}  top_description",
                "run", col_label, "mem_gb", "kept", "crashed", "status"
            );
            println!("{}", crate::term::rule());
            for r in &filtered {
                let best = r.best();
                let status_col = best
                    .map(|e| crate::term::status_cell(&e.status))
                    .unwrap_or_else(|| "          ".to_string());
                println!(
                    "{:<20}  {:>10.6}  {:>7.1}  {:>5}  {:>7}  {}  {}",
                    truncate(&r.run_tag, 20),
                    best.map(|e| e.val_bpb).unwrap_or(0.0),
                    best.map(|e| e.memory_gb).unwrap_or(0.0),
                    r.kept().count(),
                    r.experiments
                        .iter()
                        .filter(|e| e.status == Status::Crash)
                        .count(),
                    status_col,
                    truncate(
                        best.map(|e| e.description.as_str()).unwrap_or("—"),
                        crate::term::DESC_TRUNC_NARROW
                    )
                );
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::model::{Experiment, RunLog, Status};
    use std::collections::HashMap;

    fn make_exp(commit: &str, status: Status, val_bpb: f64) -> Experiment {
        Experiment {
            commit: commit.to_string(),
            val_bpb,
            memory_gb: 0.0,
            status,
            description: format!("desc_{commit}"),
            timestamp: String::new(),
            params: HashMap::new(),
            parent_commit: None,
            crash_excerpt: None,
            metric_name: None,
            metric_direction: None,
            signals: vec![],
        }
    }

    fn make_run(tag: &str, exps: Vec<Experiment>) -> RunLog {
        RunLog {
            run_tag: tag.to_string(),
            created_at: String::new(),
            experiments: exps,
            metric_name: None,
            metric_direction: None,
            schema_version: 1,
        }
    }

    fn make_run_with_metric(
        tag: &str,
        metric_name: Option<&str>,
        direction: Option<crate::model::Direction>,
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
    fn runs_are_mixed_metric_uniform_returns_false() {
        let runs = vec![
            make_run_with_metric(
                "a",
                Some("val_bpb"),
                Some(crate::model::Direction::Minimize),
                vec![make_exp("c1", Status::Keep, 1.0)],
            ),
            make_run_with_metric(
                "b",
                Some("val_bpb"),
                Some(crate::model::Direction::Minimize),
                vec![make_exp("c2", Status::Keep, 1.1)],
            ),
        ];
        assert!(!super::runs_are_mixed_metric(&runs));
    }

    #[test]
    fn runs_are_mixed_metric_different_names_returns_true() {
        let runs = vec![
            make_run_with_metric(
                "a",
                Some("val_bpb"),
                Some(crate::model::Direction::Minimize),
                vec![make_exp("c1", Status::Keep, 1.0)],
            ),
            make_run_with_metric(
                "b",
                Some("accuracy"),
                Some(crate::model::Direction::Maximize),
                vec![make_exp("c2", Status::Keep, 0.9)],
            ),
        ];
        assert!(super::runs_are_mixed_metric(&runs));
    }

    #[test]
    fn runs_are_mixed_metric_same_name_different_direction_returns_true() {
        let runs = vec![
            make_run_with_metric(
                "a",
                Some("score"),
                Some(crate::model::Direction::Minimize),
                vec![make_exp("c1", Status::Keep, 1.0)],
            ),
            make_run_with_metric(
                "b",
                Some("score"),
                Some(crate::model::Direction::Maximize),
                vec![make_exp("c2", Status::Keep, 0.9)],
            ),
        ];
        assert!(super::runs_are_mixed_metric(&runs));
    }

    #[test]
    fn runs_are_mixed_metric_skips_runs_with_no_best() {
        // Run with only crashed experiments has no best; should not count toward distinct metrics.
        let runs = vec![
            make_run_with_metric(
                "a",
                Some("val_bpb"),
                Some(crate::model::Direction::Minimize),
                vec![make_exp("c1", Status::Keep, 1.0)],
            ),
            make_run_with_metric(
                "b",
                Some("accuracy"),
                None,
                vec![make_exp("c2", Status::Crash, 0.0)],
            ),
        ];
        // run_b has no best (only crashed), so only run_a contributes → uniform → false
        assert!(!super::runs_are_mixed_metric(&runs));
    }

    #[test]
    fn compare_summary_one_row_per_run() {
        let runs = vec![
            make_run(
                "run_a",
                vec![
                    make_exp("c1", Status::Keep, 1.5),
                    make_exp("c2", Status::Keep, 1.2),
                    make_exp("c3", Status::Crash, 0.0),
                ],
            ),
            make_run(
                "run_b",
                vec![
                    make_exp("c4", Status::Keep, 2.0),
                    make_exp("c5", Status::Discard, 2.5),
                ],
            ),
        ];

        let summary = super::compare_summary(&runs);

        assert_eq!(summary.len(), 2, "one row per run");

        let a = summary.iter().find(|v| v["run"] == "run_a").unwrap();
        // best kept in run_a: c2 (1.2 < 1.5)
        assert_eq!(
            a["best_bpb"].as_f64().unwrap(),
            1.2,
            "run_a best_bpb should be 1.2"
        );
        assert_eq!(a["kept"].as_u64().unwrap(), 2, "run_a kept=2");
        assert_eq!(a["crashed"].as_u64().unwrap(), 1, "run_a crashed=1");

        let b = summary.iter().find(|v| v["run"] == "run_b").unwrap();
        assert_eq!(
            b["best_bpb"].as_f64().unwrap(),
            2.0,
            "run_b best_bpb should be 2.0"
        );
        assert_eq!(b["kept"].as_u64().unwrap(), 1, "run_b kept=1");
        assert_eq!(b["crashed"].as_u64().unwrap(), 0, "run_b crashed=0");
    }
}
