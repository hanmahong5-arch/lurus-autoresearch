mod cli;
mod commands;
mod error;
mod html;
mod hw;
mod logtail;
mod model;
mod signals;
mod store;
mod term;

use std::process::ExitCode;

use clap::Parser;

use cli::{Cli, Commands};
use store::default_data_dir;

fn main() -> ExitCode {
    let cli = Cli::parse();
    term::init(cli.no_color);
    let data_dir = cli.data_dir.unwrap_or_else(default_data_dir);

    let result = match cli.command {
        Commands::Init { path } => commands::init::cmd_init(path.as_deref().unwrap_or(&data_dir)),
        Commands::Import {
            path,
            tag,
            force,
            metric_name,
            metric_direction,
        } => commands::import::cmd_import(
            &data_dir,
            &path,
            tag,
            force,
            metric_name,
            metric_direction,
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
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
