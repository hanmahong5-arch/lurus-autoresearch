use std::path::Path;
use crate::store::load_all_runs;

pub fn cmd_stats(data_dir: &Path) {
    let runs = load_all_runs(data_dir);
    if runs.is_empty() {
        println!("No experiments found.");
        return;
    }

    let all_experiments: Vec<_> = runs.into_iter().flat_map(|r| r.experiments).collect();
    let kept: Vec<_> = all_experiments.iter().filter(|e| e.status == "keep" || e.status == "best").collect();
    let crashed = all_experiments.iter().filter(|e| e.status == "crash").count();
    let discarded = all_experiments.iter().filter(|e| e.status == "discard").count();

    if kept.is_empty() {
        println!("No kept experiments to generate stats.");
        return;
    }

    let bpbs: Vec<f64> = kept.iter().map(|e| e.val_bpb).collect();
    let best = bpbs.iter().cloned().fold(f64::INFINITY, f64::min);
    let worst = bpbs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let mean = bpbs.iter().sum::<f64>() / bpbs.len() as f64;
    let variance = bpbs.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / bpbs.len() as f64;
    let stddev = variance.sqrt();
    let improvement = worst - best;

    println!("=== Experiment Statistics ===
");
    println!("Total experiments:    {}", all_experiments.len());
    println!("Kept:                 {}", kept.len());
    println!("Discarded:            {}", discarded);
    println!("Crashed:              {}", crashed);
    println!("Crash rate:           {:.1}%", crashed as f64 / all_experiments.len() as f64 * 100.0);
    println!();
    println!("val_bpb range:        {:.6} - {:.6}", worst, best);
    println!("Mean val_bpb:         {:.6}", mean);
    println!("Std deviation:        {:.6}", stddev);
    println!("Total improvement:    {:.6} ({:.2}%)", improvement, if worst > 0.0 { improvement / worst * 100.0 } else { 0.0 });
    println!("Experiments to best:  {}", kept.len());
}
