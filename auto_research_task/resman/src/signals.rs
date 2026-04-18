//! Typed classification of training-log crash tails into actionable signals.
//!
//! Before v0.6 the `crash_excerpt` field held raw log text — useful as
//! evidence but not queryable. This module converts that tail into a
//! structured `Vec<Signal>` so agents can ask "how many OOMs did we get
//! overnight?" without regex-gymnastics in a shell script.
//!
//! Classification is regex-based, order-matters (OOM before CudaError so a
//! CUDA OOM doesn't double-count), and always returns at least one entry.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Signal {
    /// CUDA out-of-memory. Most common failure mode for
    /// under-memoried GPUs pushing batch size.
    Oom,
    /// CUDA runtime error other than OOM (e.g. illegal memory access,
    /// driver mismatch). `hint` is a short snippet from the matched line.
    CudaError { hint: String },
    /// Training loss went NaN. Usually LR too high, numerical overflow,
    /// or a malformed input.
    NanLoss,
    /// A Python `assert` fired. `location` is the file:line when
    /// extractable, else empty.
    AssertFail { location: String },
    /// Hit a time/step budget before completing.
    Timeout,
    /// Nothing matched a known pattern. `pattern` is the last non-empty
    /// line of the tail, for forensic later.
    Unknown { pattern: String },
}

impl Signal {
    /// Discriminant name used by CLI filters and the MCP tool's enum:
    /// "oom", "cuda_error", "nan_loss", "assert_fail", "timeout", "unknown".
    pub fn kind(&self) -> &'static str {
        match self {
            Signal::Oom => "oom",
            Signal::CudaError { .. } => "cuda_error",
            Signal::NanLoss => "nan_loss",
            Signal::AssertFail { .. } => "assert_fail",
            Signal::Timeout => "timeout",
            Signal::Unknown { .. } => "unknown",
        }
    }
}

pub const ALL_KINDS: &[&str] = &[
    "oom",
    "cuda_error",
    "nan_loss",
    "assert_fail",
    "timeout",
    "unknown",
];

