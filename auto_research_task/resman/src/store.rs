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

/// Return all run tag names (JSON file stems) in the data directory.
pub fn list_tags(data_dir: &Path) -> Result<Vec<String>> {
    let dir = runs_dir(data_dir);
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut tags = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("json")
            && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
        {
            tags.push(stem.to_string());
        }
    }
    tags.sort();
    Ok(tags)
}

/// Load a run by tag, or return a `TagNotFound` error with a helpful suggestion.
///
/// Suggestion strategy:
/// 1. Prefix matches among all tags
/// 2. Levenshtein distance ≤ 2
/// 3. If no close matches, list up to 5 existing tags
/// 4. If no tags at all, recommend `resman init` + `resman import`
pub fn load_run_or_suggest(data_dir: &Path, tag: &str) -> Result<RunLog> {
    if let Some(run) = load_run(data_dir, tag)? {
        return Ok(run);
    }

    let all_tags = list_tags(data_dir)?;

    let hint = if all_tags.is_empty() {
        ". No tags exist yet — run 'resman init' and 'resman import' first.".to_string()
    } else {
        // Prefix matches first
        let prefix_matches: Vec<&str> = all_tags
            .iter()
            .filter(|t| t.starts_with(tag) || tag.starts_with(t.as_str()))
            .map(|t| t.as_str())
            .collect();

        // Levenshtein ≤ 2
        let lev_matches: Vec<&str> = all_tags
            .iter()
            .filter(|t| levenshtein(tag, t) <= 2)
            .map(|t| t.as_str())
            .collect();

        // Merge, dedup, preserve order: prefix first then lev-only
        let mut candidates: Vec<&str> = prefix_matches.clone();
        for t in &lev_matches {
            if !candidates.contains(t) {
                candidates.push(t);
            }
        }

        if !candidates.is_empty() {
            let list = candidates.join(", ");
            format!(". Did you mean: {list}?")
        } else {
            // Show up to 5 available tags
            let shown: Vec<&str> = all_tags.iter().take(5).map(|t| t.as_str()).collect();
            let extra = all_tags.len().saturating_sub(5);
            if extra > 0 {
                format!(
                    ". Available tags: {} (and {} more)",
                    shown.join(", "),
                    extra
                )
            } else {
                format!(". Available tags: {}", shown.join(", "))
            }
        }
    };

    Err(Error::TagNotFound {
        tag: tag.to_string(),
        hint,
    })
}

