use std::path::Path;
use std::str::FromStr;

use regex::Regex;

use crate::cli::{OutputFormat, SortField};
use crate::error::Result;
use crate::model::{Experiment, RunLog, Status};
use crate::store::{load_all_runs, load_run_or_suggest, truncate};

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

/// Filter, sort, and truncate a flat list of `(Experiment, RunLog)` pairs.
///
/// Returns `Err` when a signal filter name or status string is invalid.
pub(crate) fn filter_sort_truncate(
    mut tagged: Vec<(Experiment, RunLog)>,
    status_filter: Option<&str>,
    sort_by: &SortField,
    grep_pat: Option<&str>,
    top: Option<usize>,
    reverse: bool,
    signal_filters: &[String],
) -> Result<Vec<(Experiment, RunLog)>> {
    let re = grep_pat.map(Regex::new).transpose()?;

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

    Ok(tagged)
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
        Some(t) => vec![load_run_or_suggest(data_dir, t)?],
        None => load_all_runs(data_dir)?,
    };
    if runs.is_empty() {
        println!(
            "{}",
            crate::term::empty_state(
                "no experiments found. try `resman import <results.tsv>` first."
            )
        );
        return Ok(());
    }

    // Build Vec<(Experiment, RunLog)> to preserve run context for metric name resolution.
    let tagged: Vec<(Experiment, RunLog)> = runs
        .into_iter()
        .flat_map(|r| {
            let exps: Vec<Experiment> = r.experiments.clone();
            exps.into_iter()
                .map(move |e| (e, r.clone()))
                .collect::<Vec<_>>()
        })
        .collect();

    let tagged = filter_sort_truncate(
        tagged,
        status_filter,
        sort_by,
        grep_pat,
        top,
        reverse,
        signal_filters,
    )?;

    if tagged.is_empty() {
        println!(
            "{}",
            crate::term::empty_state("no experiments matched filters.")
        );
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
                    e.commit,
                    e.val_bpb,
                    e.memory_gb,
                    e.status,
                    crate::store::tsv_field(&e.description)
                );
            }
        }
        OutputFormat::Table => {
            let n = tagged.len();
            println!(
                "{}",
                crate::term::section_header("list", Some(&format!("{n} experiment(s)")))
            );
            println!(
                "{:>4}  {:>10}  {:>7}  {:>8}  {:<10}  description",
                "#", col_label, "mem_gb", "commit", "status"
            );
            println!("{}", crate::term::rule());
            for (i, (e, _)) in tagged.iter().enumerate() {
                println!(
                    "{:>4}  {:>10.6}  {:>7.1}  {:>8}  {}  {}",
                    i + 1,
                    e.val_bpb,
                    e.memory_gb,
                    e.commit,
                    crate::term::status_cell(&e.status),
                    truncate(&e.description, crate::term::DESC_TRUNC)
                );
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::cli::SortField;
    use crate::model::{Experiment, RunLog, Status};
    use crate::signals::Signal;
    use std::collections::HashMap;

    fn make_exp(commit: &str, status: Status, val_bpb: f64, sigs: Vec<Signal>) -> Experiment {
        Experiment {
            commit: commit.to_string(),
            val_bpb,
            memory_gb: 0.0,
            status,
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

    fn make_run(tag: &str, exps: Vec<Experiment>) -> RunLog {
        RunLog {
            run_tag: tag.to_string(),
            created_at: String::new(),
            experiments: exps,
            metric_name: None,
            metric_direction: None,
            schema_version: 1,
        }
    }

    #[test]
    fn list_filters_by_signal() {
        let data_dir = std::env::temp_dir().join("resman_test_list_signal");
        std::fs::create_dir_all(crate::store::runs_dir(&data_dir)).unwrap();

        let run = make_run(
            "sig_test",
            vec![
                make_exp("oom_commit", Status::Keep, 1.0, vec![Signal::Oom]),
                make_exp("nan_commit", Status::Keep, 1.0, vec![Signal::NanLoss]),
            ],
        );
        crate::store::save_run(&data_dir, &run).unwrap();

        // Filtering to "oom" should only return the first experiment.
        let tagged: Vec<_> = run
            .experiments
            .clone()
            .into_iter()
            .map(|e| (e, run.clone()))
            .collect();

        let signal_filters = vec!["oom".to_string()];
        let result = super::filter_sort_truncate(
            tagged,
            Some("all"),
            &SortField::ValBpb,
            None,
            None,
            false,
            &signal_filters,
        )
        .unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0.commit, "oom_commit");

        let _ = std::fs::remove_dir_all(&data_dir);
    }

    #[test]
    fn filter_sort_truncate_status_and_top() {
        let run = make_run(
            "t",
            vec![
                make_exp("a", Status::Keep, 2.0, vec![]),
                make_exp("b", Status::Discard, 1.0, vec![]),
                make_exp("c", Status::Keep, 3.0, vec![]),
                make_exp("d", Status::Keep, 1.5, vec![]),
            ],
        );
        let tagged: Vec<_> = run
            .experiments
            .clone()
            .into_iter()
            .map(|e| (e, run.clone()))
            .collect();

        // status_filter=None retains only kept; top=2; sorted by val_bpb ascending.
        let result = super::filter_sort_truncate(
            tagged,
            None,
            &SortField::ValBpb,
            None,
            Some(2),
            false,
            &[],
        )
        .unwrap();

        assert_eq!(result.len(), 2, "top=2 should yield 2 results");
        // After filtering (keep only kept: a=2.0, c=3.0, d=1.5), sorted ascending: d(1.5), a(2.0), c(3.0) → top 2: d, a
        assert_eq!(result[0].0.commit, "d");
        assert_eq!(result[0].0.val_bpb, 1.5);
        assert_eq!(result[1].0.commit, "a");
        assert_eq!(result[1].0.val_bpb, 2.0);
    }

    #[test]
    fn filter_sort_truncate_signal_and_status() {
        let run = make_run(
            "t",
            vec![
                make_exp("x", Status::Keep, 1.0, vec![Signal::Oom]),
                make_exp("y", Status::Keep, 1.0, vec![Signal::NanLoss]),
                make_exp("z", Status::Discard, 1.0, vec![Signal::Oom]),
            ],
        );
        let tagged: Vec<_> = run
            .experiments
            .clone()
            .into_iter()
            .map(|e| (e, run.clone()))
            .collect();

        // status_filter=None (kept only) + signal oom → only "x"
        let result = super::filter_sort_truncate(
            tagged,
            None,
            &SortField::ValBpb,
            None,
            None,
            false,
            &["oom".to_string()],
        )
        .unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0.commit, "x");
    }
}
