use std::path::Path;
use std::str::FromStr;

use regex::Regex;

use crate::cli::{OutputFormat, SortField};
use crate::error::Result;
use crate::model::{Experiment, RunLog, Status};
use crate::store::{load_all_runs, require_run, truncate};

pub struct ListOpts<'a> {
    pub status_filter: Option<&'a str>,
    pub sort_by: &'a SortField,
    pub grep_pat: Option<&'a str>,
    pub top: Option<usize>,
    pub reverse: bool,
    pub tag: Option<&'a str>,
    pub format: &'a OutputFormat,
    pub signal_filters: &'a [String],
}

pub fn cmd_list(data_dir: &Path, opts: ListOpts<'_>) -> Result<()> {
    let ListOpts {
        status_filter,
        sort_by,
        grep_pat,
        top,
        reverse,
        tag,
        format,
        signal_filters,
    } = opts;
    let runs = match tag {
        Some(t) => vec![require_run(data_dir, t)?],
        None => load_all_runs(data_dir)?,
    };
    if runs.is_empty() {
        println!("no experiments found. try `resman import <results.tsv>` first.");
        return Ok(());
    }

    let re = grep_pat.map(Regex::new).transpose()?;

    // Build Vec<(Experiment, RunLog)> to preserve run context for metric name resolution.
    let mut tagged: Vec<(Experiment, RunLog)> = runs
        .into_iter()
        .flat_map(|r| {
            let exps: Vec<Experiment> = r.experiments.clone();
            exps.into_iter()
                .map(move |e| (e, r.clone()))
                .collect::<Vec<_>>()
        })
        .collect();

    match status_filter {
        None => tagged.retain(|(e, _)| e.status.is_kept()),
        Some("all") => {}
        Some(s) => {
            let target = Status::from_str(s)?;
            tagged.retain(|(e, _)| e.status == target);
        }
    }
    if let Some(re) = &re {
        tagged.retain(|(e, _)| re.is_match(&e.description));
    }

    // Validate and apply signal filters.
    for want in signal_filters.iter() {
        if !crate::signals::ALL_KINDS.contains(&want.as_str()) {
            return Err(crate::error::Error::InvalidStatus(format!(
                "unknown signal type `{want}`; expected one of: {}",
                crate::signals::ALL_KINDS.join(", ")
            )));
        }
    }
    if !signal_filters.is_empty() {
        tagged.retain(|(e, _run)| {
            signal_filters
                .iter()
                .all(|want| e.signals.iter().any(|s| s.kind() == want))
        });
    }

    match sort_by {
        SortField::ValBpb => tagged.sort_by(|(a, _), (b, _)| {
            a.val_bpb
                .partial_cmp(&b.val_bpb)
                .unwrap_or(std::cmp::Ordering::Equal)
        }),
        SortField::MemoryGb => tagged.sort_by(|(a, _), (b, _)| {
            a.memory_gb
                .partial_cmp(&b.memory_gb)
                .unwrap_or(std::cmp::Ordering::Equal)
        }),
        SortField::Description => {
            tagged.sort_by(|(a, _), (b, _)| a.description.cmp(&b.description))
        }
        SortField::Commit => tagged.sort_by(|(a, _), (b, _)| a.commit.cmp(&b.commit)),
    }
    if reverse {
        tagged.reverse();
    }
    if let Some(n) = top {
        tagged.truncate(n);
    }

    if tagged.is_empty() {
        println!("no experiments matched filters.");
        return Ok(());
    }

    // Determine column label: use common name if all entries agree, else "metric".
    let first_name = tagged[0].0.effective_metric_name(&tagged[0].1);
    let all_same = tagged
        .iter()
        .all(|(e, r)| e.effective_metric_name(r) == first_name);
    let col_label = if all_same { first_name } else { "metric" };

    match format {
        OutputFormat::Json => {
            let exps: Vec<&Experiment> = tagged.iter().map(|(e, _)| e).collect();
            println!("{}", serde_json::to_string_pretty(&exps)?)
        }
        OutputFormat::Tsv => {
            println!("commit\t{col_label}\tmemory_gb\tstatus\tdescription");
            for (e, _) in &tagged {
                println!(
                    "{}\t{:.6}\t{:.1}\t{}\t{}",
                    e.commit, e.val_bpb, e.memory_gb, e.status, e.description
                );
            }
        }
        OutputFormat::Table => {
            println!(
                "{:>4}  {:>10}  {:>7}  {:>8}  {:>8}  description",
                "#", col_label, "mem_gb", "commit", "status"
            );
            println!("{}", "-".repeat(96));
            for (i, (e, _)) in tagged.iter().enumerate() {
                println!(
                    "{:>4}  {:>10.6}  {:>7.1}  {:>8}  {:>8}  {}",
                    i + 1,
                    e.val_bpb,
                    e.memory_gb,
                    e.commit,
                    e.status,
                    truncate(&e.description, 50)
                );
            }
            println!("\n{} experiment(s) shown", tagged.len());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::model::{Experiment, RunLog, Status};
    use crate::signals::Signal;
    use std::collections::HashMap;

    fn make_exp_with_signals(commit: &str, sigs: Vec<Signal>) -> Experiment {
        Experiment {
            commit: commit.to_string(),
            val_bpb: 1.0,
            memory_gb: 0.0,
            status: Status::Keep,
            description: String::new(),
            timestamp: String::new(),
            params: HashMap::new(),
            parent_commit: None,
            crash_excerpt: None,
            metric_name: None,
            metric_direction: None,
            signals: sigs,
        }
    }

    #[test]
    fn list_filters_by_signal() {
        let data_dir = std::env::temp_dir().join("resman_test_list_signal");
        std::fs::create_dir_all(crate::store::runs_dir(&data_dir)).unwrap();

        let run = RunLog {
            run_tag: "sig_test".to_string(),
            created_at: String::new(),
            experiments: vec![
                make_exp_with_signals("oom_commit", vec![Signal::Oom]),
                make_exp_with_signals("nan_commit", vec![Signal::NanLoss]),
            ],
            metric_name: None,
            metric_direction: None,
        };
        crate::store::save_run(&data_dir, &run).unwrap();

        // Filtering to "oom" should only return the first experiment.
        // We test the filtering logic directly by building tagged and applying it.
        let signal_filters: Vec<String> = vec!["oom".to_string()];
        let mut tagged: Vec<(Experiment, RunLog)> = run
            .experiments
            .clone()
            .into_iter()
            .map(|e| (e, run.clone()))
            .collect();

        for want in signal_filters.iter() {
            assert!(
                crate::signals::ALL_KINDS.contains(&want.as_str()),
                "unexpected kind {want}"
            );
        }
        tagged.retain(|(e, _)| {
            signal_filters
                .iter()
                .all(|want| e.signals.iter().any(|s| s.kind() == want))
        });

        assert_eq!(tagged.len(), 1);
        assert_eq!(tagged[0].0.commit, "oom_commit");

        let _ = std::fs::remove_dir_all(&data_dir);
    }
}
