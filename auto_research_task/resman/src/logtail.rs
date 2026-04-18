//! Log-tail helper: extract the last N lines of a file.
//!
//! Used by `resman add --log run.log` when recording a crash, so the agent
//! can later read the traceback without keeping the full log around. Responds
//! to upstream PR #101 and bd75534 (crash diagnostics via traceback reading).

use std::fs;
use std::path::Path;

/// Read a file and return the last `n` non-empty lines, joined by '\n'.
///
/// For the log sizes we care about (training logs under ~10 MB) we just read
/// the whole file — simplest correct implementation. If this ever becomes a
/// bottleneck we can switch to reverse-reading with seek.
pub fn tail_lines(path: &Path, n: usize) -> std::io::Result<String> {
    let content = fs::read_to_string(path)?;
    let lines: Vec<&str> = content.lines().collect();
    let start = lines.len().saturating_sub(n);
    Ok(lines[start..].join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(name: &str, body: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("resman_test_{name}"));
        fs::write(&p, body).unwrap();
        p
    }

    #[test]
    fn tails_last_n() {
        let p = scratch("tail_n", "one\ntwo\nthree\nfour\nfive\n");
        assert_eq!(tail_lines(&p, 3).unwrap(), "three\nfour\nfive");
    }

    #[test]
    fn tail_shorter_than_file_returns_all() {
        let p = scratch("tail_short", "a\nb\n");
        assert_eq!(tail_lines(&p, 50).unwrap(), "a\nb");
    }
}
