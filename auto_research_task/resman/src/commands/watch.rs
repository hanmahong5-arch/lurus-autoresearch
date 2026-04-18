use std::fs;
use std::path::Path;
use std::thread;
use std::time::{Duration, SystemTime};

use crate::error::{Error, Result};
use crate::store::runs_dir;

/// Poll `path`'s mtime; re-import whenever it changes. Designed to run during
/// overnight agent sessions: the user's agent appends rows to `results.tsv`
/// while `resman watch` keeps the resman store and reports in sync.
pub fn cmd_watch(
    data_dir: &Path,
    path: &Path,
    tag: Option<String>,
    interval_secs: u64,
) -> Result<()> {
    if !path.exists() {
        return Err(Error::NotFound(path.to_path_buf()));
    }
    let interval = Duration::from_secs(interval_secs.max(1));
    let tag = tag.unwrap_or_else(|| {
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("untagged")
            .to_string()
    });

    println!(
        "watching {} → tag `{tag}` (interval {}s)",
        path.display(),
        interval.as_secs()
    );
    println!("ctrl-c to stop.");
    println!();

    // Make sure the target run dir exists so the user can see progress.
    let _ = runs_dir(data_dir);

    let mut last_mtime: Option<SystemTime> = None;
    let mut last_count: usize = 0;

    loop {
        let mtime = fs::metadata(path).and_then(|m| m.modified()).ok();
        if mtime != last_mtime {
            last_mtime = mtime;
            match super::import::cmd_import(data_dir, path, Some(tag.clone()), true, None, None) {
                Ok(_) => {
                    // After import, read back the current count so we can show deltas.
                    if let Ok(Some(run)) = crate::store::load_run(data_dir, &tag) {
                        let new_count = run.experiments.len();
                        if new_count != last_count {
                            let delta = new_count as isize - last_count as isize;
                            let sign = if delta > 0 { "+" } else { "" };
                            println!(
                                "  → {new_count} total ({sign}{delta}) | best: {}",
                                run.best()
                                    .map(|e| format!("{:.6}", e.val_bpb))
                                    .unwrap_or_else(|| "—".into())
                            );
                            last_count = new_count;
                        }
                    }
                }
                Err(e) => eprintln!("import failed: {e}"),
            }
        }
        thread::sleep(interval);
    }
}