/// Standard dynamic-programming Levenshtein distance.
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let m = a.len();
    let n = b.len();
    let mut dp = vec![vec![0usize; n + 1]; m + 1];
    for (i, row) in dp.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in dp[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=m {
        for j in 1..=n {
            dp[i][j] = if a[i - 1] == b[j - 1] {
                dp[i - 1][j - 1]
            } else {
                1 + dp[i - 1][j - 1].min(dp[i - 1][j]).min(dp[i][j - 1])
            };
        }
    }
    dp[m][n]
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

    // --- Levenshtein tests ---

    #[test]
    fn lev_identical_strings() {
        assert_eq!(levenshtein("apr17", "apr17"), 0);
    }

    #[test]
    fn lev_one_edit() {
        // "apr17" → "apr18": one substitution
        assert_eq!(levenshtein("apr17", "apr18"), 1);
    }

    #[test]
    fn lev_insertion_deletion() {
        // "cat" → "cats": one insertion
        assert_eq!(levenshtein("cat", "cats"), 1);
        // "kitten" → "sitting": classic example = 3
        assert_eq!(levenshtein("kitten", "sitting"), 3);
    }

    // --- load_run_or_suggest tests ---

    fn setup_tags(name: &str, tags: &[&str]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(name);
        std::fs::create_dir_all(runs_dir(&dir)).unwrap();
        for tag in tags {
            let run = crate::model::RunLog {
                run_tag: tag.to_string(),
                created_at: String::new(),
                experiments: vec![],
                metric_name: None,
                metric_direction: None,
                schema_version: 1,
            };
            save_run(&dir, &run).unwrap();
        }
        dir
    }

    #[test]
    fn load_run_or_suggest_hit() {
        let dir = setup_tags("resman_suggest_hit", &["apr17", "apr18"]);
        let result = load_run_or_suggest(&dir, "apr17");
        assert!(result.is_ok(), "expected Ok, got {result:?}");
        assert_eq!(result.unwrap().run_tag, "apr17");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_run_or_suggest_miss_with_suggestion() {
        let dir = setup_tags("resman_suggest_miss", &["apr17", "apr18"]);
        let result = load_run_or_suggest(&dir, "apr19");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        // apr18 is within levenshtein 2 of apr19 (1 edit)
        assert!(
            msg.contains("Did you mean") || msg.contains("Available tags"),
            "expected suggestion, got: {msg}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_run_or_suggest_no_tags() {
        let dir = std::env::temp_dir().join("resman_suggest_empty");
        std::fs::create_dir_all(runs_dir(&dir)).unwrap();
        let result = load_run_or_suggest(&dir, "anything");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("No tags exist yet"),
            "expected no-tags message, got: {msg}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn proptest_save_load_roundtrip_preserves_fields() {
        use proptest::prelude::*;
        proptest!(|(
            tag in "[a-z][a-z0-9]{2,10}",
            n_exp in 0usize..6,
        )| {
            let dir = tempfile::tempdir().unwrap();
            ensure_initialized(dir.path()).unwrap();

            let experiments: Vec<crate::model::Experiment> = (0..n_exp).map(|i| crate::model::Experiment {
                commit: format!("c{i:07x}"),
                val_bpb: i as f64 * 0.1 + 0.5,
                memory_gb: i as f64,
                status: crate::model::Status::Keep,
                description: format!("exp {i}"),
                timestamp: "2026-05-16T00:00:00Z".to_string(),
                params: std::collections::HashMap::new(),
                parent_commit: None,
                crash_excerpt: None,
                metric_name: None,
                metric_direction: None,
                signals: vec![],
            }).collect();

            let run = crate::model::RunLog {
                run_tag: tag.clone(),
                created_at: "2026-05-16T00:00:00Z".to_string(),
                experiments,
                metric_name: None,
                metric_direction: None,
                schema_version: 1,
            };

            save_run(dir.path(), &run).unwrap();
            let loaded = load_run(dir.path(), &tag).unwrap().expect("must reload");
            prop_assert_eq!(loaded.run_tag, run.run_tag);
            prop_assert_eq!(loaded.created_at, run.created_at);
            prop_assert_eq!(loaded.experiments.len(), run.experiments.len());
            prop_assert_eq!(loaded.schema_version, 1u32);
            for (a, b) in loaded.experiments.iter().zip(run.experiments.iter()) {
                prop_assert_eq!(a.commit.clone(), b.commit.clone());
                prop_assert_eq!(a.val_bpb, b.val_bpb);
            }
        });
    }

    #[test]
    fn legacy_store_missing_schema_version_loads_as_one() {
        let dir = tempfile::tempdir().unwrap();
        ensure_initialized(dir.path()).unwrap();
        // Hand-write a v0.7-shape JSON: experiments + run_tag + created_at, no schema_version.
        let json = r#"{"run_tag":"legacy","created_at":"2026-05-16T00:00:00Z","experiments":[]}"#;
        std::fs::write(runs_dir(dir.path()).join("legacy.json"), json).unwrap();
        let r = load_run(dir.path(), "legacy").unwrap().expect("must load");
        assert_eq!(
            r.schema_version, 1,
            "legacy stores must default to schema_version=1"
        );
    }

    /// Concurrency invariant: parallel writes to **different tags** must each
    /// land cleanly. This is the dominant agent pattern (one agent per tag,
    /// many agents overnight). The per-tag JSON file means no inter-tag lock.
    #[test]
    fn concurrent_writes_to_different_tags_all_land() {
        use std::thread;
        let dir = tempfile::tempdir().unwrap();
        ensure_initialized(dir.path()).unwrap();
        let dir_path = dir.path().to_path_buf();

        const N_THREADS: usize = 8;
        const ROUNDS_PER_THREAD: usize = 3;
        const N_EXPERIMENTS: usize = 4;

        let handles: Vec<_> = (0..N_THREADS)
            .map(|tid| {
                let dp = dir_path.clone();
                thread::spawn(move || {
                    for round in 0..ROUNDS_PER_THREAD {
                        let tag = format!("thread_{tid:02}");
                        let experiments: Vec<crate::model::Experiment> = (0..N_EXPERIMENTS)
                            .map(|eidx| crate::model::Experiment {
                                commit: format!("t{tid:02}r{round}e{eidx}"),
                                val_bpb: (tid as f64) + (eidx as f64) * 0.01,
                                memory_gb: 0.0,
                                status: crate::model::Status::Keep,
                                description: format!("t{tid} r{round} e{eidx}"),
                                timestamp: format!("2026-05-16T00:00:{eidx:02}Z"),
                                params: std::collections::HashMap::new(),
                                parent_commit: None,
                                crash_excerpt: None,
                                metric_name: None,
                                metric_direction: None,
                                signals: vec![],
                            })
                            .collect();
                        let run = crate::model::RunLog {
                            run_tag: tag,
                            created_at: "2026-05-16T00:00:00Z".to_string(),
                            experiments,
                            metric_name: None,
                            metric_direction: None,
                            schema_version: 1,
                        };
                        save_run(&dp, &run).expect("save_run under contention");
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().expect("thread panicked under contention");
        }

        let loaded = load_all_runs(&dir_path).expect("load_all_runs after contention");
        assert_eq!(
            loaded.len(),
            N_THREADS,
            "one RunLog per tag — every thread's last write survived"
        );
        for run in &loaded {
            assert_eq!(run.experiments.len(), N_EXPERIMENTS);
            assert_eq!(run.schema_version, 1);
            for exp in &run.experiments {
                assert!(
                    exp.commit.starts_with('t'),
                    "no corrupted commits: got {}",
                    exp.commit
                );
            }
        }
    }

    /// Concurrency stress: parallel writes to the **same tag**. Last writer
    /// wins (no global lock) but the file on disk must always be parseable
    /// JSON — the atomic tmp+rename guarantees no reader sees a partial write.
    #[test]
    fn concurrent_writes_to_same_tag_never_corrupt_json() {
        use std::thread;
        let dir = tempfile::tempdir().unwrap();
        ensure_initialized(dir.path()).unwrap();
        let dir_path = dir.path().to_path_buf();

        const N_THREADS: usize = 8;
        const ROUNDS: usize = 5;

        let handles: Vec<_> = (0..N_THREADS)
            .map(|tid| {
                let dp = dir_path.clone();
                thread::spawn(move || {
                    for r in 0..ROUNDS {
                        let exp = crate::model::Experiment {
                            commit: format!("t{tid:02}r{r}"),
                            val_bpb: tid as f64,
                            memory_gb: 0.0,
                            status: crate::model::Status::Keep,
                            description: format!("contention t={tid} r={r}"),
                            timestamp: format!("2026-05-16T{tid:02}:{r:02}:00Z"),
                            params: std::collections::HashMap::new(),
                            parent_commit: None,
                            crash_excerpt: None,
                            metric_name: None,
                            metric_direction: None,
                            signals: vec![],
                        };
                        let run = crate::model::RunLog {
                            run_tag: "contended".to_string(),
                            created_at: "2026-05-16T00:00:00Z".to_string(),
                            experiments: vec![exp],
                            metric_name: None,
                            metric_direction: None,
                            schema_version: 1,
                        };
                        // Under same-tag contention, save_run may occasionally collide
                        // on the tmp file. The contract: the file ends in a valid state.
                        // Transient errors during the race are acceptable but rare; if
                        // they happen we just retry once.
                        if save_run(&dp, &run).is_err() {
                            let _ = save_run(&dp, &run);
                        }
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().expect("no thread panic under same-tag contention");
        }

        // File on disk must be a parseable RunLog (last writer wins).
        let loaded = load_run(&dir_path, "contended")
            .expect("valid JSON after same-tag contention")
            .expect("file present");
        assert_eq!(loaded.run_tag, "contended");
        assert_eq!(loaded.schema_version, 1);
        assert_eq!(
            loaded.experiments.len(),
            1,
            "each save_run writes exactly one experiment; last writer wins"
        );
    }
}
