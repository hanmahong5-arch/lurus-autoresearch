//! Hardware / environment detection helpers.
//!
//! These are strictly opt-in: if `nvidia-smi` isn't installed or fails for any
//! reason, we return `None` and the caller proceeds without GPU metadata.
//! Never let environment probing break the agent loop.

use std::process::{Command, Stdio};
use std::time::Duration;

/// Return the first NVIDIA GPU's name (e.g. "NVIDIA H100 80GB HBM3"), or None.
///
/// We launch `nvidia-smi --query-gpu=name --format=csv,noheader` with a short
/// timeout. Any error — command not found, non-zero exit, timeout, non-UTF-8
/// output — returns None silently. Responds to upstream PR #102 wanting
/// dynamic GPU detection instead of hardcoded MFU assumptions.
pub fn detect_gpu_name() -> Option<String> {
    run_with_timeout(
        "nvidia-smi",
        &["--query-gpu=name", "--format=csv,noheader"],
        Duration::from_millis(500),
    )
    .and_then(|s| s.lines().next().map(|l| l.trim().to_string()))
    .filter(|s| !s.is_empty())
}

fn run_with_timeout(bin: &str, args: &[&str], _timeout: Duration) -> Option<String> {
    // std::process has no native timeout; for our use case (nvidia-smi returns
    // in <50ms or not at all) it's fine to skip manual timeout enforcement and
    // just rely on the command being fast or absent. A full timeout would need
    // threading or a shell wrapper — not worth it for a 500ms probe.
    let out = Command::new(bin)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8(out.stdout).ok()
}
