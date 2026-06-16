/// Integration tests locking down 6 byte-identical public contracts of the resman CLI.
///
/// Each test sets RESMAN_HOME to a fresh tempdir and exercises the binary via assert_cmd.
use assert_cmd::Command;
use std::path::Path;
use tempfile::TempDir;

/// Convenience: build a Command with RESMAN_HOME already set.
fn resman(home: &Path) -> Command {
    let mut cmd = Command::cargo_bin("resman").expect("binary must exist");
    cmd.env("RESMAN_HOME", home);
    cmd
}

fn sample_tsv() -> &'static str {
    concat!(env!("CARGO_MANIFEST_DIR"), "/examples/sample-results.tsv")
}

/// Helper: init + import sample data into the given home dir.
fn init_and_import(home: &Path) {
    resman(home).arg("init").assert().success();
    resman(home)
        .args(["import", sample_tsv(), "-t", "smoke"])
        .assert()
        .success();
}

// ── Test 1 ───────────────────────────────────────────────────────────────────
/// `best -f value` must output a pure f64 float — no prefix, no ANSI escapes, no extra lines.
#[test]
fn best_value_format_is_pure_float() {
    let home = TempDir::new().unwrap();
    init_and_import(home.path());

    let output = resman(home.path())
        .args(["best", "-f", "value"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let text = String::from_utf8(output).unwrap();
    let trimmed = text.trim();

    // Must not contain ANSI escape sequences.
    assert!(
        !trimmed.contains('\x1b'),
        "stdout contained ANSI escape: {:?}",
        trimmed
    );

    // Must be exactly one line (no embedded newlines after trim).
    assert!(
        !trimmed.contains('\n'),
        "stdout had extra lines: {:?}",
        trimmed
    );

    // Must parse as f64.
    trimmed
        .parse::<f64>()
        .unwrap_or_else(|_| panic!("`best -f value` output {:?} is not a valid f64", trimmed));

    // Must be the exact `{:.6}` byte format (six decimals) — the public
    // shell-script API contract. Lock it so a format change can't slip through.
    let dot = trimmed
        .find('.')
        .expect("`best -f value` must contain a decimal point");
    assert_eq!(
        trimmed.len() - dot - 1,
        6,
        "`best -f value` must print exactly six decimals: {trimmed:?}"
    );
}

// ── Test 2 ───────────────────────────────────────────────────────────────────
/// `best -f value` on an empty store must exit non-zero and not emit a parseable float.
#[test]
fn best_value_on_empty_store_fails_cleanly() {
    let home = TempDir::new().unwrap();
    resman(home.path()).arg("init").assert().success();

    let output = resman(home.path())
        .args(["best", "-f", "value"])
        .assert()
        .failure() // exit code != 0
        .get_output()
        .stdout
        .clone();

    let text = String::from_utf8(output).unwrap();
    let trimmed = text.trim();

    // Stdout must NOT be a parseable float (scripts must not silently consume garbage).
    assert!(
        trimmed.parse::<f64>().is_err(),
        "stdout should not be a valid float on empty store, got: {:?}",
        trimmed
    );
}

// ── Test 3 ───────────────────────────────────────────────────────────────────
/// `import` is idempotent when `--force` is supplied — second import must exit 0 without error.
#[test]
fn import_is_idempotent_under_force() {
    let home = TempDir::new().unwrap();
    resman(home.path()).arg("init").assert().success();

    // First import.
    resman(home.path())
        .args(["import", sample_tsv(), "-t", "t1"])
        .assert()
        .success();

    // Second import with --force must also succeed.
    resman(home.path())
        .args(["import", sample_tsv(), "-t", "t1", "--force"])
        .assert()
        .success();
}

// ── Test 4 ───────────────────────────────────────────────────────────────────
/// `list -o json` must emit a valid JSON array with at least one element.
#[test]
fn list_json_is_valid_array() {
    let home = TempDir::new().unwrap();
    init_and_import(home.path());

    let output = resman(home.path())
        .args(["list", "-o", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let text = String::from_utf8(output).unwrap();
    let value: serde_json::Value =
        serde_json::from_str(text.trim()).expect("list -o json must be valid JSON");

    let arr = value.as_array().expect("list -o json must be a JSON array");
    assert!(!arr.is_empty(), "JSON array must have at least one element");
}

// ── Test 5 ───────────────────────────────────────────────────────────────────
/// `list -o tsv` first line must be a stable tab-separated header containing `commit` and `val_bpb`.
#[test]
fn list_tsv_has_stable_header() {
    let home = TempDir::new().unwrap();
    init_and_import(home.path());

    let output = resman(home.path())
        .args(["list", "-o", "tsv"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let text = String::from_utf8(output).unwrap();
    let first_line = text
        .lines()
        .next()
        .expect("TSV output must have at least one line");

    // Lock the exact header.
    assert_eq!(
        first_line, "commit\tval_bpb\tmemory_gb\tstatus\tdescription",
        "TSV header changed — public contract broken"
    );
}

// ── Test 6 ───────────────────────────────────────────────────────────────────
/// `resman mcp` must respond to an `initialize` JSON-RPC 2.0 request with a valid envelope.
#[test]
fn mcp_initialize_handshake() {
    let home = TempDir::new().unwrap();
    resman(home.path()).arg("init").assert().success();

    let request = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;

    let output = resman(home.path())
        .arg("mcp")
        .write_stdin(format!("{}\n", request))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let text = String::from_utf8(output).unwrap();

    // Find the first non-empty line that parses as JSON.
    let response: serde_json::Value = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .find_map(|l| serde_json::from_str(l).ok())
        .expect("mcp stdout must contain at least one valid JSON line");

    assert_eq!(
        response["jsonrpc"].as_str(),
        Some("2.0"),
        "jsonrpc field must be \"2.0\""
    );
    assert_eq!(response["id"].as_i64(), Some(1), "id must echo back 1");
    assert!(
        response.get("result").is_some() || response.get("error").is_some(),
        "response must have either a `result` or `error` field"
    );
}

// ── Test 7 ───────────────────────────────────────────────────────────────────
/// `resman add` writes a usage.jsonl line with tool=resman_add_experiment and correct tag.
#[test]
fn add_writes_usage_jsonl() {
    let home = TempDir::new().unwrap();
    resman(home.path()).arg("init").assert().success();

    resman(home.path())
        .args([
            "add", "-t", "T", "-c", "abc1234", "-v", "1.0", "-s", "keep", "-d", "test run",
        ])
        .assert()
        .success();

    let usage_path = home.path().join("usage.jsonl");
    assert!(
        usage_path.exists(),
        "usage.jsonl must exist after resman add"
    );

    let contents = std::fs::read_to_string(&usage_path).unwrap();
    let event: serde_json::Value = contents
        .lines()
        .find_map(|l| serde_json::from_str(l).ok())
        .expect("usage.jsonl must contain at least one valid JSON line");

    assert_eq!(
        event["tool"].as_str(),
        Some("resman_add_experiment"),
        "tool must be resman_add_experiment"
    );
    assert_eq!(event["args"]["tag"].as_str(), Some("T"), "tag must be T");
}

// ── Test 8 ───────────────────────────────────────────────────────────────────
/// `resman best -f value` does NOT append any line to usage.jsonl.
#[test]
fn best_does_not_write_usage_jsonl() {
    let home = TempDir::new().unwrap();
    init_and_import(home.path());

    let usage_path = home.path().join("usage.jsonl");

    // Count lines before (import may have written one).
    let count_before = if usage_path.exists() {
        std::fs::read_to_string(&usage_path)
            .unwrap_or_default()
            .lines()
            .filter(|l| !l.trim().is_empty())
            .count()
    } else {
        0
    };

    resman(home.path())
        .args(["best", "-f", "value"])
        .assert()
        .success();

    let count_after = if usage_path.exists() {
        std::fs::read_to_string(&usage_path)
            .unwrap_or_default()
            .lines()
            .filter(|l| !l.trim().is_empty())
            .count()
    } else {
        0
    };

    assert_eq!(
        count_before, count_after,
        "`resman best` must not append to usage.jsonl (before={count_before}, after={count_after})"
    );
}

// ── Test 9 ───────────────────────────────────────────────────────────────────
/// With RESMAN_DISABLE_USAGE_LOG=1, `resman add` writes nothing to usage.jsonl.
#[test]
fn add_respects_disable_usage_log_env() {
    let home = TempDir::new().unwrap();
    resman(home.path()).arg("init").assert().success();

    let mut cmd = Command::cargo_bin("resman").expect("binary must exist");
    cmd.env("RESMAN_HOME", home.path())
        .env("RESMAN_DISABLE_USAGE_LOG", "1")
        .args([
            "add", "-t", "T", "-c", "abc1234", "-v", "1.0", "-s", "keep", "-d", "test run",
        ])
        .assert()
        .success();

    let usage_path = home.path().join("usage.jsonl");
    assert!(
        !usage_path.exists(),
        "usage.jsonl must NOT be created when RESMAN_DISABLE_USAGE_LOG=1"
    );
}
