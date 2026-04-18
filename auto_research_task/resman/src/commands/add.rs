use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use chrono::Local;

use crate::error::Result;
use crate::hw::detect_gpu_name;
use crate::logtail::tail_lines;
use crate::model::{Direction, Experiment, RunLog, Status};
use crate::store::{load_run, save_run};

pub struct AddOpts<'a> {
    pub tag: &'a str,
    pub commit: &'a str,
    pub val_bpb: f64,
    pub memory_gb: f64,
    pub status: &'a str,
    pub description: &'a str,
    pub params: &'a [String],
    pub parent: Option<&'a str>,
    pub log: Option<&'a Path>,
    pub no_gpu_probe: bool,
    pub metric_name: Option<&'a str>,
    pub metric_direction: Option<&'a str>,
    /// Pre-classified signals (from MCP path where the agent supplies the tail
    /// text directly rather than a file path). When `Some`, the classifier is
    /// not run again even if `log` is also set.
    pub preclassified_signals: Option<Vec<crate::signals::Signal>>,
}

pub fn cmd_add(data_dir: &Path, opts: AddOpts<'_>) -> Result<()> {
    let status = Status::from_str(opts.status)?;
    if status == Status::Verified {
        return Err(crate::error::Error::Custom(
            "status 'verified' can only be set via `resman verify`".to_string(),
        ));
    }

    // Parse metric_direction early so we fail fast on bad input.
    let parsed_direction: Option<Direction> =
        opts.metric_direction.map(Direction::from_str).transpose()?;

    let mut params_map = HashMap::new();
    for pair in opts.params {
        if let Some((k, v)) = pair.split_once('=') {
            params_map.insert(k.trim().to_string(), v.trim().to_string());
        } else {
            eprintln!("warning: ignoring malformed --param `{pair}` (expected key=value)");
        }
    }

    // Auto-capture GPU identity so every experiment is grounded to its hardware.
    // Upstream PR #102 asked for this for MFU accuracy; we use it as a first-class
    // param so `resman list --grep "H100"` works.
    if !opts.no_gpu_probe
        && !params_map.contains_key("gpu")
        && let Some(gpu) = detect_gpu_name()
    {
        params_map.insert("gpu".to_string(), gpu);
    }

    // Read the tail ONCE, then both classify and conditionally store as excerpt.
    let (crash_excerpt, signals) = if let Some(sigs) = opts.preclassified_signals {
        // MCP path: caller already ran classify() with the tail text.
        (None, sigs)
    } else {
        match opts.log {
            Some(log_path) => match tail_lines(log_path, 50) {
                Ok(tail) => {
                    let sigs = crate::signals::classify(&tail);
                    let excerpt = if status == Status::Crash {
                        Some(tail)
                    } else {
                        None
                    };
                    (excerpt, sigs)
                }
                Err(e) => {
                    eprintln!("warning: could not tail {}: {e}", log_path.display());
                    (None, Vec::new())
                }
            },
            None => (None, Vec::new()),
        }
    };

    let exp = Experiment {
        commit: opts.commit.to_string(),
        val_bpb: opts.val_bpb,
        memory_gb: opts.memory_gb,
        status,
        description: opts.description.to_string(),
        timestamp: Local::now().to_rfc3339(),
        params: params_map,
        parent_commit: opts.parent.map(str::to_string),
        crash_excerpt,
        metric_name: opts.metric_name.map(str::to_string),
        metric_direction: parsed_direction,
        signals,
    };

    let mut run = match load_run(data_dir, opts.tag)? {
        Some(r) => r,
        None => RunLog {
            experiments: Vec::new(),
            run_tag: opts.tag.to_string(),
            created_at: Local::now().to_rfc3339(),
            // When creating a new run, set run-level defaults from opts (first-set-wins).
            metric_name: opts.metric_name.map(str::to_string),
            metric_direction: parsed_direction,
        },
    };

    run.experiments.push(exp);
    let path = save_run(data_dir, &run)?;

    let n = run.experiments.len();
    let added_exp = &run.experiments[n - 1];
    println!("added experiment #{n} to `{}` ({})", opts.tag, status);
    if !added_exp.signals.is_empty() {
        let kinds: Vec<&str> = added_exp.signals.iter().map(|s| s.kind()).collect();
        println!("  signals: {}", kinds.join(", "));
    }
    if let Some(best) = run.best() {
        println!(
            "  current best: {}={:.6} ({})",
            best.effective_metric_name(&run),
            best.val_bpb,
            best.commit
        );
    }
    println!("  saved: {}", path.display());
    Ok(())
}

