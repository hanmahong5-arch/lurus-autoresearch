//! Unexplored-neighbors computation for distill reports.

use crate::model::{Direction, Experiment, RunLog};
use crate::store::truncate;

use super::NeighborEntry;

pub(super) fn build_neighbors(
    run: &RunLog,
    best: &Experiment,
    direction: Direction,
    _metric_name: &str,
) -> Vec<NeighborEntry> {
    let best_val = best.val_bpb;
    let mut candidates: Vec<&Experiment> = run
        .experiments
        .iter()
        .filter(|e| e.commit != best.commit && e.val_bpb > 0.0)
        .collect();

    // Sort by absolute distance to best value.
    candidates.sort_by(|a, b| {
        (a.val_bpb - best_val)
            .abs()
            .partial_cmp(&(b.val_bpb - best_val).abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    candidates.truncate(3);

    candidates
        .into_iter()
        .map(|e| {
            let delta = match direction {
                Direction::Minimize => best_val - e.val_bpb,
                Direction::Maximize => e.val_bpb - best_val,
            };
            NeighborEntry {
                commit: e.commit.clone(),
                value: e.val_bpb,
                delta,
                description: truncate(&e.description, 60),
            }
        })
        .collect()
}
