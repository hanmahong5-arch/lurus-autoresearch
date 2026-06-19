use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid status: {0} (expected keep|discard|crash|best|verified)")]
    InvalidStatus(String),

    #[error("invalid direction: {0} (expected min|max)")]
    InvalidDirection(String),

    #[error("file not found: {0} — check the path (or `resman init` to create a new store)")]
    NotFound(PathBuf),

    #[error(
        "no experiments found — run `resman import <results.tsv>` or `resman add ...` first (if you haven't created a store yet, run `resman init`)"
    )]
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

    #[error("tag `{tag}` not found{hint}")]
    TagNotFound { tag: String, hint: String },

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error(transparent)]
    Regex(#[from] regex::Error),

    #[error(transparent)]
    Glob(#[from] glob::PatternError),

    #[error("{0}")]
    Import(String),

    #[error("{0}")]
    Custom(String),
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_error_hints_init() {
        let msg = Error::Empty.to_string();
        assert!(
            msg.contains("resman init"),
            "Empty error should mention `resman init`"
        );
    }

    #[test]
    fn not_found_error_hints_init() {
        let msg = Error::NotFound(PathBuf::from("/tmp/nope")).to_string();
        assert!(
            msg.contains("resman init"),
            "NotFound error should mention `resman init`"
        );
        assert!(
            msg.contains("/tmp/nope"),
            "NotFound error should include the path"
        );
    }
}
