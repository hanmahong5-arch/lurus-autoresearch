use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(
    name = "resman",
    about = "Local-first experiment tracker for autonomous AI training agents",
    long_about = "resman — track, compare, and report ML training experiments from the terminal.\n\
                  \n\
                  Built for the era of AI agents that run 100 experiments overnight.\n\
                  Zero config, no account, no cloud. One binary. Git-native. Machine-readable.",
    author,
    version
)]
pub struct Cli {
    /// Override the data directory (default: $RESMAN_HOME, $XDG_DATA_HOME/resman, or ~/.resman)
    #[arg(short = 'D', long, global = true)]
    pub data_dir: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(ValueEnum, Clone, Debug)]
pub enum OutputFormat {
    /// Human-readable table (default)
    Table,
    /// JSON — for piping into jq, agents, dashboards
    Json,
    /// TSV — for pasting into spreadsheets
    Tsv,
}

#[derive(ValueEnum, Clone, Debug)]
pub enum SortField {
    ValBpb,
    MemoryGb,
    Description,
    Commit,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize a resman data directory
    Init { path: Option<PathBuf> },

    /// Import a results.tsv file from an autoresearch run
    Import {
        /// Path to results.tsv
        path: PathBuf,
        /// Tag for this run (default: filename stem)
        #[arg(short, long)]
        tag: Option<String>,
        /// Overwrite an existing run with this tag
        #[arg(short, long)]
        force: bool,
        /// Primary metric name (e.g. "eval_loss", "rouge_l"). Default: "val_bpb".
        #[arg(long)]
        metric_name: Option<String>,
        /// Direction: min (lower better, default) or max (higher better).
        #[arg(long)]
        metric_direction: Option<String>,
    },

    /// Append a single experiment to a run (agent-friendly, no TSV needed)
    Add {
        /// Run tag to append to (created if it doesn't exist)
        #[arg(short, long)]
        tag: String,
        /// Short git commit hash
        #[arg(short, long)]
        commit: String,
        /// Validation bits-per-byte (lower is better; 0 for crashes)
        #[arg(short = 'v', long)]
        val_bpb: f64,
        /// Peak GPU memory in GB
        #[arg(short, long, default_value_t = 0.0)]
        memory_gb: f64,
        /// Status: keep, discard, crash, best
        #[arg(short, long, default_value = "keep")]
        status: String,
        /// Free-text description of what was tried
        #[arg(short, long)]
        description: String,
        /// Extra key=value params to attach (repeatable)
        #[arg(short = 'p', long = "param")]
        params: Vec<String>,
        /// Parent commit this experiment was branched from (for lineage)
        #[arg(long)]
        parent: Option<String>,
        /// On crash, siphon the last 50 lines of this log file into the record
        #[arg(long)]
        log: Option<PathBuf>,
        /// Skip `nvidia-smi` auto-probe for GPU name
        #[arg(long)]
        no_gpu_probe: bool,
        /// Primary metric name (e.g. "eval_loss", "rouge_l"). Default: "val_bpb".
        #[arg(long)]
        metric_name: Option<String>,
        /// Direction: min (lower better, default) or max (higher better).
        #[arg(long)]
        metric_direction: Option<String>,
    },

    /// Search every experiment's description / commit / params (case-insensitive regex)
    ///
    /// Primary use: answer "has the agent already tried this?" before a new run.
    Search {
        /// Regex pattern (e.g. 'GeLU|gelu')
        pattern: String,
        /// Include discarded experiments (default: skip them)
        #[arg(short = 'd', long)]
        include_discarded: bool,
        #[arg(short = 'o', long, default_value = "table")]
        format: OutputFormat,
    },

    /// Find experiments with val_bpb closest to a target
    Near {
        /// Target val_bpb
        val_bpb: f64,
        /// Number of neighbors
        #[arg(short, long, default_value_t = 5)]
        n: usize,
        #[arg(short = 'o', long, default_value = "table")]
        format: OutputFormat,
    },

    /// Run as an MCP (Model Context Protocol) server over stdio
    ///
    /// Agent harnesses (Claude Code, Cursor, Codex) spawn this process and
    /// call tools via JSON-RPC 2.0. See docs/MCP.md for wiring.
    Mcp,

    /// Parse training logs to extract val_bpb and stats (glob supported)
    ParseLog { pattern: String },

