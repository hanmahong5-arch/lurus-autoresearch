use std::path::Path;

use regex::Regex;
use crate::model::Experiment;
use crate::store::{load_all_runs, truncate};

pub fn cmd_list(data_dir: &Path, status_filter: Option<&str>, sort_by: &str, grep_pat: Option<&str>, top: Option<usize>, reverse: bool) {
    let all = load_all_runs(data_dir);
    if all.is_empty() {
        println!("No experiments found. Try 'resman import <results.tsv>' first.");
        return;
    }

    let gre = grep_pat.and_then(|p| Regex::new(p).ok());
    let mut experiments: Vec<Experiment> = all.into_iter().flat_map(|r| r.experiments).collect();

    if let Some(s) = status_filter {
        experiments.retain(|e| e.status == s);
    } else {
        experiments.retain(|e| e.status == "keep" || e.status == "best");
    }
    if let Some(re) = &gre {
        experiments.retain(|e| re.is_match(&e.description));
    }

    match sort_by {
        "val_bpb" => experiments.sort_by(|a, b| a.val_bpb.partial_cmp(&b.val_bpb).unwrap()),
        "memory_gb" => experiments.sort_by(|a, b| a.memory_gb.partial_cmp(&b.memory_gb).unwrap()),
        "description" => experiments.sort_by(|a, b| a.description.cmp(&b.description)),
        _ => {}
    }
    if reverse {
        experiments.reverse();
    }
    if let Some(n) = top {
        experiments.truncate(n);
    }

    if experiments.is_empty() {
        println!("No experiments matched filters.");
        return;
    }

    println!("{:>6}  {:>8}  {:>10}  {:>8}  {:>7}  {}", "#", "val_bpb", "mem_gb", "commit", "status", "description");
    println!("{}", "-".repeat(110));

    for (i, exp) in experiments.iter().enumerate() {
        println!("{:>6}  {:>10.6}  {:>9.1}  {:>8}  {:>7}  {}",
            i + 1, exp.val_bpb, exp.memory_gb, &exp.commit, &exp.status, truncate(&exp.description, 60));
    }
    println!("\nTotal: {} experiments shown", experiments.len());
}
