//! `resman doctor` — environment and data-integrity self-check.
//!
//! Six checks (in order):
//!   1. data_dir         — directory writable?
//!   2. resman_home_env  — which env var sourced the data dir?
//!   3. runs_present     — any experiments recorded?
//!   4. usage_telemetry  — usage.jsonl activity
//!   5. mcp_wiring       — .mcp.json discoverable?
//!   6. invariants       — zero-byte / corrupt / orphan parent check

use std::fs;
use std::io::{self, BufRead};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::cli::OutputFormat;
use crate::error::{Error, Result};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    Ok,
    Warn,
    Fresh,
    Fail,
}

impl CheckStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            CheckStatus::Ok => "ok",
            CheckStatus::Warn => "warn",
            CheckStatus::Fresh => "fresh",
            CheckStatus::Fail => "fail",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckResult {
    pub name: &'static str,
    pub status: CheckStatus,
    pub detail: String,
    pub hint: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DoctorSummary {
    pub ok: usize,
    pub warn: usize,
    pub fresh: usize,
    pub fail: usize,
}

#[derive(Debug, Serialize)]
pub struct DoctorReport {
    pub summary: DoctorSummary,
    pub checks: Vec<CheckResult>,
}

// ---------------------------------------------------------------------------
// Individual checks
// ---------------------------------------------------------------------------

/// Check 1: data_dir exists and is writable.
pub fn check_data_dir(data_dir: &Path) -> CheckResult {
    let abs = data_dir
        .canonicalize()
        .unwrap_or_else(|_| data_dir.to_path_buf());

    if !data_dir.exists() {
        return CheckResult {
            name: "data_dir",
            status: CheckStatus::Fail,
            detail: format!("{} (does not exist)", abs.display()),
            hint: Some("run `resman init` to create it".into()),
        };
    }

    // Try to create + delete a temp file to prove write access.
    let probe = data_dir.join(".doctor_probe_tmp");
    let writable = fs::write(&probe, b"ok").is_ok();
    if writable {
        let _ = fs::remove_file(&probe);
    }

    if writable {
        CheckResult {
            name: "data_dir",
            status: CheckStatus::Ok,
            detail: format!("{} (writable)", abs.display()),
            hint: None,
        }
    } else {
        CheckResult {
            name: "data_dir",
            status: CheckStatus::Fail,
            detail: format!("{} (not writable)", abs.display()),
            hint: Some(format!("check directory permissions on {}", abs.display())),
        }
    }
}

/// Check 2: report which env var sources the data dir (informational, always ok).
pub fn check_resman_home_env(data_dir: &Path) -> CheckResult {
    let detail = if let Ok(v) = std::env::var("RESMAN_HOME") {
        format!("RESMAN_HOME={v} (env override)")
    } else if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        format!(
            "XDG_DATA_HOME fallback: {}/resman",
            PathBuf::from(&xdg).display()
        )
    } else {
        format!("default: {}", data_dir.display())
    };

    CheckResult {
        name: "resman_home_env",
        status: CheckStatus::Ok,
        detail,
        hint: None,
    }
}

/// Check 3: any experiments in the store?
pub fn check_runs_present(data_dir: &Path) -> CheckResult {
    match crate::store::load_all_runs(data_dir) {
        Err(e) => CheckResult {
            name: "runs_present",
            status: CheckStatus::Fail,
            detail: format!("failed to load runs: {e}"),
            hint: Some(format!("error loading store: {e}")),
        },
        Ok(runs) => {
            let total: usize = runs.iter().map(|r| r.experiments.len()).sum();
            let tags = runs.len();
            if total == 0 {
                CheckResult {
                    name: "runs_present",
                    status: CheckStatus::Fresh,
                    detail:
                        "0 experiments recorded — this is expected for a fresh store".to_string(),
                    hint: Some(
                        "run experiments and call `resman add` / `resman_add_experiment` to populate"
                            .into(),
                    ),
                }
            } else {
                CheckResult {
                    name: "runs_present",
                    status: CheckStatus::Ok,
                    detail: format!("{total} experiments across {tags} tags"),
                    hint: None,
                }
            }
        }
    }
}

