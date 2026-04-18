use std::path::Path;

use crate::cli::OutputFormat;
use crate::error::{Error, Result};
use crate::model::Experiment;
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
            r.experiments.into_iter().map(move |e| (tag.clone(), e))
        })
        .filter(|(_, e)| e.val_bpb > 0.0)
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
                    e.description
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
