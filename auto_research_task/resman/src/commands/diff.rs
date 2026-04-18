//! `resman diff <tag_a> <tag_b>` — config/metric diff between two runs.

use std::collections::HashMap;
use std::path::Path;

use serde_json::json;

use crate::cli::OutputFormat;
use crate::error::{Error, Result};
use crate::model::{Experiment, RunLog};
use crate::store::require_run;

// ---------------------------------------------------------------------------
// Core data types
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq)]
pub(crate) enum ParamKind {
    Same,
    Changed,
    Added,
    Removed,
}

pub(crate) struct ParamDiff {
    pub key: String,
    pub kind: ParamKind,
    pub from: Option<String>,
    pub to: Option<String>,
}

// ---------------------------------------------------------------------------
// Pure logic helpers (exposed for tests)
// ---------------------------------------------------------------------------

pub(crate) fn compute_param_diff(
    a: &HashMap<String, String>,
    b: &HashMap<String, String>,
) -> Vec<ParamDiff> {
    let mut keys: Vec<String> = {
        let mut set: std::collections::BTreeSet<String> = Default::default();
        set.extend(a.keys().cloned());
        set.extend(b.keys().cloned());
        set.into_iter().collect()
    };
    keys.sort();

    keys.into_iter()
        .map(|key| {
            let va = a.get(&key).cloned();
            let vb = b.get(&key).cloned();
            let kind = match (&va, &vb) {
                (Some(x), Some(y)) if x == y => ParamKind::Same,
                (Some(_), Some(_)) => ParamKind::Changed,
                (None, Some(_)) => ParamKind::Added,
                (Some(_), None) => ParamKind::Removed,
                (None, None) => unreachable!(),
            };
            ParamDiff {
                key,
                kind,
                from: va,
                to: vb,
            }
        })
        .collect()
}

fn pick_experiment<'a>(run: &'a RunLog, against: &str) -> Option<&'a Experiment> {
    match against {
        "best" => run.best().or_else(|| run.experiments.last()),
        "latest" => run.experiments.last(),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Text-summary helper (shared by cmd_diff + MCP tool)
// ---------------------------------------------------------------------------

pub fn diff_summary_text(
    data_dir: &Path,
    tag_a: &str,
    tag_b: &str,
    against: &str,
) -> Result<String> {
    if against != "best" && against != "latest" {
        return Err(Error::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("unknown --against value: {against} (expected 'best' or 'latest')"),
        )));
    }

    let run_a = require_run(data_dir, tag_a)?;
    let run_b = require_run(data_dir, tag_b)?;

    if run_a.experiments.is_empty() || run_b.experiments.is_empty() {
        return Err(Error::Empty);
    }

    let exp_a = pick_experiment(&run_a, against).ok_or(Error::Empty)?;
    let exp_b = pick_experiment(&run_b, against).ok_or(Error::Empty)?;

    let diffs = compute_param_diff(&exp_a.params, &exp_b.params);
    let delta = exp_b.val_bpb - exp_a.val_bpb;
    let regression = delta > 0.0;

    // Compute column widths
    let key_w = diffs.iter().map(|d| d.key.len()).max().unwrap_or(3).max(3);
    let from_w = diffs
        .iter()
        .map(|d| d.from.as_deref().unwrap_or("-").len())
        .max()
        .unwrap_or(4)
        .max(4);
    let to_w = diffs
        .iter()
        .map(|d| d.to.as_deref().unwrap_or("-").len())
        .max()
        .unwrap_or(2)
        .max(2);

    let mut out = String::new();

    let ts_a = if exp_a.timestamp.is_empty() {
        "no-timestamp".to_string()
    } else {
        exp_a.timestamp.clone()
    };
    let ts_b = if exp_b.timestamp.is_empty() {
        "no-timestamp".to_string()
    } else {
        exp_b.timestamp.clone()
    };

    let verdict = if regression {
        "regression"
    } else {
        "improvement"
    };
    out.push_str(&format!("Comparing {against}-of-tag:\n"));
    out.push_str(&format!(
        "  {tag_a}  @ {}  val_bpb={:.6}  ({})\n",
        exp_a.commit, exp_a.val_bpb, ts_a
    ));
    out.push_str(&format!(
        "  {tag_b}  @ {}  val_bpb={:.6}  ({})  delta={:+.6}  [{}]\n",
        exp_b.commit, exp_b.val_bpb, ts_b, delta, verdict
    ));

    out.push_str("\nParams diff:\n");
    if diffs.is_empty() {
        out.push_str("  (no params on either experiment)\n");
    } else {
        for d in &diffs {
            let from_s = d.from.as_deref().unwrap_or("-");
            let to_s = d.to.as_deref().unwrap_or("-");
            let kind_s = match d.kind {
                ParamKind::Same => "same",
                ParamKind::Changed => "changed",
                ParamKind::Added => "added",
                ParamKind::Removed => "removed",
            };
            out.push_str(&format!(
                "  {:<kw$}  {:<fw$}  ->  {:<tw$}  ({})\n",
                d.key,
                from_s,
                to_s,
                kind_s,
                kw = key_w,
                fw = from_w,
                tw = to_w,
            ));
        }
    }

    out.push_str("\nDescription:\n");
    if exp_a.description == exp_b.description {
        out.push_str(&format!("  same: \"{}\"\n", exp_a.description));
    } else {
        out.push_str(&format!("  {tag_a}: \"{}\"\n", exp_a.description));
        out.push_str(&format!("  {tag_b}: \"{}\"\n", exp_b.description));
    }

    Ok(out)
}

