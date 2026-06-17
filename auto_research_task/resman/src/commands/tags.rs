//! `resman tags` — per-tag snapshot.
//!
//! Lightweight overview of every tag in the store: experiment count, best
//! metric, last update, schema_version. Complements `resman stats`
//! (aggregate numbers) and `resman list` (per-experiment rows) with the
//! "what tags do I have?" answer. Also exposed as MCP tool `resman_tags`.

use std::path::Path;

use serde::Serialize;

use crate::cli::OutputFormat;
use crate::error::Result;
use crate::model::{Direction, RunLog};
use crate::store::load_all_runs;

#[derive(Debug, Serialize)]
pub struct TagSnapshot {
    pub tag: String,
    pub experiment_count: usize,
    pub best_commit: Option<String>,
    pub best_value: Option<f64>,
    pub metric_name: String,
    pub direction: String,
    pub last_update: Option<String>,
    pub schema_version: u32,
}

fn build_snapshot(run: &RunLog) -> TagSnapshot {
    let best = run.best();
    let direction = run
        .metric_direction
        .or_else(|| run.experiments.first().and_then(|e| e.metric_direction))
        .unwrap_or(Direction::Minimize);
    let metric_name = run
        .metric_name
        .clone()
        .or_else(|| run.experiments.first().and_then(|e| e.metric_name.clone()))
        .unwrap_or_else(|| "val_bpb".to_string());

    let last_update = run
        .experiments
        .iter()
        .map(|e| e.timestamp.as_str())
        .filter(|s| !s.is_empty())
        .max()
        .map(|s| s.to_string());

    TagSnapshot {
        tag: run.run_tag.clone(),
        experiment_count: run.experiments.len(),
        best_commit: best.map(|e| e.commit.clone()),
        best_value: best.map(|e| e.val_bpb),
        metric_name,
        direction: direction.as_str().to_string(),
        last_update,
        schema_version: run.schema_version,
    }
}

/// Returns one snapshot per tag, sorted by `last_update` desc (most-recently
/// touched first). Tags with no timestamps fall to the tail in tag-name order.
pub fn build_tag_snapshots(data_dir: &Path) -> Result<Vec<TagSnapshot>> {
    let runs = load_all_runs(data_dir)?;
    let mut snaps: Vec<TagSnapshot> = runs.iter().map(build_snapshot).collect();
    snaps.sort_by(|a, b| match (&b.last_update, &a.last_update) {
        (Some(bx), Some(ax)) => bx.cmp(ax),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.tag.cmp(&b.tag),
    });
    Ok(snaps)
}

