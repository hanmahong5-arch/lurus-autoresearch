//! `resman verify` — re-verify an experiment by providing a re-run metric value.
//!
//! Does NOT run training. The caller provides the new value. Resman compares
//! it against the recorded original within the given tolerance and, if it
//! passes, promotes the experiment's status to `Verified` and updates val_bpb.

use std::path::Path;

use crate::error::{Error, Result};
use crate::model::{Direction, Status};
use crate::store::{load_all_runs, load_run, load_run_or_suggest, save_run};

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

/// Core logic returning structured JSON string — used by the MCP tool.
pub fn verify_inner_json(data_dir: &Path, opts: &VerifyOpts<'_>) -> Result<String> {
    use serde_json::json;

    if opts.tolerance < 0.0 {
        return Err(Error::Custom("tolerance must be non-negative".to_string()));
    }

    let runs = match opts.tag {
        Some(t) => vec![load_run_or_suggest(data_dir, t)?],
        None => load_all_runs(data_dir)?,
    };

    let mut matches: Vec<(String, usize)> = Vec::new();
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
                let run = runs.iter().find(|r| r.run_tag == *tag).ok_or_else(|| {
                    Error::Custom(format!("tag `{tag}` not found in loaded runs"))
                })?;
                let exp = &run.experiments[*idx];
                Ok(format!("  [{tag}] {}", exp.commit))
            })
            .collect::<Result<Vec<String>>>()?;
        return Err(Error::Custom(format!(
            "ambiguous commit `{}` — matches:\n{}",
            opts.commit,
            candidates.join("\n")
        )));
    }

    let (ref_tag, ref_idx) = matches
        .into_iter()
        .next()
        .ok_or_else(|| Error::Custom("internal: matches became empty".to_string()))?;

    let mut run = load_run(data_dir, &ref_tag)?
        .ok_or_else(|| Error::Custom(format!("tag `{ref_tag}` disappeared")))?;

    let exp = &run.experiments[ref_idx];

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

        let action = if re_verify { "re-verified" } else { "verified" };
        let result = json!({
            "verified": true,
            "action": action,
            "tag": ref_tag,
            "commit": commit_short,
            "metric": metric,
            "direction": direction.as_str(),
            "original": original,
            "new": opts.new_value,
            "delta": delta,
            "tolerance": opts.tolerance,
            "previous_status": old_status.to_string(),
            "new_status": "verified"
        });
        serde_json::to_string(&result).map_err(|e| Error::Custom(e.to_string()))
    } else {
        let exceeded = match direction {
            Direction::Minimize => opts.new_value - (original + opts.tolerance),
            Direction::Maximize => (original - opts.tolerance) - opts.new_value,
        };
        let result = json!({
            "verified": false,
            "tag": ref_tag,
            "commit": commit_short,
            "metric": metric,
            "direction": direction.as_str(),
            "original": original,
            "new": opts.new_value,
            "delta": delta,
            "tolerance": opts.tolerance,
            "exceeded_by": exceeded,
            "current_status": old_status.to_string()
        });
        serde_json::to_string(&result).map_err(|e| Error::Custom(e.to_string()))
    }
}

/// Core logic, also called from the MCP tool.
pub fn verify_inner(data_dir: &Path, opts: &VerifyOpts<'_>) -> Result<String> {
    if opts.tolerance < 0.0 {
        return Err(Error::Custom("tolerance must be non-negative".to_string()));
    }

    // Collect candidate (run_tag, experiment_index) pairs whose commit starts with `opts.commit`.
    let runs = match opts.tag {
        Some(t) => vec![load_run_or_suggest(data_dir, t)?],
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
                let run = runs.iter().find(|r| r.run_tag == *tag).ok_or_else(|| {
                    Error::Custom(format!("tag `{tag}` not found in loaded runs"))
                })?;
                let exp = &run.experiments[*idx];
                Ok(format!("  [{tag}] {}", exp.commit))
            })
            .collect::<Result<Vec<String>>>()?;
        return Err(Error::Custom(format!(
            "ambiguous commit `{}` — matches:\n{}",
            opts.commit,
            candidates.join("\n")
        )));
    }

    let (ref_tag, ref_idx) = matches
        .into_iter()
        .next()
        .ok_or_else(|| Error::Custom("internal: matches became empty".to_string()))?;

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
        let verified_glyph = crate::term::status_glyph(&crate::model::Status::Verified);
        let status_transition = if re_verify {
            format!("verified → verified {verified_glyph}")
        } else {
            format!("{old_status} → verified {verified_glyph}")
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

// ===========================================================================
// `resman unverify` — symmetric retraction.
//
// Reverts a `Verified` experiment back to `Keep`. Use when a verified result
// turns out to be a fluke (e.g., a later re-run shows divergence). The
// experiment's val_bpb stays at whatever the verify pass last wrote — the
// retraction is purely about trust, not metric value. To erase the metric
// value as well, simply re-run and call `resman add` with a new commit.
// ===========================================================================

pub struct UnverifyOpts<'a> {
    pub commit: &'a str,
    pub tag: Option<&'a str>,
}

pub fn cmd_unverify(data_dir: &Path, opts: UnverifyOpts<'_>) -> Result<()> {
    let text = unverify_inner(data_dir, &opts)?;
    println!("{text}");
    Ok(())
}