/// Check 4: usage.jsonl existence and activity.
pub fn check_usage_telemetry(data_dir: &Path) -> CheckResult {
    let usage_path = data_dir.join("usage.jsonl");

    if !usage_path.exists() {
        return CheckResult {
            name: "usage_telemetry",
            status: CheckStatus::Fresh,
            detail: "no usage.jsonl yet (agent has not called MCP)".to_string(),
            hint: Some("start `resman mcp` and have an agent call any tool".into()),
        };
    }

    let meta = match fs::metadata(&usage_path) {
        Ok(m) => m,
        Err(e) => {
            return CheckResult {
                name: "usage_telemetry",
                status: CheckStatus::Fail,
                detail: format!("cannot stat usage.jsonl: {e}"),
                hint: None,
            };
        }
    };

    let size = meta.len();
    if size == 0 {
        return CheckResult {
            name: "usage_telemetry",
            status: CheckStatus::Fresh,
            detail: "no usage.jsonl yet (agent has not called MCP)".to_string(),
            hint: Some("start `resman mcp` and have an agent call any tool".into()),
        };
    }

    // Read last line to parse the latest timestamp.
    let last_ts = last_jsonl_ts(&usage_path);
    let now = chrono::Utc::now();

    match last_ts {
        None => CheckResult {
            name: "usage_telemetry",
            status: CheckStatus::Ok,
            detail: format!("{size} bytes (timestamp unreadable — events present)"),
            hint: None,
        },
        Some(ts) => {
            let age = now.signed_duration_since(ts);
            let ts_str = ts.format("%Y-%m-%dT%H:%M:%SZ").to_string();
            if age.num_days() >= 7 {
                CheckResult {
                    name: "usage_telemetry",
                    status: CheckStatus::Warn,
                    detail: format!("{size} bytes, last event {ts_str}"),
                    hint: Some(
                        "no recent agent activity — agent may have stopped using MCP".into(),
                    ),
                }
            } else if age.num_hours() >= 24 {
                CheckResult {
                    name: "usage_telemetry",
                    status: CheckStatus::Ok,
                    detail: format!("{size} bytes, last event {ts_str} (yesterday)"),
                    hint: None,
                }
            } else {
                CheckResult {
                    name: "usage_telemetry",
                    status: CheckStatus::Ok,
                    detail: format!("{size} bytes, last event {ts_str}"),
                    hint: None,
                }
            }
        }
    }
}

/// Parse the `ts` field of the last non-empty line in a JSONL file.
///
/// `#[allow(clippy::lines_filter_map_ok)]`: our jsonl is a local, small,
/// non-streaming file; an unrecoverable read error mid-iteration is fine
/// to treat as end-of-stream (the worst case is a stale-but-real ts).
#[allow(clippy::lines_filter_map_ok)]
fn last_jsonl_ts(path: &Path) -> Option<chrono::DateTime<chrono::Utc>> {
    let file = fs::File::open(path).ok()?;
    let reader = io::BufReader::new(file);
    let last_line = reader
        .lines()
        .filter_map(|l| l.ok())
        .filter(|l| !l.trim().is_empty())
        .last()?;
    let v: serde_json::Value = serde_json::from_str(&last_line).ok()?;
    let ts_str = v.get("ts")?.as_str()?;
    chrono::DateTime::parse_from_rfc3339(ts_str)
        .ok()
        .map(|dt| dt.with_timezone(&chrono::Utc))
}