pub fn cmd_tags(data_dir: &Path, format: &OutputFormat) -> Result<()> {
    let snaps = build_tag_snapshots(data_dir)?;

    if snaps.is_empty() {
        match format {
            OutputFormat::Json => println!("[]"),
            OutputFormat::Tsv => println!("tag\tn\tbest_value\tbest_commit\tmetric\tlast_update"),
            OutputFormat::Table => {
                println!(
                    "{}",
                    crate::term::empty_state(
                        "(no tags in this store — run `resman init` then `resman add`)"
                    )
                )
            }
        }
        return Ok(());
    }

    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&snaps)?);
        }
        OutputFormat::Tsv => {
            println!("tag\tn\tbest_value\tbest_commit\tmetric\tlast_update");
            for s in &snaps {
                let bv = s
                    .best_value
                    .map(|v| format!("{v:.6}"))
                    .unwrap_or_else(|| "—".to_string());
                let bc = s.best_commit.as_deref().unwrap_or("—");
                let lu = s.last_update.as_deref().unwrap_or("—");
                println!(
                    "{}\t{}\t{}\t{}\t{}\t{}",
                    s.tag, s.experiment_count, bv, bc, s.metric_name, lu
                );
            }
        }
        OutputFormat::Table => {
            let n = snaps.len();
            println!(
                "{}",
                crate::term::section_header("tags", Some(&format!("{n} tag(s)")))
            );
            println!(
                "{:<20}  {:>4}  {:>11}  {:<11}  {:<10}  last_update",
                "tag", "n", "best_value", "best_commit", "metric"
            );
            println!("{}", crate::term::rule());
            for s in &snaps {
                let bv = s
                    .best_value
                    .map(|v| format!("{v:.6}"))
                    .unwrap_or_else(|| "—".to_string());
                let bc = s
                    .best_commit
                    .as_deref()
                    .map(|c| c.chars().take(11).collect::<String>())
                    .unwrap_or_else(|| "—".to_string());
                let lu = s.last_update.as_deref().unwrap_or("—");
                let metric = if s.metric_name.len() > 10 {
                    &s.metric_name[..10]
                } else {
                    &s.metric_name
                };
                println!(
                    "{:<20}  {:>4}  {:>11}  {:<11}  {:<10}  {}",
                    s.tag, s.experiment_count, bv, bc, metric, lu
                );
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Experiment, Status};
    use crate::store::{ensure_initialized, save_run};

    fn make_exp(commit: &str, val: f64, ts: &str) -> Experiment {
        Experiment {
            commit: commit.to_string(),
            val_bpb: val,
            memory_gb: 0.0,
            status: Status::Keep,
            description: "x".to_string(),
            timestamp: ts.to_string(),
            params: std::collections::HashMap::new(),
            parent_commit: None,
            crash_excerpt: None,
            metric_name: None,
            metric_direction: None,
            signals: vec![],
        }
    }

    fn write_run(dir: &Path, tag: &str, exps: Vec<Experiment>) {
        let run = RunLog {
            run_tag: tag.to_string(),
            created_at: "2026-05-16T00:00:00Z".to_string(),
            experiments: exps,
            metric_name: None,
            metric_direction: None,
            schema_version: 1,
        };
        save_run(dir, &run).unwrap();
    }

    #[test]
    fn tag_snapshots_empty_store() {
        let dir = tempfile::tempdir().unwrap();
        ensure_initialized(dir.path()).unwrap();
        let snaps = build_tag_snapshots(dir.path()).unwrap();
        assert!(snaps.is_empty());
    }

    #[test]
    fn tag_snapshots_single_tag_basic_fields() {
        let dir = tempfile::tempdir().unwrap();
        ensure_initialized(dir.path()).unwrap();
        write_run(
            dir.path(),
            "alpha",
            vec![
                make_exp("c1", 0.99, "2026-05-16T00:00:00Z"),
                make_exp("c2", 0.95, "2026-05-16T01:00:00Z"),
            ],
        );
        let snaps = build_tag_snapshots(dir.path()).unwrap();
        assert_eq!(snaps.len(), 1);
        let s = &snaps[0];
        assert_eq!(s.tag, "alpha");
        assert_eq!(s.experiment_count, 2);
        assert_eq!(s.best_commit.as_deref(), Some("c2"));
        assert!(s.best_value.is_some());
        assert!((s.best_value.unwrap() - 0.95).abs() < f64::EPSILON);
        assert_eq!(s.metric_name, "val_bpb");
        assert_eq!(s.direction, "minimize");
        assert_eq!(
            s.last_update.as_deref(),
            Some("2026-05-16T01:00:00Z"),
            "last_update = max of timestamps"
        );
        assert_eq!(s.schema_version, 1);
    }

    #[test]
    fn tag_snapshots_sorted_by_last_update_desc() {
        let dir = tempfile::tempdir().unwrap();
        ensure_initialized(dir.path()).unwrap();
        write_run(
            dir.path(),
            "old",
            vec![make_exp("o1", 0.98, "2026-05-01T00:00:00Z")],
        );
        write_run(
            dir.path(),
            "new",
            vec![make_exp("n1", 0.99, "2026-05-16T00:00:00Z")],
        );
        write_run(
            dir.path(),
            "mid",
            vec![make_exp("m1", 0.97, "2026-05-10T00:00:00Z")],
        );
        let snaps = build_tag_snapshots(dir.path()).unwrap();
        let order: Vec<&str> = snaps.iter().map(|s| s.tag.as_str()).collect();
        assert_eq!(order, vec!["new", "mid", "old"]);
    }

    #[test]
    fn tag_snapshots_table_render_does_not_panic_with_long_metric_name() {
        let dir = tempfile::tempdir().unwrap();
        ensure_initialized(dir.path()).unwrap();
        let mut exp = make_exp("c1", 0.99, "2026-05-16T00:00:00Z");
        exp.metric_name = Some("very_long_metric_name_that_exceeds_10_chars".to_string());
        write_run(dir.path(), "longmetric", vec![exp]);
        // Should not panic regardless of metric_name length.
        cmd_tags(dir.path(), &OutputFormat::Table).unwrap();
    }
}
