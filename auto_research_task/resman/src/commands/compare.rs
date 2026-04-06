use std::path::Path;

use crate::model::Experiment;
use crate::store::{load_all_runs, default_data_dir, truncate};

pub fn cmd_compare(_data_dir: &Path, run_tags: &[String]) {
    let runs = load_all_runs(&default_data_dir());
    let filtered_runs: Vec<_> = if run_tags.is_empty() {
        runs
    } else {
        runs.into_iter()
            .filter(|r| run_tags.iter().any(|t| r.run_tag.contains(t)))
            .collect()
    };

    if filtered_runs.is_empty() {
        println!("No runs found to compare.");
        return;
    }

    println!("{:<20}  {:>10}  {:>10}  {:>8}  {:>8}  {}", "run", "best_bpb", "mem_gb", "kept", "crashed", "top description");
    println!("{}", "-".repeat(100));

    for run in &filtered_runs {
        let kept: Vec<&Experiment> = run.experiments.iter().filter(|e| e.status == "keep" || e.status == "best").collect();
        let best = kept.iter().min_by(|a, b| a.val_bpb.partial_cmp(&b.val_bpb).unwrap_or(std::cmp::Ordering::Equal));
        let crashed = run.experiments.iter().filter(|e| e.status == "crash").count();
        let best_desc = best.map(|e| e.description.as_str()).unwrap_or("N/A");
        let best_bpb = best.map(|e| e.val_bpb).unwrap_or(0.0);
        let best_mem = best.map(|e| e.memory_gb).unwrap_or(0.0);

        println!("{:<20}  {:>10.6}  {:>9.1}  {:>8}  {:>8}  {}",
            run.run_tag, best_bpb, best_mem, kept.len(), crashed,
            truncate(best_desc, 30));
    }
}