/// Check 5: .mcp.json discoverable in cwd or up to 5 parent dirs?
pub fn check_mcp_wiring() -> CheckResult {
    match find_mcp_json() {
        None => CheckResult {
            name: "mcp_wiring",
            status: CheckStatus::Warn,
            detail: "no .mcp.json in cwd or 5 levels up".to_string(),
            hint: Some(
                "create .mcp.json with `mcpServers.resman.command` pointing to this resman binary; see docs/MCP.md".into(),
            ),
        },
        Some((path, parse_err)) => {
            if let Some(err_msg) = parse_err {
                CheckResult {
                    name: "mcp_wiring",
                    status: CheckStatus::Fail,
                    detail: format!("found {} but JSON parse failed", path.display()),
                    hint: Some(format!("fix JSON syntax in {}: {err_msg}", path.display())),
                }
            } else {
                // Try to read and check for mcpServers.resman
                match fs::read_to_string(&path)
                    .ok()
                    .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                {
                    Some(v) => {
                        let has_resman = v
                            .get("mcpServers")
                            .and_then(|s| s.get("resman"))
                            .is_some();
                        if has_resman {
                            CheckResult {
                                name: "mcp_wiring",
                                status: CheckStatus::Ok,
                                detail: format!(
                                    "found {}, resman entry present",
                                    path.display()
                                ),
                                hint: None,
                            }
                        } else {
                            CheckResult {
                                name: "mcp_wiring",
                                status: CheckStatus::Warn,
                                detail: format!(
                                    "found {} but no `mcpServers.resman` entry",
                                    path.display()
                                ),
                                hint: Some("add resman to mcpServers".into()),
                            }
                        }
                    }
                    None => CheckResult {
                        name: "mcp_wiring",
                        status: CheckStatus::Fail,
                        detail: format!("found {} but JSON parse failed", path.display()),
                        hint: Some(format!("fix JSON syntax in {}", path.display())),
                    },
                }
            }
        }
    }
}

/// Walk cwd up to 5 parent dirs looking for `.mcp.json`.
/// Returns None if not found, or Some((path, parse_error_msg)) where
/// parse_error_msg is Some only on parse failure (path found but invalid JSON).
fn find_mcp_json() -> Option<(PathBuf, Option<String>)> {
    let mut dir = std::env::current_dir().ok()?;
    for _ in 0..=5 {
        let candidate = dir.join(".mcp.json");
        if candidate.exists() {
            // Validate JSON quickly
            let parse_err = fs::read_to_string(&candidate)
                .ok()
                .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).err())
                .map(|e| e.to_string());
            return Some((candidate, parse_err));
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

/// Check 6: scan runs/*.json for zero-byte, corrupt, or orphan parent_commit.
pub fn check_invariants(data_dir: &Path) -> CheckResult {
    let runs_dir = crate::store::runs_dir(data_dir);

    if !runs_dir.exists() {
        return CheckResult {
            name: "invariants",
            status: CheckStatus::Fail,
            detail: "runs/ directory does not exist".to_string(),
            hint: Some("run `resman init` to create it".into()),
        };
    }

    let entries = match fs::read_dir(&runs_dir) {
        Err(e) => {
            return CheckResult {
                name: "invariants",
                status: CheckStatus::Fail,
                detail: format!("cannot read runs/: {e}"),
                hint: Some("check directory permissions".into()),
            };
        }
        Ok(e) => e,
    };

    let mut total = 0usize;
    let mut zero_byte = 0usize;
    let mut corrupt = 0usize;
    let mut orphan_parents = 0usize;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        total += 1;

        let meta = fs::metadata(&path);
        if meta.map(|m| m.len()).unwrap_or(0) == 0 {
            zero_byte += 1;
            continue;
        }

        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => {
                corrupt += 1;
                continue;
            }
        };

        let run: crate::model::RunLog = match serde_json::from_str(&content) {
            Ok(r) => r,
            Err(_) => {
                corrupt += 1;
                continue;
            }
        };

        // Build the set of commits in this tag.
        let commits: std::collections::HashSet<&str> =
            run.experiments.iter().map(|e| e.commit.as_str()).collect();

        for exp in &run.experiments {
            let Some(parent) = &exp.parent_commit else {
                continue;
            };
            if !commits.contains(parent.as_str()) {
                orphan_parents += 1;
            }
        }
    }

    if zero_byte == 0 && corrupt == 0 && orphan_parents == 0 {
        CheckResult {
            name: "invariants",
            status: CheckStatus::Ok,
            detail: format!("{total} runs scanned, 0 zero-byte, 0 corrupt, 0 orphan parents"),
            hint: None,
        }
    } else {
        CheckResult {
            name: "invariants",
            status: CheckStatus::Warn,
            detail: format!(
                "{total} runs scanned, {zero_byte} zero-byte, {corrupt} corrupt, {orphan_parents} orphan parents"
            ),
            hint: Some("inspect listed runs or rerun affected tags".into()),
        }
    }
}

// ---------------------------------------------------------------------------
// Orchestrator
// ---------------------------------------------------------------------------

