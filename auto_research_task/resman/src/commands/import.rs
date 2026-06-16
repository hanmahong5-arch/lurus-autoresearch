use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::str::FromStr;

use chrono::Local;

use crate::cli::ImportSource;
use crate::error::{Error, Result};
use crate::model::{Direction, Experiment, RunLog, Status};
use crate::store::{load_run, save_run};

#[allow(clippy::too_many_arguments)]
pub fn cmd_import(
    data_dir: &Path,
    path: &Path,
    tag_override: Option<String>,
    force: bool,
    metric_name: Option<String>,
    metric_direction: Option<String>,
    source: ImportSource,
    metric_col: Option<String>,
) -> Result<()> {
    if !path.exists() {
        return Err(Error::NotFound(path.to_path_buf()));
    }

    let run_tag = tag_override.unwrap_or_else(|| {
        path.file_stem()
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

    let content = fs::read_to_string(path)?;

    // Effective metric name for the RunLog:
    //   - if metric_name (--metric-name) is Some, use it
    //   - else for wandb/mlflow default to the metric_col (strip metrics. prefix for mlflow)
    //   - for tsv keep None (current behavior)
    let effective_metric_name = metric_name.clone().or_else(|| match &source {
        ImportSource::Tsv => None,
        ImportSource::Wandb => metric_col.clone(),
        ImportSource::Mlflow => metric_col
            .as_deref()
            .map(|c| c.strip_prefix("metrics.").unwrap_or(c).to_string()),
    });

    let experiments = match source {
        ImportSource::Tsv => {
            // Detect a CSV file passed to the default tsv path.
            let first_nonempty = content.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
            if first_nonempty.contains(',') && !first_nonempty.contains('\t') {
                return Err(Error::Import(
                    "this file looks comma-separated, not tab-separated. \
                     If it's a wandb or mlflow export, re-run with \
                     `--from wandb --metric <column>` or `--from mlflow --metric <column>`. \
                     A plain resman TSV must use tab separators."
                        .to_string(),
                ));
            }
            parse_tsv(&content)?
        }
        ImportSource::Wandb => parse_wandb(&content, metric_col.as_deref())?,
        ImportSource::Mlflow => parse_mlflow(&content, metric_col.as_deref())?,
    };

    if experiments.is_empty() {
        eprintln!("warning: no data rows found in {}", path.display());
    }

    let run_log = RunLog {
        run_tag: run_tag.clone(),
        created_at: Local::now().to_rfc3339(),
        experiments,
        metric_name: effective_metric_name,
        metric_direction: parsed_direction,
        schema_version: 1,
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
    println!("imported {n} experiments from {}", path.display());
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

/// Build a name→index map from a header row, case-insensitive.
fn header_map(header: &[String]) -> HashMap<String, usize> {
    header
        .iter()
        .enumerate()
        .map(|(i, h)| (h.to_ascii_lowercase(), i))
        .collect()
}

/// Look up a column: exact match first (case-insensitive), then as provided.
fn col_index(map: &HashMap<String, usize>, name: &str) -> Option<usize> {
    map.get(&name.to_ascii_lowercase()).copied()
}

fn get_cell(row: &[String], idx: Option<usize>) -> &str {
    idx.and_then(|i| row.get(i))
        .map(|s| s.as_str())
        .unwrap_or("")
}

fn parse_metric_cell(cell: &str, row_idx: usize, col_name: &str) -> f64 {
    if cell.is_empty() {
        return 0.0;
    }
    cell.parse::<f64>().unwrap_or_else(|_| {
        eprintln!(
            "warning: row {row_idx}: cannot parse metric column '{col_name}' value '{cell}' as f64; using 0.0"
        );
        0.0
    })
}

fn wandb_candidate_metrics(header: &[String], rows: &[Vec<String>]) -> Vec<String> {
    let meta: std::collections::HashSet<&str> = ["id", "name", "state", "notes", "created"]
        .iter()
        .copied()
        .collect();
    let non_meta: Vec<(usize, &String)> = header
        .iter()
        .enumerate()
        .filter(|(_, h)| !meta.contains(h.to_ascii_lowercase().as_str()))
        .collect();

    // Prefer columns whose first non-empty data cell parses as f64.
    let numeric: Vec<String> = non_meta
        .iter()
        .filter(|(ci, _)| {
            rows.iter().skip(1).any(|row| {
                let cell = row.get(*ci).map(|s| s.as_str()).unwrap_or("");
                !cell.is_empty() && cell.parse::<f64>().is_ok()
            })
        })
        .map(|(_, h)| (*h).clone())
        .collect();

    if !numeric.is_empty() {
        numeric
    } else {
        non_meta.into_iter().map(|(_, h)| h.clone()).collect()
    }
}

fn parse_wandb(content: &str, metric_col: Option<&str>) -> Result<Vec<Experiment>> {
    let rows = crate::csv::parse_csv(content);
    if rows.is_empty() {
        return Ok(Vec::new());
    }

    let header = &rows[0];
    let map = header_map(header);

    // If no --metric was given, detect candidates and error with guidance.
    let metric_col = match metric_col {
        None => {
            let candidates = wandb_candidate_metrics(header, &rows);
            const MAX_SHOWN: usize = 12;
            let (shown, truncated) = if candidates.len() > MAX_SHOWN {
                (candidates[..MAX_SHOWN].to_vec(), true)
            } else {
                (candidates.clone(), false)
            };
            let mut list = shown.join(", ");
            if truncated {
                list.push_str(" …");
            }
            let (label, example) = if candidates.is_empty() {
                (
                    format!("  available columns: {}", header.join(", ")),
                    String::new(),
                )
            } else {
                (
                    format!("  detected metric columns: {list}"),
                    format!("\n  re-run with e.g. --metric {}", candidates[0]),
                )
            };
            return Err(Error::Import(format!(
                "--from wandb needs --metric <column> — these tools log many metrics, \
                 so resman won't guess your objective.\n{label}{example}"
            )));
        }
        Some(c) => c,
    };

    // Verify metric column exists
    let metric_idx = col_index(&map, metric_col).ok_or_else(|| {
        Error::Import(format!(
            "metric column '{}' not found; available: {}",
            metric_col,
            header.join(", ")
        ))
    })?;

    let id_idx = col_index(&map, "id");
    let name_idx = col_index(&map, "name");
    let state_idx = col_index(&map, "state");
    let notes_idx = col_index(&map, "notes");
    let created_idx = col_index(&map, "created");

    // Columns to exclude from params
    let excluded: std::collections::HashSet<usize> = [
        id_idx,
        name_idx,
        Some(metric_idx),
        state_idx,
        notes_idx,
        created_idx,
    ]
    .into_iter()
    .flatten()
    .collect();

    let mut experiments = Vec::new();
    for (i, row) in rows.iter().skip(1).enumerate() {
        let id_cell = get_cell(row, id_idx);
        let name_cell = get_cell(row, name_idx);
        let commit = if !id_cell.is_empty() {
            id_cell.to_string()
        } else if !name_cell.is_empty() {
            name_cell.to_string()
        } else {
            format!("row{i}")
        };

        let metric_cell = get_cell(row, Some(metric_idx));
        let val_bpb = parse_metric_cell(metric_cell, i + 1, metric_col);

        let state_cell = get_cell(row, state_idx);
        let status = match state_cell.to_ascii_lowercase().as_str() {
            "finished" => Status::Keep,
            "crashed" | "failed" | "killed" | "preempted" => Status::Crash,
            _ => Status::Keep,
        };

        let notes_cell = get_cell(row, notes_idx);
        let description = if !notes_cell.is_empty() {
            notes_cell.to_string()
        } else if !name_cell.is_empty() {
            name_cell.to_string()
        } else {
            String::new()
        };

        let timestamp = get_cell(row, created_idx).to_string();

        // Collect params from non-excluded columns
        let mut params = HashMap::new();
        for (col_idx, col_name) in header.iter().enumerate() {
            if excluded.contains(&col_idx) {
                continue;
            }
            let cell = get_cell(row, Some(col_idx));
            if !cell.is_empty() {
                params.insert(col_name.clone(), cell.to_string());
            }
        }

        experiments.push(Experiment {
            commit,
            val_bpb,
            memory_gb: 0.0,
            status,
            description,
            timestamp,
            params,
            parent_commit: None,
            crash_excerpt: None,
            metric_name: None,
            metric_direction: None,
            signals: Vec::new(),
        });
    }

    Ok(experiments)
}

fn parse_mlflow(content: &str, metric_col: Option<&str>) -> Result<Vec<Experiment>> {
    let rows = crate::csv::parse_csv(content);
    if rows.is_empty() {
        return Ok(Vec::new());
    }

    let header = &rows[0];
    let map = header_map(header);

    // If no --metric was given, detect candidates and error with guidance.
    let metric_col = match metric_col {
        None => {
            // Candidates: columns starting with "metrics." (case-insensitive), prefix stripped
            // using original-case suffix for display.
            let candidates: Vec<String> = header
                .iter()
                .filter_map(|h| {
                    if h.to_ascii_lowercase().starts_with("metrics.") {
                        Some(h["metrics.".len()..].to_string())
                    } else {
                        None
                    }
                })
                .collect();

            const MAX_SHOWN: usize = 12;
            let (shown, truncated) = if candidates.len() > MAX_SHOWN {
                (candidates[..MAX_SHOWN].to_vec(), true)
            } else {
                (candidates.clone(), false)
            };
            let mut list = shown.join(", ");
            if truncated {
                list.push_str(" …");
            }
            let (label, example) = if candidates.is_empty() {
                (
                    format!("  available columns: {}", header.join(", ")),
                    String::new(),
                )
            } else {
                (
                    format!("  detected metric columns: {list}"),
                    format!("\n  re-run with e.g. --metric {}", candidates[0]),
                )
            };
            return Err(Error::Import(format!(
                "--from mlflow needs --metric <column> — these tools log many metrics, \
                 so resman won't guess your objective.\n{label}{example}"
            )));
        }
        Some(c) => c,
    };

    // Accept metric_col verbatim or with metrics. prefix
    let resolved_metric_idx =
        col_index(&map, metric_col).or_else(|| col_index(&map, &format!("metrics.{metric_col}")));

    let metric_idx = resolved_metric_idx.ok_or_else(|| {
        Error::Import(format!(
            "metric column '{}' not found (tried '{}' and 'metrics.{}'); available: {}",
            metric_col,
            metric_col,
            metric_col,
            header.join(", ")
        ))
    })?;

    let run_id_idx = col_index(&map, "run_id");
    let status_idx = col_index(&map, "status");
    let start_time_idx = col_index(&map, "start_time");
    let run_name_idx = col_index(&map, "tags.mlflow.runname");

    // Columns to exclude from params (non-param columns)
    let excluded: std::collections::HashSet<usize> = [
        run_id_idx,
        Some(metric_idx),
        status_idx,
        start_time_idx,
        run_name_idx,
    ]
    .into_iter()
    .flatten()
    .collect();

    let mut experiments = Vec::new();
    for (i, row) in rows.iter().skip(1).enumerate() {
        let run_id_cell = get_cell(row, run_id_idx);
        let commit = if !run_id_cell.is_empty() {
            run_id_cell.to_string()
        } else {
            format!("row{i}")
        };

        let metric_cell = get_cell(row, Some(metric_idx));
        let val_bpb = parse_metric_cell(metric_cell, i + 1, metric_col);

        let status_cell = get_cell(row, status_idx);
        let status = match status_cell.to_ascii_uppercase().as_str() {
            "FINISHED" => Status::Keep,
            "FAILED" | "KILLED" => Status::Crash,
            _ => Status::Keep,
        };

        let run_name_cell = get_cell(row, run_name_idx);
        let description = if !run_name_cell.is_empty() {
            run_name_cell.to_string()
        } else {
            String::new()
        };

        let timestamp = get_cell(row, start_time_idx).to_string();

        // Collect params from columns starting with "params."
        let mut params = HashMap::new();
        for (col_idx, col_name) in header.iter().enumerate() {
            if excluded.contains(&col_idx) {
                continue;
            }
            if let Some(key) = col_name.strip_prefix("params.") {
                let cell = get_cell(row, Some(col_idx));
                if !cell.is_empty() {
                    params.insert(key.to_string(), cell.to_string());
                }
            }
        }

        experiments.push(Experiment {
            commit,
            val_bpb,
            memory_gb: 0.0,
            status,
            description,
            timestamp,
            params,
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
    use crate::store::load_run;
    use tempfile::TempDir;

    // ---- parse_tsv tests (preserved) ----

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

    // ---- parse_wandb tests ----

    fn wandb_csv() -> &'static str {
        "ID,Name,State,Created,eval/loss,lr,notes\n\
         run001,baseline,finished,2024-01-01,0.987,0.001,good start\n\
         run002,bigger-lr,crashed,,0.0,0.01,\n\
         run003,\"run with, comma\",finished,2024-01-03,0.975,0.001,best so far\n"
    }

    #[test]
    fn wandb_status_mapping() {
        let exps = parse_wandb(wandb_csv(), Some("eval/loss")).unwrap();
        assert_eq!(exps.len(), 3);
        assert_eq!(exps[0].status, Status::Keep);
        assert_eq!(exps[1].status, Status::Crash);
        assert_eq!(exps[2].status, Status::Keep);
    }

    #[test]
    fn wandb_empty_metric_is_zero() {
        let exps = parse_wandb(wandb_csv(), Some("eval/loss")).unwrap();
        assert_eq!(exps[1].val_bpb, 0.0);
    }

    #[test]
    fn wandb_commit_from_id() {
        let exps = parse_wandb(wandb_csv(), Some("eval/loss")).unwrap();
        assert_eq!(exps[0].commit, "run001");
    }

    #[test]
    fn wandb_params_populated() {
        let exps = parse_wandb(wandb_csv(), Some("eval/loss")).unwrap();
        // "lr" is not in excluded set for wandb
        assert_eq!(exps[0].params.get("lr"), Some(&"0.001".to_string()));
    }

    #[test]
    fn wandb_metric_col_not_found_error() {
        let result = parse_wandb(wandb_csv(), Some("nonexistent_metric"));
        assert!(
            matches!(result, Err(Error::Import(_))),
            "expect ImportError for missing metric column"
        );
    }

    #[test]
    fn wandb_quoted_field_with_comma() {
        let exps = parse_wandb(wandb_csv(), Some("eval/loss")).unwrap();
        // run003 name has a comma in it
        assert_eq!(exps[2].commit, "run003");
    }

    #[test]
    fn wandb_missing_metric_lists_columns() {
        let result = parse_wandb(wandb_csv(), None);
        match result {
            Err(Error::Import(msg)) => {
                assert!(msg.contains("needs --metric"), "msg: {msg}");
                assert!(msg.contains("eval/loss"), "msg: {msg}");
            }
            other => panic!("expected Err(Import), got {other:?}"),
        }
    }

    // ---- parse_mlflow tests ----

    fn mlflow_csv() -> &'static str {
        "run_id,status,start_time,metrics.loss,params.lr,params.optimizer,tags.mlflow.runName\n\
         abc123,FINISHED,2024-01-01,0.543,0.001,adam,run-alpha\n\
         def456,FAILED,2024-01-02,0.0,0.01,sgd,run-beta\n\
         ghi789,FINISHED,2024-01-03,0.412,0.0001,adamw,\"run,gamma\"\n"
    }

    #[test]
    fn mlflow_run_id_becomes_commit() {
        let exps = parse_mlflow(mlflow_csv(), Some("loss")).unwrap();
        assert_eq!(exps[0].commit, "abc123");
    }

    #[test]
    fn mlflow_failed_becomes_crash() {
        let exps = parse_mlflow(mlflow_csv(), Some("loss")).unwrap();
        assert_eq!(exps[1].status, Status::Crash);
    }

    #[test]
    fn mlflow_finished_becomes_keep() {
        let exps = parse_mlflow(mlflow_csv(), Some("loss")).unwrap();
        assert_eq!(exps[0].status, Status::Keep);
    }

    #[test]
    fn mlflow_params_prefix_stripped() {
        let exps = parse_mlflow(mlflow_csv(), Some("loss")).unwrap();
        assert_eq!(exps[0].params.get("lr"), Some(&"0.001".to_string()));
        assert_eq!(exps[0].params.get("optimizer"), Some(&"adam".to_string()));
        // Should NOT have "params.lr" key
        assert!(!exps[0].params.contains_key("params.lr"));
    }

    #[test]
    fn mlflow_accepts_metric_col_with_metrics_prefix() {
        // When user passes "metrics.loss", it should also work
        let exps = parse_mlflow(mlflow_csv(), Some("metrics.loss")).unwrap();
        assert_eq!(exps.len(), 3);
        assert!((exps[0].val_bpb - 0.543).abs() < 1e-9);
    }

    #[test]
    fn mlflow_accepts_metric_col_without_prefix() {
        let exps = parse_mlflow(mlflow_csv(), Some("loss")).unwrap();
        assert_eq!(exps.len(), 3);
        assert!((exps[0].val_bpb - 0.543).abs() < 1e-9);
    }

    #[test]
    fn mlflow_metric_col_not_found_error() {
        let result = parse_mlflow(mlflow_csv(), Some("nonexistent"));
        assert!(
            matches!(result, Err(Error::Import(_))),
            "expect ImportError for missing metric column"
        );
    }

    #[test]
    fn mlflow_missing_metric_lists_columns() {
        let result = parse_mlflow(mlflow_csv(), None);
        match result {
            Err(Error::Import(msg)) => {
                assert!(msg.contains("needs --metric"), "msg: {msg}");
                assert!(msg.contains("loss"), "msg: {msg}");
            }
            other => panic!("expected Err(Import), got {other:?}"),
        }
    }

    // ---- end-to-end cmd_import tests ----

    fn init_dir(tmp: &TempDir) {
        let runs_dir = tmp.path().join("runs");
        std::fs::create_dir_all(&runs_dir).unwrap();
    }

    #[test]
    fn tsv_source_detects_csv_file() {
        let tmp = TempDir::new().unwrap();
        init_dir(&tmp);
        let csv_path = tmp.path().join("looks.csv");
        std::fs::write(
            &csv_path,
            "ID,Name,State,Created,eval/loss\nrun001,baseline,finished,2024-01-01,0.987\n",
        )
        .unwrap();
        let result = cmd_import(
            tmp.path(),
            &csv_path,
            Some("w".to_string()),
            false,
            None,
            None,
            ImportSource::Tsv,
            None,
        );
        match result {
            Err(Error::Import(msg)) => {
                assert!(msg.contains("comma-separated"), "msg: {msg}");
                assert!(msg.contains("--from"), "msg: {msg}");
            }
            other => panic!("expected Err(Import), got {other:?}"),
        }
    }

    #[test]
    fn tsv_source_valid_tsv_still_works() {
        let tmp = TempDir::new().unwrap();
        init_dir(&tmp);
        let tsv_path = tmp.path().join("valid.tsv");
        std::fs::write(
            &tsv_path,
            "commit\tval_bpb\tmemory_gb\tstatus\tdescription\n\
             abc1234\t0.9979\t44.0\tkeep\tbaseline\n",
        )
        .unwrap();
        cmd_import(
            tmp.path(),
            &tsv_path,
            Some("validtsv".to_string()),
            false,
            None,
            None,
            ImportSource::Tsv,
            None,
        )
        .unwrap();
        let run = load_run(tmp.path(), "validtsv").unwrap().unwrap();
        assert_eq!(run.experiments.len(), 1);
    }

    #[test]
    fn end_to_end_wandb_import() {
        let tmp = TempDir::new().unwrap();
        init_dir(&tmp);

        // Write a small wandb CSV fixture
        let csv_path = tmp.path().join("wb.csv");
        std::fs::write(
            &csv_path,
            "ID,Name,State,Created,eval/loss,lr,notes\n\
             r1,baseline,finished,2024-01-01,0.987,0.001,ok\n\
             r2,bigger,finished,2024-01-02,0.965,0.01,better\n\
             r3,crash,crashed,,0.0,0.1,bad\n",
        )
        .unwrap();

        cmd_import(
            tmp.path(),
            &csv_path,
            Some("wbtest".to_string()),
            false,
            None,
            None,
            ImportSource::Wandb,
            Some("eval/loss".to_string()),
        )
        .unwrap();

        let run = load_run(tmp.path(), "wbtest").unwrap().unwrap();
        assert_eq!(run.experiments.len(), 3);
        // best() must be Some and finite
        let best = run.best().expect("best must exist");
        assert!(best.val_bpb.is_finite());
        assert!(best.val_bpb > 0.0);
    }

    #[test]
    fn end_to_end_mlflow_import() {
        let tmp = TempDir::new().unwrap();
        init_dir(&tmp);

        let csv_path = tmp.path().join("ml.csv");
        std::fs::write(
            &csv_path,
            "run_id,status,start_time,metrics.loss,params.lr,params.optimizer,tags.mlflow.runName\n\
             abc,FINISHED,2024-01-01,0.543,0.001,adam,run-a\n\
             def,FAILED,2024-01-02,0.0,0.01,sgd,run-b\n\
             ghi,FINISHED,2024-01-03,0.412,0.0001,adamw,run-c\n",
        )
        .unwrap();

        cmd_import(
            tmp.path(),
            &csv_path,
            Some("mltest".to_string()),
            false,
            None,
            None,
            ImportSource::Mlflow,
            Some("loss".to_string()),
        )
        .unwrap();

        let run = load_run(tmp.path(), "mltest").unwrap().unwrap();
        assert_eq!(run.experiments.len(), 3);
        let best = run.best().expect("best must exist");
        assert!(best.val_bpb.is_finite());
        assert!(best.val_bpb > 0.0);
    }
}
