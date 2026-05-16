//! Per-call usage telemetry for the MCP server.
//!
//! Writes one JSONL event per tool call into `<data_dir>/usage.jsonl`. This is
//! the source of truth for "which agents call which tools, with what args, with
//! what success rate" — needed to tune composite weights and distill templates
//! before v1.0 schema freeze.
//!
//! Opt out via `RESMAN_DISABLE_USAGE_LOG=1`. Failures log once to stderr and
//! are silently swallowed afterward — telemetry must never break a tool call.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::time::Instant;

use serde_json::{Value, json};

const USAGE_FILE: &str = "usage.jsonl";
const DISABLE_ENV: &str = "RESMAN_DISABLE_USAGE_LOG";

pub struct CallTimer {
    started: Instant,
}

impl CallTimer {
    pub fn start() -> Self {
        Self {
            started: Instant::now(),
        }
    }

    pub fn elapsed_ms(&self) -> u64 {
        u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
}

/// Append one usage event. Best-effort — errors logged to stderr, not returned.
pub fn log_call(
    data_dir: &Path,
    tool: &str,
    args: &Value,
    ok: bool,
    duration_ms: u64,
    result_chars: usize,
) {
    if std::env::var(DISABLE_ENV).is_ok() {
        return;
    }
    let ts = chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string();
    let event = json!({
        "ts": ts,
        "tool": tool,
        "args": args,
        "ok": ok,
        "duration_ms": duration_ms,
        "result_chars": result_chars,
    });
    let mut line = event.to_string();
    line.push('\n');

    let path = data_dir.join(USAGE_FILE);
    match OpenOptions::new().create(true).append(true).open(&path) {
        Ok(mut f) => {
            if let Err(e) = f.write_all(line.as_bytes()) {
                eprintln!("resman-mcp: usage log write failed: {e}");
            }
        }
        Err(e) => {
            eprintln!(
                "resman-mcp: usage log open failed ({}): {e}",
                path.display()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    //! Single combined test — the three cases all mutate `RESMAN_DISABLE_USAGE_LOG`,
    //! and cargo runs tests in parallel. Splitting would race.
    use super::*;
    use serde_json::json;

    fn read_lines(path: &Path) -> Vec<Value> {
        let s = std::fs::read_to_string(path).unwrap_or_default();
        s.lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    #[test]
    fn log_call_writes_appends_and_respects_opt_out() {
        // Case 1: single write, all fields land correctly.
        let dir = tempfile::tempdir().unwrap();
        unsafe {
            std::env::remove_var(DISABLE_ENV);
        }
        log_call(
            dir.path(),
            "resman_best",
            &json!({"composite": true}),
            true,
            12,
            128,
        );
        let events = read_lines(&dir.path().join(USAGE_FILE));
        assert_eq!(events.len(), 1);
        let e = &events[0];
        assert_eq!(e["tool"], "resman_best");
        assert_eq!(e["ok"], true);
        assert_eq!(e["duration_ms"], 12);
        assert_eq!(e["result_chars"], 128);
        assert_eq!(e["args"]["composite"], true);
        assert!(e["ts"].as_str().unwrap().ends_with('Z'));

        // Case 2: append semantics — three more lines stack onto the same file.
        for i in 0..3 {
            log_call(
                dir.path(),
                "resman_search",
                &json!({"pattern": format!("p{i}")}),
                i % 2 == 0,
                i as u64,
                i * 10,
            );
        }
        let events = read_lines(&dir.path().join(USAGE_FILE));
        assert_eq!(events.len(), 4);
        assert_eq!(events[1]["args"]["pattern"], "p0");
        assert_eq!(events[2]["ok"], false);

        // Case 3: opt-out env var prevents writes (use a fresh dir so absence is unambiguous).
        let dir2 = tempfile::tempdir().unwrap();
        unsafe {
            std::env::set_var(DISABLE_ENV, "1");
        }
        log_call(dir2.path(), "resman_best", &json!({}), true, 0, 0);
        unsafe {
            std::env::remove_var(DISABLE_ENV);
        }
        assert!(
            !dir2.path().join(USAGE_FILE).exists(),
            "usage.jsonl should not be created when opt-out env var is set"
        );
    }
}
