use std::path::PathBuf;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "resman", about = "Research experiment manager for autoresearch workflows", author = "autoresearch", version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Directory to store resman data (default: ~/.resman)
    #[arg(short, long, global = true)]
    pub data_dir: Option<PathBuf>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Import a results.tsv file from an autoresearch run
    Import {
        path: PathBuf,
        /// Name/tag for this run
        #[arg(short, long)]
        tag: Option<String>,
    },
    /// Parse training logs to extract val_bpb and stats
    ParseLog {
        /// Path to run.log or glob pattern
        pattern: String,
    },
    /// List all experiments across runs
    List {
        /// Filter by status (keep/discard/crash)
        #[arg(short, long)]
        status: Option<String>,
        /// Sort by this field (val_bpb/commit/memory_gb/description)
        #[arg(short, long, default_value = "val_bpb")]
        sort_by: String,
        /// Filter description with regex
        #[arg(short, long)]
        grep: Option<String>,
        /// Show only top N
        #[arg(short, long)]
        top: Option<usize>,
        /// Reverse sort order
        #[arg(long)]
        reverse: bool,
    },
    /// Compare best experiments across multiple runs
    Compare {
        run_tags: Vec<String>,
    },
    /// Generate an HTML report of all experiments
    Report {
        output: PathBuf,
    },
    /// Export data as JSON
    Export {
        output: PathBuf,
    },
    /// Show stats: mean/std, improvement rate, crash rate, etc.
    Stats,
    /// Initialize a new resman data directory
    Init {
        path: Option<PathBuf>,
    },
}
