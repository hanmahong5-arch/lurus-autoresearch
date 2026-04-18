//! `resman verify` — re-verify an experiment by providing a re-run metric value.
//!
//! Does NOT run training. The caller provides the new value. Resman compares
//! it against the recorded original within the given tolerance and, if it
//! passes, promotes the experiment's status to `Verified` and updates val_bpb.

use std::path::Path;

use crate::error::{Error, Result};
use crate::model::{Direction, Status};
use crate::store::{load_all_runs, load_run, save_run};

pub struct VerifyOpts<'a> {
    pub commit: &'a str,
    pub new_value: f64,
    pub tolerance: f64,
    pub tag: Option<&'a str>,
}

pub fn cmd_verify(data_dir: &Path, opts: VerifyOpts<'_>) -> Result<()> {
    let text = verify_inner(data_dir, &opts)?;
    println!("{text}");
    Ok(())
}

/// Core logic, also called from the MCP tool.
pub fn verify_inner(data_dir: &Path, opts: &VerifyOpts<'_>) -> Result<String> {
    if opts.tolerance < 0.0 {
        return Err(Error::Custom("tolerance must be non-negative".to_string()));
    }

    // Collect candidate (run_tag, experiment_index) pairs whose commit starts with `opts.commit`.
    let runs = match opts.tag {
        Some(t) => match load_run(data_dir, t)? {
            Some(r) => vec![r],
            None => {
                return Err(Error::Custom(format!("no such tag: {}", opts.tag.unwrap())));
            }
        },
        None => load_all_runs(data_dir)?,
    };

    // Find matching experiments by commit prefix.
    let mut matches: Vec<(String, usize)> = Vec::new(); // (tag, exp_index)
    for run in &runs {
        for (idx, exp) in run.experiments.iter().enumerate() {
            if exp.commit.starts_with(opts.commit) || opts.commit.starts_with(&*exp.commit) {
                matches.push((run.run_tag.clone(), idx));
            }
        }
    }

    if matches.is_empty() {
        return Err(Error::Custom(format!(
            "no experiment found with commit starting with `{}`",
            opts.commit
        )));
    }

    if matches.len() > 1 {
        let candidates: Vec<String> = matches
            .iter()
            .map(|(tag, idx)| {
                let run = runs.iter().find(|r| r.run_tag == *tag).unwrap();
                let exp = &run.experiments[*idx];
                format!("  [{tag}] {}", exp.commit)
            })
            .collect();
        return Err(Error::Custom(format!(
            "ambiguous commit `{}` — matches:\n{}",
            opts.commit,
            candidates.join("\n")
        )));
    }

    let (ref_tag, ref_idx) = matches.into_iter().next().unwrap();

    // Reload the specific run mutably.
    let mut run = load_run(data_dir, &ref_tag)?
        .ok_or_else(|| Error::Custom(format!("tag `{ref_tag}` disappeared")))?;

    let exp = &run.experiments[ref_idx];

    // Gate on status.
    if exp.status == Status::Crash {
        return Err(Error::Custom(format!(
            "cannot verify a crash experiment (commit {}, tag {ref_tag})",
            exp.commit
        )));
    }

    let original = exp.val_bpb;
    let direction = exp.effective_direction(&run);
    let metric = exp.effective_metric_name(&run).to_string();
    let commit_short = exp.commit.clone();
    let old_status = exp.status;

    let delta = opts.new_value - original;
    let passes = match direction {
        Direction::Minimize => opts.new_value <= original + opts.tolerance,
        Direction::Maximize => opts.new_value >= original - opts.tolerance,
    };

    if passes {
        let re_verify = old_status == Status::Verified;
        run.experiments[ref_idx].status = Status::Verified;
        run.experiments[ref_idx].val_bpb = opts.new_value;
        save_run(data_dir, &run)?;

        let dir_str = direction.as_str();
        let action = if re_verify { "re-verified" } else { "verified" };
        let status_transition = if re_verify {
            "verified → verified".to_string()
        } else {
            format!("{old_status} → verified")
        };

        Ok(format!(
            "{action} {commit_short} on tag {ref_tag}\n  metric ({metric}, {dir_str})\n    original:  {original:.6}\n    new:       {nv:.6}\n    delta:     {delta:+.6}\n    tolerance: {tol:.6}\n  status: {status_transition}",
            nv = opts.new_value,
            tol = opts.tolerance,
        ))
    } else {
        let exceeded = match direction {
            Direction::Minimize => opts.new_value - (original + opts.tolerance),
            Direction::Maximize => (original - opts.tolerance) - opts.new_value,
        };
        let dir_str = direction.as_str();

        Ok(format!(
            "not verified: {commit_short} on tag {ref_tag}\n  metric ({metric}, {dir_str})\n    original:  {original:.6}\n    new:       {nv:.6}\n    delta:     {delta:+.6}\n    tolerance: {tol:.6}  (exceeded by {exceeded:.6})\n  status: {old_status} (unchanged)",
            nv = opts.new_value,
            tol = opts.tolerance,
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use chrono::Local;

    use super::*;
    use crate::model::{Direction, Experiment, RunLog, Status};
    use crate::store::{load_run, runs_dir, save_run};

    fn make_run(tag: &str, experiments: Vec<Experiment>) -> RunLog {
        RunLog {
            experiments,
            run_tag: tag.to_string(),
            created_at: Local::now().to_rfc3339(),
            metric_name: None,
            metric_direction: None,
        }
    }

    fn make_exp(
        commit: &str,
        val: f64,
        status: Status,
        direction: Option<Direction>,
    ) -> Experiment {
        Experiment {
            commit: commit.to_string(),
            val_bpb: val,
            memory_gb: 0.0,
            status,
            description: "test".to_string(),
            timestamp: Local::now().to_rfc3339(),
            params: HashMap::new(),
            parent_commit: None,
            crash_excerpt: None,
            metric_name: None,
            metric_direction: direction,
            signals: Vec::new(),
        }
    }

    fn setup_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(name);
        std::fs::create_dir_all(runs_dir(&dir)).unwrap();
        dir
    }

    #[test]
    fn verify_success_minimize() {
        let dir = setup_dir("resman_verify_success_min");
        let run = make_run("foo", vec![make_exp("abc1234", 0.985, Status::Keep, None)]);
        save_run(&dir, &run).unwrap();

        let result = verify_inner(
            &dir,
            &VerifyOpts {
                commit: "abc1234",
                new_value: 0.982,
                tolerance: 0.01,
                tag: None,
            },
        );
        assert!(result.is_ok(), "{:?}", result);
        let msg = result.unwrap();
        assert!(msg.starts_with("verified"), "expected verified, got: {msg}");

        let saved = load_run(&dir, "foo").unwrap().unwrap();
        let exp = &saved.experiments[0];
        assert_eq!(exp.status, Status::Verified);
        assert!((exp.val_bpb - 0.982).abs() < f64::EPSILON);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_rejects_out_of_tolerance_minimize() {
        let dir = setup_dir("resman_verify_oot_min");
        let run = make_run("bar", vec![make_exp("abc1234", 0.985, Status::Keep, None)]);
        save_run(&dir, &run).unwrap();

        let result = verify_inner(
            &dir,
            &VerifyOpts {
                commit: "abc1234",
                new_value: 1.02,
                tolerance: 0.01,
                tag: None,
            },
        );
        assert!(result.is_ok(), "{:?}", result);
        let msg = result.unwrap();
        assert!(
            msg.starts_with("not verified"),
            "expected not verified, got: {msg}"
        );

        // Status and value must be unchanged.
        let saved = load_run(&dir, "bar").unwrap().unwrap();
        let exp = &saved.experiments[0];
        assert_eq!(exp.status, Status::Keep);
        assert!((exp.val_bpb - 0.985).abs() < f64::EPSILON);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_maximize_direction() {
        let dir = setup_dir("resman_verify_max");
        let run = make_run(
            "baz",
            vec![make_exp(
                "abc1234",
                0.80,
                Status::Keep,
                Some(Direction::Maximize),
            )],
        );
        save_run(&dir, &run).unwrap();

        // 0.79 >= 0.80 - 0.02 (= 0.78) → passes
        let result = verify_inner(
            &dir,
            &VerifyOpts {
                commit: "abc1234",
                new_value: 0.79,
                tolerance: 0.02,
                tag: None,
            },
        );
        assert!(result.is_ok(), "{:?}", result);
        let msg = result.unwrap();
        assert!(msg.starts_with("verified"), "expected verified, got: {msg}");

        let saved = load_run(&dir, "baz").unwrap().unwrap();
        assert_eq!(saved.experiments[0].status, Status::Verified);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_ambiguous_commit_errors() {
        let dir = setup_dir("resman_verify_ambiguous");
        let run = make_run(
            "amb",
            vec![
                make_exp("abc1234", 0.9, Status::Keep, None),
                make_exp("abc1256", 0.8, Status::Keep, None),
            ],
        );
        save_run(&dir, &run).unwrap();

        let result = verify_inner(
            &dir,
            &VerifyOpts {
                commit: "abc12",
                new_value: 0.85,
                tolerance: 0.01,
                tag: None,
            },
        );
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("abc1234") && msg.contains("abc1256"),
            "expected both commits listed: {msg}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_rejects_crash() {
        let dir = setup_dir("resman_verify_crash");
        let run = make_run("cr", vec![make_exp("abc1234", 0.0, Status::Crash, None)]);
        save_run(&dir, &run).unwrap();

        let result = verify_inner(
            &dir,
            &VerifyOpts {
                commit: "abc1234",
                new_value: 0.9,
                tolerance: 0.01,
                tag: None,
            },
        );
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("cannot verify a crash"),
            "expected crash error, got: {msg}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_reverify_allowed() {
        let dir = setup_dir("resman_verify_reverify");
        let run = make_run(
            "rev",
            vec![make_exp("abc1234", 0.985, Status::Verified, None)],
        );
        save_run(&dir, &run).unwrap();

        let result = verify_inner(
            &dir,
            &VerifyOpts {
                commit: "abc1234",
                new_value: 0.980,
                tolerance: 0.01,
                tag: None,
            },
        );
        assert!(result.is_ok(), "{:?}", result);
        let msg = result.unwrap();
        assert!(
            msg.contains("re-verified"),
            "expected re-verified message, got: {msg}"
        );

        let saved = load_run(&dir, "rev").unwrap().unwrap();
        assert_eq!(saved.experiments[0].status, Status::Verified);
        assert!((saved.experiments[0].val_bpb - 0.980).abs() < f64::EPSILON);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
