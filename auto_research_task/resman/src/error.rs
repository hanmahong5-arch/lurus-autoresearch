use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid status: {0} (expected keep|discard|crash|best)")]
    InvalidStatus(String),

    #[error("file not found: {0}")]
    NotFound(PathBuf),

    #[error("no experiments found — run `resman import <results.tsv>` or `resman add ...` first")]
    Empty,

    #[error("malformed TSV at line {line}: expected >=4 tab-separated columns, got {got}")]
    MalformedTsv { line: usize, got: usize },

    #[error("invalid float in column {column} at line {line}: {value}")]
    InvalidFloat {
        line: usize,
        column: &'static str,
        value: String,
    },

    #[error("run tag `{0}` already exists; use --force to overwrite")]
    DuplicateTag(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error(transparent)]
    Regex(#[from] regex::Error),

    #[error(transparent)]
    Glob(#[from] glob::PatternError),
}

pub type Result<T> = std::result::Result<T, Error>;