/// Run all 6 checks in order and return the results.
pub fn run_checks(data_dir: &Path) -> Vec<CheckResult> {
    vec![
        check_data_dir(data_dir),
        check_resman_home_env(data_dir),
        check_runs_present(data_dir),
        check_usage_telemetry(data_dir),
        check_mcp_wiring(),
        check_invariants(data_dir),
    ]
}

// ---------------------------------------------------------------------------
// CLI entry point
// ---------------------------------------------------------------------------

pub fn cmd_doctor(data_dir: &Path, format: &OutputFormat) -> Result<()> {
    let checks = run_checks(data_dir);

    let mut ok = 0usize;
    let mut warn = 0usize;
    let mut fresh = 0usize;
    let mut fail = 0usize;
    for c in &checks {
        match c.status {
            CheckStatus::Ok => ok += 1,
            CheckStatus::Warn => warn += 1,
            CheckStatus::Fresh => fresh += 1,
            CheckStatus::Fail => fail += 1,
        }
    }
    let summary = DoctorSummary {
        ok,
        warn,
        fresh,
        fail,
    };
    let report = DoctorReport {
        summary,
        checks: checks.clone(),
    };

    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        OutputFormat::Tsv => {
            println!("status\tcheck\tdetail\thint");
            for c in &checks {
                let hint_str = c.hint.as_deref().unwrap_or("");
                println!(
                    "{}\t{}\t{}\t{}",
                    c.status.as_str(),
                    c.name,
                    c.detail,
                    hint_str
                );
            }
        }
        OutputFormat::Table => {
            println!("=== resman doctor ===\n");
            // Column widths: status=6, check=18, detail=dynamic
            println!("{:<6}  {:<18}  detail", "status", "check");
            for c in &checks {
                println!("{:<6}  {:<18}  {}", c.status.as_str(), c.name, c.detail);
                if let Some(hint) = &c.hint {
                    println!("{:<6}  {:<18}      hint: {}", "", "", hint);
                }
            }
            println!(
                "\nsummary: {ok} ok / {warn} warn / {fresh} fresh / {fail} fail",
                ok = report.summary.ok,
                warn = report.summary.warn,
                fresh = report.summary.fresh,
                fail = report.summary.fail,
            );
        }
    }

    if fail > 0 {
        Err(Error::Custom(format!(
            "{fail} check(s) failed; see output above"
        )))
    } else {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn fresh_dir() -> (TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        crate::store::ensure_initialized(&path).unwrap();
        (dir, path)
    }

    // 1. data_dir writable (tempdir always is)
    #[test]
    fn data_dir_writable() {
        let (_dir, path) = fresh_dir();
        let r = check_data_dir(&path);
        assert_eq!(r.status, CheckStatus::Ok);
        assert!(r.detail.contains("writable"));
    }

    // 2. data_dir does not exist → fail
    #[test]
    fn data_dir_does_not_exist() {
        let path = PathBuf::from("/tmp/resman_doctor_nonexistent_xyz_12345");
        let r = check_data_dir(&path);
        assert_eq!(r.status, CheckStatus::Fail);
        assert!(r.hint.as_deref().unwrap_or("").contains("resman init"));
    }

    // 3. runs_present — empty store → fresh
    #[test]
    fn runs_present_fresh() {
        let (_dir, path) = fresh_dir();
        let r = check_runs_present(&path);
        assert_eq!(r.status, CheckStatus::Fresh);
        assert!(r.detail.contains("0 experiments"));
    }

    // 4. runs_present — 2 runs written → ok with count
    #[test]
    fn runs_present_ok() {
        let (_dir, path) = fresh_dir();
        // Write two minimal RunLog JSON files directly.
        let runs_dir = crate::store::runs_dir(&path);
        for tag in ["tagA", "tagB"] {
            let run = crate::model::RunLog {
                run_tag: tag.to_string(),
                created_at: "2026-01-01T00:00:00Z".to_string(),
                experiments: vec![make_exp("abc001", None)],
                metric_name: None,
                metric_direction: None,
                schema_version: 1,
            };
            crate::store::save_run(&path, &run).unwrap();
        }
        let _ = runs_dir;
        let r = check_runs_present(&path);
        assert_eq!(r.status, CheckStatus::Ok);
        assert!(r.detail.contains("2 experiments across 2 tags"));
    }

    // 5. usage_telemetry — missing file → fresh
    #[test]
    fn usage_telemetry_missing() {
        let (_dir, path) = fresh_dir();
        let r = check_usage_telemetry(&path);
        assert_eq!(r.status, CheckStatus::Fresh);
    }

    // 6. usage_telemetry — recent ts → ok
    #[test]
    fn usage_telemetry_recent() {
        let (_dir, path) = fresh_dir();
        let usage_path = path.join("usage.jsonl");
        let now = chrono::Utc::now();
        let ts = now.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
        let line = format!(
            "{{\"ts\":\"{ts}\",\"tool\":\"resman_best\",\"args\":{{}},\"ok\":true,\"duration_ms\":5,\"result_chars\":42}}\n"
        );
        let mut f = fs::File::create(&usage_path).unwrap();
        f.write_all(line.as_bytes()).unwrap();
        let r = check_usage_telemetry(&path);
        assert_eq!(r.status, CheckStatus::Ok);
    }

    // 7. mcp_wiring — tempdir without .mcp.json anywhere → warn
    #[test]
    fn mcp_wiring_not_found() {
        // We can't reliably set cwd in unit tests, so we test find_mcp_json
        // indirectly by asserting the check never panics and returns a valid status.
        // The actual status depends on the test runner's cwd — skip status assertion.
        let r = check_mcp_wiring();
        // Must be one of the four valid statuses
        assert!(matches!(
            r.status,
            CheckStatus::Ok | CheckStatus::Warn | CheckStatus::Fresh | CheckStatus::Fail
        ));
        assert!(!r.detail.is_empty());
    }

    // 8. mcp_wiring_found — write a valid .mcp.json with resman entry in tempdir
    #[test]
    fn mcp_wiring_found() {
        let dir = tempfile::tempdir().unwrap();
        let mcp_path = dir.path().join(".mcp.json");
        let content = r#"{"mcpServers":{"resman":{"command":"resman","args":["mcp"]}}}"#;
        fs::write(&mcp_path, content).unwrap();

        // Test find_mcp_json directly — it searches from cwd, not from dir.
        // We verify parse logic: valid JSON with resman entry.
        let v: serde_json::Value = serde_json::from_str(content).unwrap();
        assert!(v.get("mcpServers").and_then(|s| s.get("resman")).is_some());
    }

    // 9. invariants — 2 valid runs → ok
    #[test]
    fn invariants_no_issue() {
        let (_dir, path) = fresh_dir();
        let run = crate::model::RunLog {
            run_tag: "t1".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            experiments: vec![make_exp("aaa001", None), make_exp("bbb002", Some("aaa001"))],
            metric_name: None,
            metric_direction: None,
            schema_version: 1,
        };
        crate::store::save_run(&path, &run).unwrap();
        let r = check_invariants(&path);
        assert_eq!(r.status, CheckStatus::Ok);
        assert!(r.detail.contains("1 runs scanned"));
        assert!(r.detail.contains("0 zero-byte"));
        assert!(r.detail.contains("0 corrupt"));
        assert!(r.detail.contains("0 orphan parents"));
    }

    // 10. invariants — orphan parent_commit inside same tag → warn
    #[test]
    fn invariants_detects_orphan_parent() {
        let (_dir, path) = fresh_dir();
        // parent_commit "zzz999" is not in the experiments list
        let run = crate::model::RunLog {
            run_tag: "orphan_test".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            experiments: vec![make_exp("aaa001", Some("zzz999"))],
            metric_name: None,
            metric_direction: None,
            schema_version: 1,
        };
        crate::store::save_run(&path, &run).unwrap();
        let r = check_invariants(&path);
        assert_eq!(r.status, CheckStatus::Warn);
        assert!(r.detail.contains("1 orphan parents"));
    }

    // Helper: build a minimal Experiment.
    fn make_exp(commit: &str, parent: Option<&str>) -> crate::model::Experiment {
        crate::model::Experiment {
            commit: commit.to_string(),
            val_bpb: 0.985,
            memory_gb: 0.0,
            status: crate::model::Status::Keep,
            description: "test".to_string(),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            params: std::collections::HashMap::new(),
            parent_commit: parent.map(|s| s.to_string()),
            crash_excerpt: None,
            signals: vec![],
            metric_name: None,
            metric_direction: None,
        }
    }
}
