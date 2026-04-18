use std::path::Path;

use regex::RegexBuilder;

use crate::cli::OutputFormat;
use crate::error::Result;
use crate::model::Experiment;
use crate::store::{load_all_runs, truncate};

/// Answer the question "has the agent already tried this?" — the single most
/// requested feature in the upstream community (issue #47, #418, PR #80).
///
/// Scans every experiment's `description` (and optionally `params` / `commit`)
/// for a regex. Case-insensitive by default.
pub fn cmd_search(
    data_dir: &Path,
    pattern: &str,
    format: &OutputFormat,
    include_discarded: bool,
) -> Result<()> {
    let re = RegexBuilder::new(pattern).case_insensitive(true).build()?;
    let runs = load_all_runs(data_dir)?;
    let mut hits: Vec<(&str, &Experiment)> = Vec::new();

    for run in &runs {
        for e in &run.experiments {
            if !include_discarded && matches!(e.status, crate::model::Status::Discard) {
                continue;
            }
            let haystack = format!(
                "{} {} {}",
                e.description,
                e.commit,
                e.params
                    .iter()
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            );
            if re.is_match(&haystack) {
                hits.push((&run.run_tag, e));
            }
        }
    }

    if hits.is_empty() {
        println!("no matches for `{pattern}` across {} run(s).", runs.len());
        println!("→ idea is unexplored; safe to try.");
        return Ok(());
    }

    match format {
        OutputFormat::Json => {
            let out: Vec<_> = hits
                .iter()
                .map(|(tag, e)| {
                    serde_json::json!({
                        "run": tag, "commit": e.commit, "val_bpb": e.val_bpb,
                        "status": e.status, "description": e.description,
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        OutputFormat::Tsv => {
            println!("run\tcommit\tval_bpb\tstatus\tdescription");
            for (tag, e) in &hits {
                println!(
                    "{tag}\t{}\t{:.6}\t{}\t{}",
                    e.commit, e.val_bpb, e.status, e.description
                );
            }
        }
        OutputFormat::Table => {
            println!(
                "{:<14}  {:>10}  {:>8}  {:>8}  description",
                "run", "val_bpb", "commit", "status"
            );
            println!("{}", "-".repeat(88));
            for (tag, e) in &hits {
                println!(
                    "{:<14}  {:>10.6}  {:>8}  {:>8}  {}",
                    truncate(tag, 14),
                    e.val_bpb,
                    e.commit,
                    e.status,
                    truncate(&e.description, 42)
                );
            }
            println!(
                "\n{} match(es). → idea already explored; consider a variation.",
                hits.len()
            );
        }
    }
    Ok(())
}