/// Classify a log tail (typically the last 50 lines from `logtail::tail_lines`)
/// into one or more `Signal`s. Always returns at least one entry; if nothing
/// known matched, returns a single `Signal::Unknown` with the last non-empty
/// line of the tail as its pattern.
pub fn classify(tail: &str) -> Vec<Signal> {
    fn re(pat: &str) -> Regex {
        Regex::new(pat).expect("hardcoded regex")
    }
    static RE_OOM: OnceLock<Regex> = OnceLock::new();
    static RE_CUDA: OnceLock<Regex> = OnceLock::new();
    static RE_NAN: OnceLock<Regex> = OnceLock::new();
    static RE_ASSERT: OnceLock<Regex> = OnceLock::new();
    static RE_ASSERT_LOC: OnceLock<Regex> = OnceLock::new();
    static RE_TIMEOUT: OnceLock<Regex> = OnceLock::new();

    let re_oom = RE_OOM.get_or_init(|| {
        re(r"(?i)(CUDA out of memory|CUDAOutOfMemoryError|torch\.cuda\.OutOfMemoryError|out of memory while|RuntimeError:.*out of memory)")
    });
    let re_cuda = RE_CUDA.get_or_init(|| {
        re(r"(?i)(CUDA error[:\s]|RuntimeError:\s*CUDA|cuda runtime error|illegal memory access)")
    });
    let re_nan = RE_NAN.get_or_init(|| {
        re(r"(?i)(loss is nan|loss:\s*nan|nan loss|detected nan|found inf or nan|loss=.*nan)")
    });
    let re_assert = RE_ASSERT.get_or_init(|| re(r"AssertionError"));
    let re_assert_loc = RE_ASSERT_LOC.get_or_init(|| re(r#"File "([^"]+)", line (\d+)"#));
    let re_timeout = RE_TIMEOUT.get_or_init(|| {
        re(r"(?i)(TimeoutError|wall\s*clock.*exceeded|budget exceeded|exceeded.*time limit|training time.*exceeded)")
    });

    let mut out = Vec::new();

    if re_oom.is_match(tail) {
        out.push(Signal::Oom);
    }
    if re_cuda.is_match(tail) && !out.iter().any(|s| matches!(s, Signal::Oom)) {
        // Extract the matching line as a hint, truncated to ~80 chars.
        let hint = tail
            .lines()
            .find(|l| re_cuda.is_match(l))
            .map(|l| crate::store::truncate(l.trim(), 80))
            .unwrap_or_default();
        out.push(Signal::CudaError { hint });
    }
    if re_nan.is_match(tail) {
        out.push(Signal::NanLoss);
    }
    if re_assert.is_match(tail) {
        let location = re_assert_loc
            .captures(tail)
            .map(|c| format!("{}:{}", &c[1], &c[2]))
            .unwrap_or_default();
        out.push(Signal::AssertFail { location });
    }
    if re_timeout.is_match(tail) {
        out.push(Signal::Timeout);
    }

    if out.is_empty() {
        let pattern = tail
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .map(|l| crate::store::truncate(l.trim(), 120))
            .unwrap_or_default();
        out.push(Signal::Unknown { pattern });
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_plain_oom() {
        let t = "RuntimeError: CUDA out of memory. Tried to allocate 4.00 GiB";
        let sigs = classify(t);
        assert!(sigs.iter().any(|s| matches!(s, Signal::Oom)));
    }

    #[test]
    fn cuda_error_suppressed_when_oom_present() {
        // "CUDA out of memory" matches both OOM and the CUDA error regex;
        // we only want the OOM since it's more specific.
        let t = "RuntimeError: CUDA out of memory.";
        let sigs = classify(t);
        assert_eq!(
            sigs.iter()
                .filter(|s| matches!(s, Signal::CudaError { .. }))
                .count(),
            0
        );
        assert_eq!(sigs.iter().filter(|s| matches!(s, Signal::Oom)).count(), 1);
    }

    #[test]
    fn detects_cuda_error_with_hint() {
        let t = "step 420: CUDA error: an illegal memory access was encountered";
        let sigs = classify(t);
        match sigs.iter().find(|s| matches!(s, Signal::CudaError { .. })) {
            Some(Signal::CudaError { hint }) => assert!(hint.contains("illegal memory access")),
            _ => panic!("expected CudaError"),
        }
    }

    #[test]
    fn detects_nan_loss() {
        for t in &["loss is nan", "loss: NaN", "found inf or nan in gradients"] {
            assert!(
                classify(t).iter().any(|s| matches!(s, Signal::NanLoss)),
                "failed on {t}"
            );
        }
    }

    #[test]
    fn detects_assert_fail_with_location() {
        let t = "Traceback (most recent call last):\n  File \"train.py\", line 42, in forward\n    assert x.shape[0] == batch_size\nAssertionError";
        let sigs = classify(t);
        match sigs.iter().find(|s| matches!(s, Signal::AssertFail { .. })) {
            Some(Signal::AssertFail { location }) => {
                assert!(location.contains("train.py"), "got location={location}");
                assert!(location.contains("42"));
            }
            _ => panic!("expected AssertFail"),
        }
    }

    #[test]
    fn detects_timeout() {
        let t = "TimeoutError: wall clock budget exceeded after 300s";
        assert!(classify(t).iter().any(|s| matches!(s, Signal::Timeout)));
    }

    #[test]
    fn unknown_fallback_captures_last_line() {
        let t = "step 1: loss=4.2\nstep 2: loss=3.8\nSegmentation fault (core dumped)\n";
        let sigs = classify(t);
        assert_eq!(sigs.len(), 1);
        match &sigs[0] {
            Signal::Unknown { pattern } => assert!(pattern.contains("Segmentation")),
            other => panic!("expected Unknown, got {other:?}"),
        }
    }

    #[test]
    fn kind_roundtrips_through_serde() {
        let original = vec![
            Signal::Oom,
            Signal::CudaError {
                hint: "illegal access".into(),
            },
            Signal::NanLoss,
            Signal::AssertFail {
                location: "train.py:42".into(),
            },
            Signal::Timeout,
            Signal::Unknown {
                pattern: "core dumped".into(),
            },
        ];
        let json = serde_json::to_string(&original).unwrap();
        assert!(json.contains(r#""type":"oom""#));
        assert!(json.contains(r#""type":"cuda_error""#));
        let parsed: Vec<Signal> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, original);
    }
}
