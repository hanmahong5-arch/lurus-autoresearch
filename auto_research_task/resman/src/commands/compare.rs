use std::path::Path;

use crate::cli::OutputFormat;
use crate::error::Result;
use crate::model::Status;
use crate::store::{load_all_runs, truncate};

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
        println!("no runs found to compare.");
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
            let summary: Vec<_> = filtered
                .iter()
                .map(|r| {
                    let best = r.best();
                    let metric = best.map(|b| b.effective_metric_name(r)).unwrap_or("val_bpb");
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
                .collect();
            println!("{}", serde_json::to_string_pretty(&summary)?);
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
                "{:<20}  {:>10}  {:>7}  {:>5}  {:>7}  top_description",
                "run", col_label, "mem_gb", "kept", "crashed"
            );
            println!("{}", "-".repeat(92));
            for r in &filtered {
                let best = r.best();
                println!(
                    "{:<20}  {:>10.6}  {:>7.1}  {:>5}  {:>7}  {}",
                    truncate(&r.run_tag, 20),
                    best.map(|e| e.val_bpb).unwrap_or(0.0),
                    best.map(|e| e.memory_gb).unwrap_or(0.0),
                    r.kept().count(),
                    r.experiments
                        .iter()
                        .filter(|e| e.status == Status::Crash)
                        .count(),
                    truncate(best.map(|e| e.description.as_str()).unwrap_or("—"), 30)
                );
            }
        }
    }
    Ok(())
}
