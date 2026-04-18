use std::path::Path;

use crate::error::{Error, Result};
use crate::model::{Direction, RunLog};
use crate::store::{load_all_runs, require_run};

/// Print the best experiment — designed for shell scripts and agent loops.
///
/// Formats:
///   - "table" (default): human-readable multi-line summary
///   - "value": single val_bpb float, nothing else (for `$(resman best -f value)`)
///   - "json": compact JSON line
pub fn cmd_best(data_dir: &Path, tag: Option<&str>, format: &str) -> Result<()> {
    let runs: Vec<RunLog> = match tag {
        Some(t) => vec![require_run(data_dir, t)?],
        None => load_all_runs(data_dir)?,
    };

    if runs.is_empty() || runs.iter().all(|r| r.experiments.is_empty()) {
        return Err(Error::Empty);
    }

    // For each run, take its best under its own direction. Then across runs,
    // pick the overall winner — but we need a single direction to rank cross-run.
    // Use the first run's effective direction as the tiebreaker — when comparing
    // heterogeneous metrics, this is already a user error; surface a warning.
    let mut global_best: Option<(&RunLog, &crate::model::Experiment)> = None;
    for r in &runs {
        if let Some(b) = r.best() {
            let dir = b.effective_direction(r);
            match global_best {
                None => {
                    global_best = Some((r, b));
                }
                Some((gr, gb)) => {
                    let gdir = gb.effective_direction(gr);
                    if dir != gdir {
                        eprintln!(
                            "warning: comparing runs with different directions ({} vs {}); using first run's direction",
                            gdir.as_str(),
                            dir.as_str()
                        );
                    }
                    let better = match gdir {
                        Direction::Minimize => b.val_bpb < gb.val_bpb,
                        Direction::Maximize => b.val_bpb > gb.val_bpb,
                    };
                    if better {
                        global_best = Some((r, b));
                    }
                }
            }
        }
    }

    let (run, best) = global_best.ok_or(Error::Empty)?;
    let label = best.effective_metric_name(run);

    match format {
        "value" => println!("{:.6}", best.val_bpb),
        "json" => println!("{}", serde_json::to_string(best)?),
        _ => {
            println!("best experiment:");
            println!("  {}:     {:.6}", label, best.val_bpb);
            println!("  memory_gb:   {:.1}", best.memory_gb);
            println!("  commit:      {}", best.commit);
            println!("  description: {}", best.description);
        }
    }
    Ok(())
}
