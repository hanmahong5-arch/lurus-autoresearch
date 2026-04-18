use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::model::RunLog;

/// Returns the default data directory.
///
/// Precedence:
/// 1. `$RESMAN_HOME` (explicit override for CI / multi-project setups)
/// 2. `$XDG_DATA_HOME/resman` (Linux convention)
/// 3. `~/.resman` (fallback, macOS/Windows)
pub fn default_data_dir() -> PathBuf {
    if let Ok(p) = env::var("RESMAN_HOME") {
        return PathBuf::from(p);
    }
    if let Ok(p) = env::var("XDG_DATA_HOME") {
        return PathBuf::from(p).join("resman");
    }
    let home = env::var("USERPROFILE")
        .or_else(|_| env::var("HOME"))
        .unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".resman")
}

pub fn runs_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("runs")
}

pub fn ensure_initialized(data_dir: &Path) -> Result<()> {
    fs::create_dir_all(runs_dir(data_dir))?;
    Ok(())
}

/// Load all runs from the data directory, sorted by `created_at` ascending.
/// Silently skips malformed JSON files (with a stderr warning) so one bad file
/// doesn't break the whole workspace.
pub fn load_all_runs(data_dir: &Path) -> Result<Vec<RunLog>> {
    let dir = runs_dir(data_dir);
    if !dir.exists() {
        return Ok(vec![]);
    }

    let mut runs = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        match fs::read_to_string(&path).and_then(|s| {
            serde_json::from_str::<RunLog>(&s)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        }) {
            Ok(run) => runs.push(run),
            Err(e) => eprintln!("warning: skipping {}: {}", path.display(), e),
        }
    }
    runs.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    Ok(runs)
}

pub fn load_run(data_dir: &Path, tag: &str) -> Result<Option<RunLog>> {
    let path = runs_dir(data_dir).join(format!("{tag}.json"));
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&path)?;
    Ok(Some(serde_json::from_str(&content)?))
}

pub fn save_run(data_dir: &Path, run: &RunLog) -> Result<PathBuf> {
    ensure_initialized(data_dir)?;
    let path = runs_dir(data_dir).join(format!("{}.json", run.run_tag));
    // Atomic write: tmp + rename, so an interrupted write never produces a
    // half-file that breaks `load_all_runs` for the next invocation.
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(run)?;
    fs::write(&tmp, json)?;
    fs::rename(&tmp, &path)?;
    Ok(path)
}

pub fn require_run(data_dir: &Path, tag: &str) -> Result<RunLog> {
    load_run(data_dir, tag)?
        .ok_or_else(|| Error::NotFound(runs_dir(data_dir).join(format!("{tag}.json"))))
}

/// Truncate a string to `max_len` display columns, appending "…" if cut.
/// UTF-8 safe.
pub fn truncate(s: &str, max_len: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_len {
        return s.to_string();
    }
    let cut = max_len.saturating_sub(1);
    let mut out: String = chars.into_iter().take(cut).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_short_passthrough() {
        assert_eq!(truncate("hi", 10), "hi");
    }

    #[test]
    fn truncate_long_cuts_with_ellipsis() {
        assert_eq!(truncate("hello world", 5), "hell…");
    }

    #[test]
    fn truncate_utf8_safe() {
        // Multi-byte chars count as 1 display column here.
        assert_eq!(truncate("你好世界和平", 3), "你好…");
    }
}
