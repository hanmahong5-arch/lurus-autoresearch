use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use chrono::Local;
use std::collections::HashMap;

use crate::model::{Experiment, RunLog};

pub fn cmd_import(data_dir: &Path, tsv_path: &Path, tag_override: Option<String>) {
    if !tsv_path.exists() {
        eprintln!("TSV file not found: {}", tsv_path.display());
        return;
    }

    let run_tag = tag_override.unwrap_or_else(|| {
        tsv_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("untagged")
            .to_string()
    });

    let file = File::open(tsv_path).unwrap();
    let reader = BufReader::new(file);
    let mut experiments = Vec::new();

    let lines: Vec<String> = reader.lines().filter_map(|l| l.ok()).collect();
    if lines.is_empty() {
        eprintln!("Empty file");
        return;
    }

    let mut best_bpb = f64::INFINITY;
    let mut best_idx = 0;
    let mut crash_count = 0;
    let mut keep_count = 0;

    for (i, line) in lines.iter().enumerate().skip(1) {
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 4 {
            continue;
        }

        let commit = cols[0].trim().to_string();
        let val_bpb: f64 = cols[1].trim().parse().unwrap_or(0.0);
        let memory_gb: f64 = cols[2].trim().parse().unwrap_or(0.0);
        let status = if cols.len() > 3 { cols[3].trim() } else { "keep" };
        let description = if cols.len() > 4 {
            cols[4..].join("\t")
        } else {
            String::new()
        };

        if status == "keep" || status == "best" {
            keep_count += 1;
        }
        if status == "crash" {
            crash_count += 1;
        }

        if val_bpb > 0.0 && val_bpb < best_bpb {
            best_bpb = val_bpb;
            best_idx = i;
        }

        experiments.push(Experiment {
            commit,
            val_bpb,
            memory_gb,
            status: status.to_string(),
            description,
            timestamp: String::new(),
            params: HashMap::new(),
        });
    }

    let run_log = RunLog {
        experiments: experiments.clone(),
        run_tag: run_tag.clone(),
        created_at: Local::now().to_rfc3339(),
    };

    let runs_dir = data_dir.join("runs");
    let _ = std::fs::create_dir_all(&runs_dir);
    let out_path = runs_dir.join(format!("{}.json", run_tag));
    let json = serde_json::to_string_pretty(&run_log).unwrap();
    let _ = std::fs::write(&out_path, json);

    println!("Imported {} experiments from {}", run_log.experiments.len(), tsv_path.display());
    if best_bpb < f64::INFINITY {
        println!("  best val_bpb: {:.6} (experiment #{})", best_bpb, best_idx);
    }
    println!("  kept: {}, crashed: {}", keep_count, crash_count);
    println!("  saved to: {}", out_path.display());
}
