mod cli;
mod commands;
mod model;
mod store;

use clap::Parser;
use cli::{Cli, Commands};
use commands::{compare, export, import, init, list, parse_log, report, stats};
use store::default_data_dir;

fn main() {
    let cli = Cli::parse();
    let data_dir = cli.data_dir.unwrap_or_else(default_data_dir);

    match cli.command {
        Commands::Init { path } => init::cmd_init(&path.unwrap_or(data_dir)),
        Commands::Import { path, tag } => import::cmd_import(&data_dir, &path, tag),
        Commands::ParseLog { pattern } => parse_log::cmd_parse_log(&pattern),
        Commands::List { status, sort_by, grep, top, reverse } =>
            list::cmd_list(&data_dir, status.as_deref(), &sort_by, grep.as_deref(), top, reverse),
        Commands::Compare { run_tags } => compare::cmd_compare(&data_dir, &run_tags),
        Commands::Report { output } => report::cmd_report(&data_dir, &output),
        Commands::Export { output } => export::cmd_export(&data_dir, &output),
        Commands::Stats => stats::cmd_stats(&data_dir),
    }
}
