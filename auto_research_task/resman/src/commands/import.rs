use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::str::FromStr;

use chrono::Local;

use crate::error::{Error, Result};
use crate::model::{Direction, Experiment, RunLog, Status};
use crate::store::{load_run, save_run};

pub fn cmd_import(
    data_dir: &Path,
    tsv_path: &Path,
    tag_override: Option<String>,
    force: bool,
    metric_name: Option<String>,
    metric_direction: Option<String>,
) -> Result<()> {
    if !tsv_path.exists() {
        return Err(Error::NotFound(tsv_path.to_path_buf()));
    }

    let run_tag = tag_override.unwrap_or_else(|| {
        tsv_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("untagged")
            .to_string()
    });

    if !force && load_run(data_dir, &run_tag)?.is_some() {
        return Err(Error::DuplicateTag(run_tag));
    }

    // Parse direction early for fail-fast behavior.
    let parsed_direction: Option<Direction> = metric_direction
        .as_deref()
        .map(Direction::from_str)
        .transpose()?;

    let content = fs::read_to_string(tsv_path)?;
    let experiments = parse_tsv(&content)?;

    if experiments.is_empty() {
        eprintln!("warning: no data rows found in {}", tsv_path.display());
    }

    let run_log = RunLog {
        run_tag: run_tag.clone(),
        created_at: Local::now().to_rfc3339(),
        experiments,
        metric_name,
        metric_direction: parsed_direction,
    };

    let out_path = save_run(data_dir, &run_log)?;

    let n = run_log.experiments.len();
    let kept = run_log
        .experiments
        .iter()
        .filter(|e| e.status.is_kept())
        .count();
    let crashed = run_log
        .experiments
        .iter()
        .filter(|e| e.status == Status::Crash)
        .count();
    println!("imported {n} experiments from {}", tsv_path.display());
    if let Some(best) = run_log.best() {
        println!(
            "  best {}: {:.6}  ({})",
            best.effective_metric_name(&run_log),
            best.val_bpb,
            best.commit
        );
    }
    println!("  kept: {kept}  crashed: {crashed}");
    println!("  saved: {}", out_path.display());
    Ok(())
}

fn parse_tsv(content: &str) -> Result<Vec<Experiment>> {
    let mut lines = content.lines().enumerate();
    // Detect and skip header line (starts with "commit" or contains "val_bpb")
    let first = lines.next();
    let mut experiments = Vec::new();

    let rows: Vec<(usize, &str)> = match first {
        Some((_, first_line))
            if first_line.starts_with("commit") || first_line.contains("val_bpb") =>
        {
            lines.collect()
        }
        Some(l) => std::iter::once(l).chain(lines).collect(),
        None => return Ok(experiments),
    };

    for (i, line) in rows {
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 4 {
            return Err(Error::MalformedTsv {
                line: i + 1,
                got: cols.len(),
            });
        }

        let commit = cols[0].trim().to_string();
        let val_bpb: f64 = cols[1].trim().parse().map_err(|_| Error::InvalidFloat {
            line: i + 1,
            column: "val_bpb",
            value: cols[1].to_string(),
        })?;
        let memory_gb: f64 = cols[2].trim().parse().map_err(|_| Error::InvalidFloat {
            line: i + 1,
            column: "memory_gb",
            value: cols[2].to_string(),
        })?;
        let status = Status::from_str(cols[3].trim())?;
        let description = if cols.len() > 4 {
            cols[4..].join("\t")
        } else {
            String::new()
        };

        experiments.push(Experiment {
            commit,
            val_bpb,
            memory_gb,
            status,
            description,
            timestamp: String::new(),
            params: HashMap::new(),
            parent_commit: None,
            crash_excerpt: None,
            metric_name: None,
            metric_direction: None,
            signals: Vec::new(),
        });
    }

    Ok(experiments)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_with_header() {
        let tsv = "commit\tval_bpb\tmemory_gb\tstatus\tdescription\n\
                   abc1234\t0.997900\t44.0\tkeep\tbaseline\n\
                   def5678\t1.005000\t44.2\tdiscard\tGeLU activation\n";
        let out = parse_tsv(tsv).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].commit, "abc1234");
        assert_eq!(out[0].status, Status::Keep);
        assert_eq!(out[1].status, Status::Discard);
    }

    #[test]
    fn parses_without_header() {
        let tsv = "abc1234\t0.997900\t44.0\tkeep\tbaseline\n";
        let out = parse_tsv(tsv).unwrap();
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn rejects_short_rows() {
        let tsv = "commit\tval_bpb\tmemory_gb\tstatus\n\
                   abc1234\t0.9\t44.0\n";
        assert!(matches!(parse_tsv(tsv), Err(Error::MalformedTsv { .. })));
    }

    #[test]
    fn preserves_tabs_in_description() {
        let tsv = "abc\t0.9\t44.0\tkeep\thas\ttabs\there";
        let out = parse_tsv(tsv).unwrap();
        assert_eq!(out[0].description, "has\ttabs\there");
    }
}