/// Convenience constructor for call sites that still pass a positional-ish API
/// (the CLI surface). Kept here so `main.rs` stays short.
#[allow(clippy::too_many_arguments)]
pub fn cmd_add_from_flags(
    data_dir: &Path,
    tag: &str,
    commit: &str,
    val_bpb: f64,
    memory_gb: f64,
    status: &str,
    description: &str,
    params: &[String],
    parent: Option<&str>,
    log: Option<&PathBuf>,
    no_gpu_probe: bool,
    metric_name: Option<&str>,
    metric_direction: Option<&str>,
) -> Result<()> {
    cmd_add(
        data_dir,
        AddOpts {
            tag,
            commit,
            val_bpb,
            memory_gb,
            status,
            description,
            params,
            parent,
            log: log.map(|p| p.as_path()),
            no_gpu_probe,
            metric_name,
            metric_direction,
            preclassified_signals: None,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signals::Signal;
    use crate::store::load_run;

    #[test]
    fn add_rejects_verified_status() {
        let data_dir = std::env::temp_dir().join("resman_test_add_no_verified");
        std::fs::create_dir_all(crate::store::runs_dir(&data_dir)).unwrap();

        let result = cmd_add(
            &data_dir,
            AddOpts {
                tag: "test_verified",
                commit: "abc999",
                val_bpb: 0.9,
                memory_gb: 0.0,
                status: "verified",
                description: "should be rejected",
                params: &[],
                parent: None,
                log: None,
                no_gpu_probe: true,
                metric_name: None,
                metric_direction: None,
                preclassified_signals: None,
            },
        );

        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("resman verify"),
            "expected helpful error pointing to resman verify, got: {msg}"
        );

        let _ = std::fs::remove_dir_all(&data_dir);
    }

    #[test]
    fn add_classifies_log_on_crash() {
        // Write a fake log to a temp file.
        let log_path = std::env::temp_dir().join("resman_test_crash.log");
        std::fs::write(
            &log_path,
            "step 42: loss=3.2\nRuntimeError: CUDA out of memory. Tried 4.00 GiB\n",
        )
        .unwrap();

        let data_dir = std::env::temp_dir().join("resman_test_add_classify");
        std::fs::create_dir_all(crate::store::runs_dir(&data_dir)).unwrap();

        cmd_add(
            &data_dir,
            AddOpts {
                tag: "test_classify",
                commit: "abc000",
                val_bpb: 0.0,
                memory_gb: 0.0,
                status: "crash",
                description: "oom test",
                params: &[],
                parent: None,
                log: Some(&log_path),
                no_gpu_probe: true,
                metric_name: None,
                metric_direction: None,
                preclassified_signals: None,
            },
        )
        .unwrap();

        let run = load_run(&data_dir, "test_classify").unwrap().unwrap();
        let exp = &run.experiments[0];
        assert!(
            exp.signals.iter().any(|s| matches!(s, Signal::Oom)),
            "expected Oom signal, got {:?}",
            exp.signals
        );
        // crash_excerpt should be set for crash status
        assert!(exp.crash_excerpt.is_some());

        // Cleanup
        let _ = std::fs::remove_dir_all(&data_dir);
        let _ = std::fs::remove_file(&log_path);
    }
}
