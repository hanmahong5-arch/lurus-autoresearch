use std::path::Path;

use crate::error::Result;
use crate::model::{Experiment, Status};
use crate::store::{load_all_runs, require_run};

pub fn cmd_stats(data_dir: &Path, tag: Option<&str>) -> Result<()> {
    let experiments: Vec<Experiment> = match tag {
        Some(t) => require_run(data_dir, t)?.experiments,
        None => load_all_runs(data_dir)?
            .into_iter()
            .flat_map(|r| r.experiments)
            .collect(),
    };

    if experiments.is_empty() {
        println!("no experiments found.");
        return Ok(());
    }

    let kept: Vec<&Experiment> = experiments.iter().filter(|e| e.status.is_kept()).collect();
    let crashed = experiments
        .iter()
        .filter(|e| e.status == Status::Crash)
        .count();
    let discarded = experiments
        .iter()
        .filter(|e| e.status == Status::Discard)
        .count();
    let total = experiments.len();

    println!(
        "=== experiment statistics{} ===\n",
        tag.map(|t| format!(" ({t})")).unwrap_or_default()
    );
    println!("total:       {total}");
    println!(
        "kept:        {}  ({:.1}%)",
        kept.len(),
        pct(kept.len(), total)
    );
    println!("discarded:   {discarded}  ({:.1}%)", pct(discarded, total));
    println!("crashed:     {crashed}  ({:.1}%)", pct(crashed, total));

    if kept.is_empty() {
        println!("\nno kept experiments — nothing to summarize.");
        return Ok(());
    }

    let bpbs: Vec<f64> = kept
        .iter()
        .map(|e| e.val_bpb)
        .filter(|v| *v > 0.0)
        .collect();
    if bpbs.is_empty() {
        return Ok(());
    }
    let best = bpbs.iter().copied().fold(f64::INFINITY, f64::min);
    let worst = bpbs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let mean = bpbs.iter().sum::<f64>() / bpbs.len() as f64;
    let variance = bpbs.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / bpbs.len() as f64;
    let stddev = variance.sqrt();
    let improvement = worst - best;
    let pct_improve = if worst > 0.0 {
        improvement / worst * 100.0
    } else {
        0.0
    };

    println!();
    println!("val_bpb:");
    println!("  best:        {best:.6}");
    println!("  worst:       {worst:.6}");
    println!("  mean:        {mean:.6}");
    println!("  stddev:      {stddev:.6}");
    println!("  improvement: {improvement:.6}  ({pct_improve:.2}%)");

    // Experiments-per-unit-progress — a more useful signal than raw counts.
    let improvement_rate = if improvement > 0.0 {
        improvement / total as f64
    } else {
        0.0
    };
    println!("  bpb-drop per experiment: {improvement_rate:.6}");
    Ok(())
}

fn pct(part: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        part as f64 / total as f64 * 100.0
    }
}
