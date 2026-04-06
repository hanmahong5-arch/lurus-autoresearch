use std::env;
use std::fs;
use std::path::Path;
use crate::model::RunLog;

/// Returns the default data directory (~/.resman)
pub fn default_data_dir() -> std::path::PathBuf {
    let home = if let Ok(home) = env::var("USERPROFILE") {
        home
    } else if let Ok(home) = env::var("HOME") {
        home
    } else {
        ".".into()
    };
    std::path::PathBuf::from(home).join(".resman")
}

/// Load all runs from the data directory's runs/ folder
pub fn load_all_runs(data_dir: &Path) -> Vec<RunLog> {
    let runs_dir = data_dir.join("runs");
    if !runs_dir.exists() {
        return vec![];
    }

    let mut runs = Vec::new();
    for entry in fs::read_dir(&runs_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(run) = serde_json::from_str::<RunLog>(&content) {
                    runs.push(run);
                }
            }
        }
    }
    runs.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    runs
}

/// Truncate a string to max_len bytes, appending "..." if needed
pub fn truncate(s: &str, max_len: usize) -> &str {
    if s.len() > max_len {
        let cut = max_len.saturating_sub(3);
        // Avoid splitting UTF-8 char boundary
        let end = s.char_indices()
            .take_while(|(i, _)| *i <= cut)
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(0);
        &s[..end]
    } else {
        s
    }
}
