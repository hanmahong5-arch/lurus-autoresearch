mod cli;
mod commands;
mod csv;
mod error;
mod html;
mod hw;
mod logtail;
mod model;
mod signals;
mod store;
mod term;
mod usage;

use std::process::ExitCode;

use clap::{CommandFactory, Parser};
use serde_json::json;

use cli::{Cli, Commands};
use store::default_data_dir;

/// Return `(canonical_tool_name, args_json)` ONLY for loop-advancing CLI
/// commands that should be mirrored in `usage.jsonl`. Returns `None` for all
/// read-only / shell-API / infrastructure commands (Best, List, Search, …).
fn usage_descriptor(cmd: &Commands) -> Option<(&'static str, serde_json::Value)> {
    match cmd {
        Commands::Add { tag, status, .. } => Some((
            "resman_add_experiment",
            json!({"tag": tag, "status": status}),
        )),
        Commands::Verify { tag, commit, .. } => {
            Some(("resman_verify", json!({"tag": tag, "commit": commit})))
        }
        Commands::Unverify { commit, tag } => {
            Some(("resman_unverify", json!({"tag": tag, "commit": commit})))
        }
        Commands::Import { tag, .. } => Some(("resman_import", json!({"tag": tag}))),
        Commands::Distill { tag, all, .. } => {
            if *all {
                Some(("resman_distill", json!({"all": true})))
            } else {
                Some(("resman_distill", json!({"tag": tag})))
            }
        }
        _ => None,
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    term::init(cli.no_color);
    let data_dir = cli.data_dir.unwrap_or_else(default_data_dir);

    let descriptor = usage_descriptor(&cli.command);
    let timer = usage::CallTimer::start();

    let result = match cli.command {
        Commands::Init { path } => commands::init::cmd_init(path.as_deref().unwrap_or(&data_dir)),
        Commands::Import {
            path,
            tag,
            force,
            metric_name,
            metric_direction,
            from,
            metric,
        } => commands::import::cmd_import(
            &data_dir,
            &path,
            tag,
            force,
            metric_name,
            metric_direction,
            from,
            metric,
        ),
        Commands::Add {
            tag,
            commit,
            val_bpb,
            memory_gb,
            status,
            description,
            params,
            parent,
            log,
            no_gpu_probe,
            metric_name,
            metric_direction,
        } => commands::add::cmd_add_from_flags(
            &data_dir,
            &tag,
            &commit,
            val_bpb,
            memory_gb,
            &status,
            &description,
            &params,
            parent.as_deref(),
            log.as_ref(),
            no_gpu_probe,
            metric_name.as_deref(),
            metric_direction.as_deref(),
        ),
        Commands::Search {
            pattern,
            include_discarded,
            format,
        } => commands::search::cmd_search(&data_dir, &pattern, &format, include_discarded),
        Commands::Near { val_bpb, n, format } => {
            commands::near::cmd_near(&data_dir, val_bpb, n, &format)
        }
        Commands::ParseLog { pattern } => commands::parse_log::cmd_parse_log(&pattern),
        Commands::List {
            status,
            sort_by,
            grep,
            top,
            reverse,
            tag,
            format,
            signal,
        } => commands::list::cmd_list(
            &data_dir,
            commands::list::ListOpts {
                status_filter: status.as_deref(),
                sort_by: &sort_by,
                grep_pat: grep.as_deref(),
                top,
                reverse,
                tag: tag.as_deref(),
                format: &format,
                signal_filters: &signal,
            },
        ),
        Commands::Best {
            tag,
            format,
            composite,
        } => commands::best::cmd_best(&data_dir, tag.as_deref(), &format, composite),
        Commands::Compare { run_tags, format } => {
            commands::compare::cmd_compare(&data_dir, &run_tags, &format)
        }
        Commands::Report { output, title } => {
            commands::report::cmd_report(&data_dir, &output, title.as_deref())
        }
        Commands::Export { output } => commands::export::cmd_export(&data_dir, &output),
        Commands::Watch {
            path,
            tag,
            interval,
        } => commands::watch::cmd_watch(&data_dir, &path, tag, interval),
        Commands::Stats { tag } => commands::stats::cmd_stats(&data_dir, tag.as_deref()),
        Commands::Mcp => commands::mcp::cmd_mcp(data_dir.clone()),
        Commands::Diff {
            tag_a,
            tag_b,
            against,
            format,
        } => commands::diff::cmd_diff(&data_dir, &tag_a, &tag_b, &against, &format),
        Commands::Tree {
            tag,
            highlight_best,
            format,
        } => commands::tree::cmd_tree(&data_dir, &tag, highlight_best, &format),
        Commands::Distill {
            tag,
            out,
            format,
            html,
            all,
        } => {
            if all {
                commands::distill::cmd_cross_distill(&data_dir, out.as_deref(), &format)
            } else {
                let t = tag.expect("tag is required when --all is not set");
                commands::distill::cmd_distill(
                    &data_dir,
                    &t,
                    out.as_deref(),
                    &format,
                    html.as_deref(),
                )
            }
        }
        Commands::Usage {
            by_tool,
            errors,
            sequences,
            summary,
            tool,
            since,
            top,
            format,
        } => commands::usage::cmd_usage(
            &data_dir,
            commands::usage::UsageOpts {
                by_tool,
                errors,
                sequences,
                summary,
                tool,
                since,
                top,
                format,
            },
        ),
        Commands::Verify {
            commit,
            value,
            tolerance,
            tag,
        } => commands::verify::cmd_verify(
            &data_dir,
            commands::verify::VerifyOpts {
                commit: &commit,
                new_value: value,
                tolerance,
                tag: tag.as_deref(),
            },
        ),
        Commands::Doctor { format } => commands::doctor::cmd_doctor(&data_dir, &format),
        Commands::Tags { format } => commands::tags::cmd_tags(&data_dir, &format),
        Commands::Unverify { commit, tag } => commands::verify::cmd_unverify(
            &data_dir,
            commands::verify::UnverifyOpts {
                commit: &commit,
                tag: tag.as_deref(),
            },
        ),
        Commands::Completions { shell } => {
            let mut cmd = Cli::command();
            clap_complete::generate(shell, &mut cmd, "resman", &mut std::io::stdout());
            Ok(())
        }
    };

    if let Some((tool, args)) = &descriptor {
        usage::log_call(&data_dir, tool, args, result.is_ok(), timer.elapsed_ms(), 0);
    }

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cli::{Commands, OutputFormat};

    fn make_add(tag: &str, status: &str) -> Commands {
        Commands::Add {
            tag: tag.to_string(),
            commit: "abc1234".to_string(),
            val_bpb: 1.0,
            memory_gb: 0.0,
            status: status.to_string(),
            description: "test".to_string(),
            params: vec![],
            parent: None,
            log: None,
            no_gpu_probe: false,
            metric_name: None,
            metric_direction: None,
        }
    }

    #[test]
    fn usage_descriptor_add() {
        let cmd = make_add("mytag", "keep");
        let (name, args) = usage_descriptor(&cmd).expect("Add must produce a descriptor");
        assert_eq!(name, "resman_add_experiment");
        assert_eq!(args["tag"], "mytag");
        assert_eq!(args["status"], "keep");
    }

    #[test]
    fn usage_descriptor_verify() {
        let cmd = Commands::Verify {
            commit: "abc1234".to_string(),
            value: 0.9,
            tolerance: 0.01,
            tag: Some("t1".to_string()),
        };
        let (name, args) = usage_descriptor(&cmd).expect("Verify must produce a descriptor");
        assert_eq!(name, "resman_verify");
        assert_eq!(args["commit"], "abc1234");
        assert_eq!(args["tag"], "t1");
    }

    #[test]
    fn usage_descriptor_verify_no_tag() {
        let cmd = Commands::Verify {
            commit: "abc1234".to_string(),
            value: 0.9,
            tolerance: 0.01,
            tag: None,
        };
        let (name, args) = usage_descriptor(&cmd).expect("Verify must produce a descriptor");
        assert_eq!(name, "resman_verify");
        assert!(args["tag"].is_null());
    }

    #[test]
    fn usage_descriptor_unverify() {
        let cmd = Commands::Unverify {
            commit: "def5678".to_string(),
            tag: Some("t2".to_string()),
        };
        let (name, args) = usage_descriptor(&cmd).expect("Unverify must produce a descriptor");
        assert_eq!(name, "resman_unverify");
        assert_eq!(args["commit"], "def5678");
        assert_eq!(args["tag"], "t2");
    }

    #[test]
    fn usage_descriptor_import() {
        let cmd = Commands::Import {
            path: std::path::PathBuf::from("x.tsv"),
            tag: Some("run1".to_string()),
            force: false,
            metric_name: None,
            metric_direction: None,
            from: cli::ImportSource::Tsv,
            metric: None,
        };
        let (name, args) = usage_descriptor(&cmd).expect("Import must produce a descriptor");
        assert_eq!(name, "resman_import");
        assert_eq!(args["tag"], "run1");
    }

    #[test]
    fn usage_descriptor_distill_tag() {
        let cmd = Commands::Distill {
            tag: Some("run1".to_string()),
            out: None,
            format: crate::commands::distill::DistillFormat::Markdown,
            html: None,
            all: false,
        };
        let (name, args) = usage_descriptor(&cmd).expect("Distill(tag) must produce a descriptor");
        assert_eq!(name, "resman_distill");
        assert_eq!(args["tag"], "run1");
        assert!(args.get("all").is_none() || args["all"].is_null());
    }

    #[test]
    fn usage_descriptor_distill_all() {
        let cmd = Commands::Distill {
            tag: None,
            out: None,
            format: crate::commands::distill::DistillFormat::Markdown,
            html: None,
            all: true,
        };
        let (name, args) = usage_descriptor(&cmd).expect("Distill(all) must produce a descriptor");
        assert_eq!(name, "resman_distill");
        assert_eq!(args["all"], true);
    }

    #[test]
    fn usage_descriptor_returns_none_for_best() {
        let cmd = Commands::Best {
            tag: None,
            format: "value".to_string(),
            composite: false,
        };
        assert!(
            usage_descriptor(&cmd).is_none(),
            "Best must NOT produce a usage descriptor"
        );
    }

    #[test]
    fn usage_descriptor_returns_none_for_mcp() {
        assert!(usage_descriptor(&Commands::Mcp).is_none());
    }

    #[test]
    fn usage_descriptor_returns_none_for_usage() {
        let cmd = Commands::Usage {
            by_tool: false,
            errors: false,
            sequences: false,
            summary: false,
            tool: None,
            since: None,
            top: 20,
            format: OutputFormat::Table,
        };
        assert!(usage_descriptor(&cmd).is_none());
    }

    #[test]
    fn usage_descriptor_returns_none_for_list() {
        let cmd = Commands::List {
            status: None,
            sort_by: cli::SortField::ValBpb,
            grep: None,
            top: None,
            reverse: false,
            tag: None,
            format: OutputFormat::Table,
            signal: vec![],
        };
        assert!(usage_descriptor(&cmd).is_none());
    }
}
