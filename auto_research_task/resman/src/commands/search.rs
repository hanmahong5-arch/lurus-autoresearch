use std::path::Path;

use regex::{Regex, RegexBuilder};

use crate::cli::OutputFormat;
use crate::error::Result;
use crate::model::{Experiment, Status};
use crate::store::{load_all_runs, truncate};

/// Whether an experiment matches a search: a case-insensitive regex over its
/// description, commit, and `k=v` params. Discards are skipped unless
/// `include_discarded`. Shared by `cmd_search` and the MCP `tool_search` so the
/// two match predicates cannot drift.
pub(crate) fn matches_search(e: &Experiment, re: &Regex, include_discarded: bool) -> bool {
    if !include_discarded && matches!(e.status, Status::Discard) {
        return false;
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
    re.is_match(&haystack)
}

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
            if matches_search(e, &re, include_discarded) {
                hits.push((&run.run_tag, e));
            }
        }
    }

    if hits.is_empty() {
        // Format-aware empty result: machine formats must stay parseable on the
        // (common) no-match path — an empty JSON array / a bare TSV header —
        // never human prose. Only the table view gets the friendly hint.
        match format {
            OutputFormat::Json => println!("[]"),
            OutputFormat::Tsv => println!("run\tcommit\tval_bpb\tstatus\tdescription"),
            OutputFormat::Table => {
                println!("no matches for `{pattern}` across {} run(s).", runs.len());
                println!("→ idea is unexplored; safe to try.");
            }
        }
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
                    e.commit,
                    e.val_bpb,
                    e.status,
                    crate::store::tsv_field(&e.description)
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