fn locate_unverify_target(
    data_dir: &Path,
    opts: &UnverifyOpts<'_>,
) -> Result<(String, usize, crate::model::RunLog)> {
    let runs = match opts.tag {
        Some(t) => vec![load_run_or_suggest(data_dir, t)?],
        None => load_all_runs(data_dir)?,
    };

    let mut matches: Vec<(String, usize)> = Vec::new();
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
                let run = runs.iter().find(|r| r.run_tag == *tag).ok_or_else(|| {
                    Error::Custom(format!("tag `{tag}` not found in loaded runs"))
                })?;
                let exp = &run.experiments[*idx];
                Ok(format!("  [{tag}] {}", exp.commit))
            })
            .collect::<Result<Vec<String>>>()?;
        return Err(Error::Custom(format!(
            "ambiguous commit `{}` — matches:\n{}",
            opts.commit,
            candidates.join("\n")
        )));
    }

    let (ref_tag, ref_idx) = matches
        .into_iter()
        .next()
        .ok_or_else(|| Error::Custom("internal: matches became empty".to_string()))?;
    let run = load_run(data_dir, &ref_tag)?
        .ok_or_else(|| Error::Custom(format!("tag `{ref_tag}` disappeared")))?;
    Ok((ref_tag, ref_idx, run))
}

/// Returns human-readable text describing the unverify outcome.
pub fn unverify_inner(data_dir: &Path, opts: &UnverifyOpts<'_>) -> Result<String> {
    let (ref_tag, ref_idx, mut run) = locate_unverify_target(data_dir, opts)?;
    let exp = &run.experiments[ref_idx];

    if exp.status != Status::Verified {
        return Err(Error::Custom(format!(
            "cannot unverify {}: current status is `{}`, not `verified`",
            exp.commit, exp.status
        )));
    }

    let commit_short = exp.commit.clone();
    let prev_status = exp.status;
    let retained_value = exp.val_bpb;
    let metric = exp.effective_metric_name(&run).to_string();

    run.experiments[ref_idx].status = Status::Keep;
    save_run(data_dir, &run)?;

    Ok(format!(
        "unverified {commit_short} on tag {ref_tag}\n  metric ({metric})\n    retained value: {retained_value:.6}\n  status: {prev_status} → keep"
    ))
}

/// Returns structured JSON describing the unverify outcome (MCP path).
pub fn unverify_inner_json(data_dir: &Path, opts: &UnverifyOpts<'_>) -> Result<String> {
    use serde_json::json;

    let (ref_tag, ref_idx, mut run) = locate_unverify_target(data_dir, opts)?;
    let exp = &run.experiments[ref_idx];

    if exp.status != Status::Verified {
        return Err(Error::Custom(format!(
            "cannot unverify {}: current status is `{}`, not `verified`",
            exp.commit, exp.status
        )));
    }

    let commit_short = exp.commit.clone();
    let prev_status = exp.status;
    let retained_value = exp.val_bpb;
    let metric = exp.effective_metric_name(&run).to_string();

    run.experiments[ref_idx].status = Status::Keep;
    save_run(data_dir, &run)?;

    let result = json!({
        "unverified": true,
        "tag": ref_tag,
        "commit": commit_short,
        "metric": metric,
        "retained_value": retained_value,
        "previous_status": prev_status.to_string(),
        "new_status": "keep"
    });
    serde_json::to_string(&result).map_err(|e| Error::Custom(e.to_string()))
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
            schema_version: 1,
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

    // ---------------------------------------------------------------
    // unverify
    // ---------------------------------------------------------------

    #[test]
    fn unverify_reverts_verified_to_keep() {
        let dir = setup_dir("resman_unverify_success");
        let run = make_run(
            "rv",
            vec![make_exp("abc1234", 0.980, Status::Verified, None)],
        );
        save_run(&dir, &run).unwrap();

        let result = unverify_inner(
            &dir,
            &UnverifyOpts {
                commit: "abc1234",
                tag: None,
            },
        );
        assert!(result.is_ok(), "{:?}", result);
        let msg = result.unwrap();
        assert!(msg.starts_with("unverified"));
        assert!(msg.contains("verified → keep"));

        let saved = load_run(&dir, "rv").unwrap().unwrap();
        assert_eq!(saved.experiments[0].status, Status::Keep);
        // val_bpb retained — only trust label changed.
        assert!((saved.experiments[0].val_bpb - 0.980).abs() < f64::EPSILON);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unverify_rejects_non_verified_status() {
        let dir = setup_dir("resman_unverify_rejects_keep");
        let run = make_run("rk", vec![make_exp("abc1234", 0.985, Status::Keep, None)]);
        save_run(&dir, &run).unwrap();

        let result = unverify_inner(
            &dir,
            &UnverifyOpts {
                commit: "abc1234",
                tag: None,
            },
        );
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("not `verified`"), "got: {msg}");

        // Status unchanged.
        let saved = load_run(&dir, "rk").unwrap().unwrap();
        assert_eq!(saved.experiments[0].status, Status::Keep);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unverify_json_returns_structured_payload() {
        let dir = setup_dir("resman_unverify_json");
        let run = make_run(
            "rj",
            vec![make_exp("abc1234", 0.97, Status::Verified, None)],
        );
        save_run(&dir, &run).unwrap();

        let json_str = unverify_inner_json(
            &dir,
            &UnverifyOpts {
                commit: "abc1234",
                tag: None,
            },
        )
        .unwrap();
        let v: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(v["unverified"], true);
        assert_eq!(v["new_status"], "keep");
        assert_eq!(v["previous_status"], "verified");
        assert_eq!(v["tag"], "rj");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unverify_ambiguous_commit_errors() {
        let dir = setup_dir("resman_unverify_ambiguous");
        let run = make_run(
            "ra",
            vec![
                make_exp("abc1234", 0.97, Status::Verified, None),
                make_exp("abc1256", 0.96, Status::Verified, None),
            ],
        );
        save_run(&dir, &run).unwrap();

        let result = unverify_inner(
            &dir,
            &UnverifyOpts {
                commit: "abc12",
                tag: None,
            },
        );
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("ambiguous"), "expected ambiguous error: {msg}");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