    /// List experiments across runs
    List {
        /// Filter by status (keep/discard/crash/best/all). Default: kept-only
        #[arg(short, long)]
        status: Option<String>,
        /// Sort by this field
        #[arg(short = 'S', long, default_value = "val-bpb")]
        sort_by: SortField,
        /// Filter description with regex
        #[arg(short, long)]
        grep: Option<String>,
        /// Show only top N
        #[arg(short, long)]
        top: Option<usize>,
        /// Reverse sort order
        #[arg(long)]
        reverse: bool,
        /// Restrict to a single run tag
        #[arg(long)]
        tag: Option<String>,
        /// Output format
        #[arg(short = 'o', long, default_value = "table")]
        format: OutputFormat,
        /// Filter to experiments whose signals contain this type (oom, cuda_error,
        /// nan_loss, assert_fail, timeout, unknown). Repeatable — AND across values.
        #[arg(long)]
        signal: Vec<String>,
    },

    /// Print the best experiment across all runs (or a single tag), one line
    ///
    /// Designed for shell scripts and agent loops:
    ///   BEST=$(resman best --format value)
    ///   if (( $(echo "$NEW < $BEST" | bc -l) )); then ... fi
    Best {
        /// Restrict to a single run tag
        #[arg(short, long)]
        tag: Option<String>,
        /// Output format: table (default), value (just val_bpb), json
        #[arg(short, long, default_value = "table")]
        format: String,
        /// Use composite scoring (metric + verification + lineage + description).
        /// Default false — plain metric ranking unchanged.
        #[arg(long, default_value_t = false)]
        composite: bool,
    },

    /// Compare best experiments across multiple runs
    Compare {
        run_tags: Vec<String>,
        #[arg(short = 'o', long, default_value = "table")]
        format: OutputFormat,
    },

    /// Generate an HTML report with SVG trend chart
    Report {
        output: PathBuf,
        /// Report title (default: "Research Experiment Report")
        #[arg(long)]
        title: Option<String>,
    },

    /// Export all data as JSON
    Export { output: PathBuf },

    /// Watch results.tsv and auto-import on change
    ///
    /// Runs forever, polling every --interval seconds. Emits a short summary
    /// on each import so the user (or their agent) can tail it.
    Watch {
        /// Path to results.tsv
        path: PathBuf,
        /// Tag for this run (default: filename stem)
        #[arg(short, long)]
        tag: Option<String>,
        /// Poll interval in seconds
        #[arg(short, long, default_value_t = 2)]
        interval: u64,
    },

    /// Show stats: mean/std, improvement rate, crash rate, etc.
    Stats {
        /// Restrict to a single run tag
        #[arg(short, long)]
        tag: Option<String>,
    },

    /// Show the config/metric diff between the representative experiment of two runs.
    ///
    /// Useful for "why did this branch win?" analysis after overnight agent runs.
    Diff {
        /// First run tag
        tag_a: String,
        /// Second run tag
        tag_b: String,
        /// Which experiment to compare: best | latest
        #[arg(long, default_value = "best")]
        against: String,
        /// Output format
        #[arg(short = 'o', long, default_value = "table")]
        format: OutputFormat,
    },

    /// Render the lineage tree of a run via `parent_commit` links.
    ///
    /// Root nodes have no parent or point to a commit not in this run.
    Tree {
        /// Run tag to render
        #[arg(short, long)]
        tag: String,
        /// Only render the lineage leading to the best experiment
        #[arg(long)]
        highlight_best: bool,
        /// Output format
        #[arg(short = 'o', long, default_value = "table")]
        format: OutputFormat,
    },

    /// Generate a structured Markdown/JSON summary of a run (best, lineage,
    /// failure signals, unexplored neighbors, heuristic suggestions).
    ///
    /// The primary "what did we learn last night?" artifact for agent memory.
    Distill {
        /// Run tag to distill
        #[arg(short, long)]
        tag: String,
        /// Write output to this file instead of stdout
        #[arg(long)]
        out: Option<std::path::PathBuf>,
        /// Output format: markdown (default) or json
        #[arg(short = 'o', long, default_value = "markdown")]
        format: crate::commands::distill::DistillFormat,
    },

    /// Re-verify an experiment by providing a new metric value from a re-run.
    ///
    /// If the new value is within tolerance in the expected direction,
    /// the experiment is promoted to status `verified` and val_bpb is updated.
    /// Does NOT orchestrate training — you run the experiment yourself and pass
    /// the result in via --value.
    Verify {
        /// Full or short commit hash to verify (prefix match)
        commit: String,
        /// New metric value from the re-run
        #[arg(short = 'v', long)]
        value: f64,
        /// Absolute tolerance (default 0.01)
        #[arg(short = 't', long, default_value_t = 0.01)]
        tolerance: f64,
        /// Restrict search to this run tag (optional)
        #[arg(long)]
        tag: Option<String>,
    },
}