// ---------------------------------------------------------------------------
// Public command entry point
// ---------------------------------------------------------------------------

pub fn cmd_diff(
    data_dir: &Path,
    tag_a: &str,
    tag_b: &str,
    against: &str,
    format: &OutputFormat,
) -> Result<()> {
    if against != "best" && against != "latest" {
        return Err(Error::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("unknown --against value: {against} (expected 'best' or 'latest')"),
        )));
    }

    let run_a = require_run(data_dir, tag_a)?;
    let run_b = require_run(data_dir, tag_b)?;

    if run_a.experiments.is_empty() || run_b.experiments.is_empty() {
        return Err(Error::Empty);
    }

    let exp_a = pick_experiment(&run_a, against).ok_or(Error::Empty)?;
    let exp_b = pick_experiment(&run_b, against).ok_or(Error::Empty)?;

    match format {
        OutputFormat::Table => {
            let text = diff_summary_text(data_dir, tag_a, tag_b, against)?;
            print!("{text}");
        }
        OutputFormat::Json => {
            let diffs = compute_param_diff(&exp_a.params, &exp_b.params);
            let delta = exp_b.val_bpb - exp_a.val_bpb;
            let regression = delta > 0.0;

            let params_json: Vec<_> = diffs
                .iter()
                .map(|d| {
                    json!({
                        "key": d.key,
                        "kind": match d.kind {
                            ParamKind::Same => "same",
                            ParamKind::Changed => "changed",
                            ParamKind::Added => "added",
                            ParamKind::Removed => "removed",
                        },
                        "from": d.from,
                        "to": d.to,
                    })
                })
                .collect();

            let out = json!({
                "against": against,
                "a": {
                    "tag": tag_a,
                    "commit": exp_a.commit,
                    "val_bpb": exp_a.val_bpb,
                    "timestamp": exp_a.timestamp,
                    "description": exp_a.description,
                },
                "b": {
                    "tag": tag_b,
                    "commit": exp_b.commit,
                    "val_bpb": exp_b.val_bpb,
                    "timestamp": exp_b.timestamp,
                    "description": exp_b.description,
                },
                "delta": delta,
                "regression": regression,
                "params": params_json,
            });
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        OutputFormat::Tsv => {
            let diffs = compute_param_diff(&exp_a.params, &exp_b.params);
            println!("key\tkind\tfrom\tto");
            for d in &diffs {
                let kind_s = match d.kind {
                    ParamKind::Same => "same",
                    ParamKind::Changed => "changed",
                    ParamKind::Added => "added",
                    ParamKind::Removed => "removed",
                };
                println!(
                    "{}\t{}\t{}\t{}",
                    d.key,
                    kind_s,
                    d.from.as_deref().unwrap_or("-"),
                    d.to.as_deref().unwrap_or("-"),
                );
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{RunLog, Status};
    use std::collections::HashMap;

    fn make_experiment(
        commit: &str,
        val_bpb: f64,
        status: Status,
        desc: &str,
        params: HashMap<String, String>,
        parent: Option<&str>,
    ) -> crate::model::Experiment {
        crate::model::Experiment {
            commit: commit.to_string(),
            val_bpb,
            memory_gb: 0.0,
            status,
            description: desc.to_string(),
            timestamp: String::new(),
            params,
            parent_commit: parent.map(|s| s.to_string()),
            crash_excerpt: None,
            metric_name: None,
            metric_direction: None,
            signals: Vec::new(),
        }
    }

    fn make_run(tag: &str, exps: Vec<crate::model::Experiment>) -> RunLog {
        RunLog {
            run_tag: tag.to_string(),
            created_at: String::new(),
            experiments: exps,
            metric_name: None,
            metric_direction: None,
        }
    }

    #[test]
    fn classifies_param_diffs() {
        let mut a = HashMap::new();
        a.insert("lr".into(), "0.01".into());
        a.insert("optim".into(), "muon".into());
        a.insert("warmup".into(), "200".into());

        let mut b = HashMap::new();
        b.insert("lr".into(), "0.02".into()); // changed
        b.insert("optim".into(), "muon".into()); // same
        b.insert("n_layer".into(), "14".into()); // added

        let diffs = compute_param_diff(&a, &b);
        let same_count = diffs.iter().filter(|d| d.kind == ParamKind::Same).count();
        let changed_count = diffs
            .iter()
            .filter(|d| d.kind == ParamKind::Changed)
            .count();
        let added_count = diffs.iter().filter(|d| d.kind == ParamKind::Added).count();
        let removed_count = diffs
            .iter()
            .filter(|d| d.kind == ParamKind::Removed)
            .count();

        assert_eq!(same_count, 1);
        assert_eq!(changed_count, 1);
        assert_eq!(added_count, 1);
        assert_eq!(removed_count, 1);
    }

    #[test]
    fn diff_empty_run_errors() {
        use std::env;
        let data_dir = env::temp_dir().join("resman_test_diff_empty");
        let _ = std::fs::remove_dir_all(&data_dir);

        // Write two runs to disk — one empty
        let run_a = make_run("tagA_empty", vec![]);
        let run_b = make_run(
            "tagB_nonempty",
            vec![make_experiment(
                "abc",
                0.99,
                Status::Keep,
                "b",
                HashMap::new(),
                None,
            )],
        );
        crate::store::ensure_initialized(&data_dir).unwrap();
        crate::store::save_run(&data_dir, &run_a).unwrap();
        crate::store::save_run(&data_dir, &run_b).unwrap();

        let result = diff_summary_text(&data_dir, "tagA_empty", "tagB_nonempty", "best");
        assert!(matches!(result, Err(Error::Empty)));

        let _ = std::fs::remove_dir_all(&data_dir);
    }

    #[test]
    fn diff_same_params_produces_only_same() {
        let mut params = HashMap::new();
        params.insert("lr".into(), "0.01".into());
        params.insert("optim".into(), "muon".into());

        let diffs = compute_param_diff(&params, &params.clone());
        assert!(
            diffs.iter().all(|d| d.kind == ParamKind::Same),
            "expected all Same"
        );
    }
}
