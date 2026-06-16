use std::path::Path;

use crate::cli::OutputFormat;
use crate::error::Result;
use crate::model::{RunLog, Status};
use crate::store::{load_all_runs, truncate};

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
            "no runs to compare yet — add or import experiments first (`resman add ...` or `resman import <file>`)."
        );
        return Ok(());
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
                    best.map(|e| e.description.as_str()).unwrap_or("")
                );
            }
        }
        OutputFormat::Table => {
            println!(
                "{:<20}  {:>10}  {:>7}  {:>5}  {:>7}  st  top_description",
                "run", col_label, "mem_gb", "kept", "crashed"
            );
            println!("{}", "-".repeat(97));
            for r in &filtered {
                let best = r.best();
                let glyph = best
                    .map(|e| crate::term::status_glyph(&e.status))
                    .unwrap_or_else(|| "  ".to_string());
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
                    glyph,
                    truncate(best.map(|e| e.description.as_str()).unwrap_or("—"), 30)
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
