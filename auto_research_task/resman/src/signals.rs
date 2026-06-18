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
    /// Training loss went NaN, or gradient overflow was detected (AMP
    /// GradScaler overflow lands here — numerically equivalent instability).
    /// Usually LR too high, numerical overflow, or a malformed input.
    NanLoss,
    /// A Python `assert` fired. `location` is the file:line when
    /// extractable, else empty.
    AssertFail { location: String },
    /// Hit a time/step budget before completing.
    Timeout,
    /// Nothing matched a known pattern. `pattern` is the last non-empty
    /// line of the tail, for forensic later.
    Unknown { pattern: String },
    /// Training loss diverged to +inf or -inf, or gradient norm hit inf.
    /// Distinct from `NanLoss` which covers NaN; this catches the "loss
    /// blew up" case where the value is infinite. `detail` is the matching
    /// log line.
    DivergedLoss { detail: String },
    /// Hardware MFU (model FLOP utilization) is below 20 %, indicating
    /// severe under-utilization (memory-bandwidth-bound or misconfigured
    /// batch). `mfu_percent` is the parsed value.
    SlowMfu { mfu_percent: f64 },
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
            Signal::DivergedLoss { .. } => "diverged_loss",
            Signal::SlowMfu { .. } => "slow_mfu",
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
    "diverged_loss",
    "slow_mfu",
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
    static RE_DIVERGED: OnceLock<Regex> = OnceLock::new();
    static RE_MFU: OnceLock<Regex> = OnceLock::new();

    // OOM: explicit out-of-memory phrasings only. "GiB total capacity" was
    // previously included here but also matches benign CUDA memory-stats lines
    // (e.g. "GPU 0: 24.00 GiB total capacity, 4.53 GiB already allocated"),
    // causing false-positive Oom signals. Removed: benign stats must fall
    // through to Unknown. PyTorch's own OutOfMemoryError class names are
    // sufficient to catch the allocator failures without the stats line.
    let re_oom = RE_OOM.get_or_init(|| {
        re(r"(?i)(CUDA out of memory\b|CUDAOutOfMemoryError|torch\.cuda\.OutOfMemoryError|out of memory while|RuntimeError:.*out of memory|NCCL error:.*out of memory)")
    });

    // CudaError: original + cuBLAS internal error + cuDNN status errors +
    // device-side assert (which always fires a CUDA assertion, not an OOM).
    let re_cuda = RE_CUDA.get_or_init(|| {
        re(r"(?i)(CUDA error[:\s]|RuntimeError:\s*CUDA|cuda runtime error|illegal memory access|cublasStatus_t|CUBLAS_STATUS_|cuDNN error:|CUDNN_STATUS_|device-side assert triggered|ncclInternalError)")
    });

    // NanLoss: original phrasings + AMP gradient overflow phrasings.
    // "Gradient overflow detected" and "GradScaler.*overflow" are numerically
    // equivalent to NaN-loss instability so they land in the same bucket.
    let re_nan = RE_NAN.get_or_init(|| {
        re(r"(?i)(loss is nan|loss\s*[:=]\s*nan|loss\s+nan|loss=.*nan|\bnan loss|returned nan|\bnan values?|detected nan|\bnan detected|found inf or nan|\bnan in (the )?grad|gradient overflow detected|gradscaler\b.*\boverflow\b)")
    });

    let re_assert = RE_ASSERT.get_or_init(|| re(r"AssertionError"));
    let re_assert_loc = RE_ASSERT_LOC.get_or_init(|| re(r#"File "([^"]+)", line (\d+)"#));

    // Timeout: original phrasings + SLURM wall-time kill + subprocess timeout +
    // POSIX SIGALRM (signal.alarm / SIGALRM).
    let re_timeout = RE_TIMEOUT.get_or_init(|| {
        re(r"(?i)(TimeoutError|wall\s*clock.*exceeded|budget exceeded|exceeded.*time limit|training time.*exceeded|DUE TO TIME LIMIT|subprocess\.TimeoutExpired|SIGALRM\b|signal\.alarm\b)")
    });

    // DivergedLoss: broadened to cover:
    //   - optional leading minus: loss=inf, loss=-inf
    //   - more key names: train_loss, lm_loss, plus the bare "loss" key
    //   - separator is = or : with optional surrounding spaces
    //   - grad-norm inf alternate: "grad norm: inf" / "clip_grad_norm ... inf"
    // NanLoss still takes priority (guard below); this regex deliberately does
    // NOT match "nan" — that is handled by RE_NAN.
    let re_diverged = RE_DIVERGED.get_or_init(|| {
        re(r"(?i)(\b(?:train_loss|lm_loss|loss)'?\s{0,2}[=:]\s{0,2}-?inf\b|(?:grad\s+norm|clip_grad_norm)[^:\n]*?:\s*inf\b)")
    });

    // SlowMfu: original "mfu_percent: N" + HuggingFace "mfu=N%" + bare "MFU: N"
    // (case-insensitive for the latter two). Captures the numeric value in group 1.
    let re_mfu =
        RE_MFU.get_or_init(|| re(r"(?i)(?:mfu_percent:\s*([\d.]+)|mfu=([\d.]+)%|MFU:\s*([\d.]+))"));

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
            .captures_iter(tail)
            .last()
            .map(|c| format!("{}:{}", &c[1], &c[2]))
            .unwrap_or_default();
        out.push(Signal::AssertFail { location });
    }
    if re_timeout.is_match(tail) {
        out.push(Signal::Timeout);
    }
    // DivergedLoss: only fire when NanLoss did NOT already match, so NaN takes priority.
    if re_diverged.is_match(tail) && !out.iter().any(|s| matches!(s, Signal::NanLoss)) {
        let detail = tail
            .lines()
            .find(|l| re_diverged.is_match(l))
            .map(|l| l.trim().to_string())
            .unwrap_or_default();
        out.push(Signal::DivergedLoss { detail });
    }
    // SlowMfu: parse the first mfu value found across all three format variants;
    // signal only if < 20.0.
    if let Some(mfu) = re_mfu
        .captures(tail)
        .and_then(|cap| {
            // Groups 1/2/3 correspond to the three alternates.
            cap.get(1)
                .or_else(|| cap.get(2))
                .or_else(|| cap.get(3))
                .and_then(|m| m.as_str().parse::<f64>().ok())
        })
        .filter(|&v| v < 20.0)
    {
        out.push(Signal::SlowMfu { mfu_percent: mfu });
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
        for t in &[
            "loss is nan",
            "loss: NaN",
            "found inf or nan in gradients",
            // Real-world signatures found via dogfooding that previously fell
            // through to Unknown: space-separated "loss nan" and PyTorch's
            // anomaly-detector "returned nan values in its Nth output".
            "iter 880/5000 | loss nan",
            "RuntimeError: Function LayerNormBackward0 returned nan values in its 0th output.",
            "nan detected in gradients",
        ] {
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
    fn assert_fail_location_is_deepest_frame() {
        // Multi-frame traceback: outer frame is train.py:10, inner (deepest) is model.py:99.
        // The fix must capture model.py:99 (last File match), not train.py:10 (first).
        let t = concat!(
            "Traceback (most recent call last):\n",
            "  File \"train.py\", line 10, in run\n",
            "    model.forward(x)\n",
            "  File \"model.py\", line 99, in forward\n",
            "    assert x.shape[-1] == self.dim\n",
            "AssertionError"
        );
        let sigs = classify(t);
        match sigs.iter().find(|s| matches!(s, Signal::AssertFail { .. })) {
            Some(Signal::AssertFail { location }) => {
                assert!(
                    location.contains("model.py"),
                    "expected deepest frame model.py, got {location}"
                );
                assert!(location.contains("99"), "expected line 99, got {location}");
                assert!(
                    !location.contains("train.py"),
                    "should not contain outer frame train.py, got {location}"
                );
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

    #[test]
    fn classify_diverged_loss_inf() {
        let t = "step 10: loss=inf";
        let sigs = classify(t);
        assert!(
            sigs.iter()
                .any(|s| matches!(s, Signal::DivergedLoss { .. })),
            "expected DivergedLoss, got {sigs:?}"
        );
        match sigs
            .iter()
            .find(|s| matches!(s, Signal::DivergedLoss { .. }))
        {
            Some(Signal::DivergedLoss { detail }) => assert!(detail.contains("loss=inf")),
            _ => panic!("expected DivergedLoss"),
        }
    }

    #[test]
    fn classify_slow_mfu_under_threshold() {
        let t = "mfu_percent: 15.3\n";
        let sigs = classify(t);
        match sigs.iter().find(|s| matches!(s, Signal::SlowMfu { .. })) {
            Some(Signal::SlowMfu { mfu_percent }) => {
                assert!((mfu_percent - 15.3).abs() < 1e-9, "mfu={mfu_percent}")
            }
            _ => panic!("expected SlowMfu, got {sigs:?}"),
        }
    }

    #[test]
    fn classify_slow_mfu_above_threshold_no_signal() {
        let t = "mfu_percent: 35.0";
        let sigs = classify(t);
        assert!(
            !sigs.iter().any(|s| matches!(s, Signal::SlowMfu { .. })),
            "unexpected SlowMfu for mfu=35.0"
        );
    }

    #[test]
    fn classify_nan_loss_still_works() {
        let t = "loss: NaN";
        let sigs = classify(t);
        assert!(
            sigs.iter().any(|s| matches!(s, Signal::NanLoss)),
            "NanLoss regression: expected NanLoss in {sigs:?}"
        );
        // NanLoss takes priority — DivergedLoss must NOT fire when NaN matched.
        assert!(
            !sigs
                .iter()
                .any(|s| matches!(s, Signal::DivergedLoss { .. })),
            "DivergedLoss should not fire when NanLoss already matched"
        );
    }

    // ── NEW TESTS ────────────────────────────────────────────────────────────

    // OOM: PyTorch allocator line without "out of memory" text.
    #[test]
    fn oom_pytorch_allocator_line_without_oom_text() {
        let t = "torch.cuda.OutOfMemoryError: Tried to allocate 4.00 GiB (GPU 0; 6.00 GiB total capacity)";
        assert!(
            classify(t).iter().any(|s| matches!(s, Signal::Oom)),
            "expected Oom from PyTorch allocator line"
        );
    }

    // Regression: benign CUDA memory-stats line must NOT classify as Oom.
    // Previously the "GiB total capacity" branch in RE_OOM fired on this.
    #[test]
    fn benign_cuda_memory_stats_not_oom() {
        let t = "GPU 0: 24.00 GiB total capacity, 4.53 GiB already allocated";
        let sigs = classify(t);
        assert!(
            !sigs.iter().any(|s| matches!(s, Signal::Oom)),
            "benign memory-stats line must NOT classify as Oom, got {sigs:?}"
        );
    }

    // Real CUDA OOM (RuntimeError phrasing) must still classify as Oom.
    #[test]
    fn real_cuda_oom_still_classifies() {
        let t = "RuntimeError: CUDA out of memory. Tried to allocate 2.00 GiB (GPU 0; 23.65 GiB total capacity; 21.03 GiB already allocated)";
        assert!(
            classify(t).iter().any(|s| matches!(s, Signal::Oom)),
            "real OOM RuntimeError must still classify as Oom"
        );
    }

    // CudaError: a *generic* NCCL internal error is a CUDA-stack/comms failure,
    // not necessarily OOM — only an explicit "NCCL error: ... out of memory"
    // counts as Oom (see oom_nccl_explicit_oom).
    #[test]
    fn nccl_internal_error_is_cuda_error() {
        let t = "ncclInternalError: ncclSystemError";
        let sigs = classify(t);
        assert!(
            sigs.iter().any(|s| matches!(s, Signal::CudaError { .. })),
            "expected CudaError from generic ncclInternalError, got {sigs:?}"
        );
        assert!(
            !sigs.iter().any(|s| matches!(s, Signal::Oom)),
            "generic ncclInternalError must NOT classify as Oom"
        );
    }

    // OOM: NCCL explicit out-of-memory message.
    #[test]
    fn oom_nccl_explicit_oom() {
        let t = "NCCL error: out of memory during all-reduce";
        assert!(
            classify(t).iter().any(|s| matches!(s, Signal::Oom)),
            "expected Oom from NCCL error: out of memory"
        );
    }

    // CudaError: cuBLAS status error.
    #[test]
    fn cuda_error_cublas_status() {
        let t = "RuntimeError: cuBLAS error: cublasStatus_t: CUBLAS_STATUS_INTERNAL_ERROR";
        let sigs = classify(t);
        assert!(
            sigs.iter().any(|s| matches!(s, Signal::CudaError { .. })),
            "expected CudaError from cuBLAS status, got {sigs:?}"
        );
        // Must not also fire OOM.
        assert!(!sigs.iter().any(|s| matches!(s, Signal::Oom)));
    }

    // CudaError: cuDNN status error.
    #[test]
    fn cuda_error_cudnn_status() {
        let t = "RuntimeError: cuDNN error: CUDNN_STATUS_EXECUTION_FAILED";
        let sigs = classify(t);
        assert!(
            sigs.iter().any(|s| matches!(s, Signal::CudaError { .. })),
            "expected CudaError from cuDNN status, got {sigs:?}"
        );
    }

    // CudaError: device-side assert.
    #[test]
    fn cuda_error_device_side_assert() {
        let t = "CUDA error: device-side assert triggered";
        let sigs = classify(t);
        assert!(
            sigs.iter().any(|s| matches!(s, Signal::CudaError { .. })),
            "expected CudaError from device-side assert, got {sigs:?}"
        );
    }

    // NanLoss: AMP gradient overflow (GradScaler variant).
    #[test]
    fn nan_loss_amp_grad_scaler_overflow() {
        let t = "GradScaler: overflow detected, skipping optimizer step";
        assert!(
            classify(t).iter().any(|s| matches!(s, Signal::NanLoss)),
            "expected NanLoss from GradScaler overflow"
        );
    }

    // NanLoss: explicit "Gradient overflow detected" phrasing.
    #[test]
    fn nan_loss_gradient_overflow_detected() {
        let t = "Gradient overflow detected in fp16 training";
        assert!(
            classify(t).iter().any(|s| matches!(s, Signal::NanLoss)),
            "expected NanLoss from gradient overflow detected"
        );
    }

    // DivergedLoss: negative-inf loss (loss: -inf).
    #[test]
    fn diverged_loss_negative_inf() {
        let t = "step 50: loss: -inf";
        let sigs = classify(t);
        assert!(
            sigs.iter()
                .any(|s| matches!(s, Signal::DivergedLoss { .. })),
            "expected DivergedLoss for loss: -inf, got {sigs:?}"
        );
    }

    // DivergedLoss: HuggingFace Trainer key name (train_loss=inf).
    #[test]
    fn diverged_loss_train_loss_key() {
        let t = "{'train_loss': inf, 'epoch': 1}";
        let sigs = classify(t);
        assert!(
            sigs.iter()
                .any(|s| matches!(s, Signal::DivergedLoss { .. })),
            "expected DivergedLoss for train_loss=inf, got {sigs:?}"
        );
    }

    // DivergedLoss: Megatron-LM lm_loss key.
    #[test]
    fn diverged_loss_lm_loss_key() {
        let t = "iteration 200 | lm_loss: inf";
        let sigs = classify(t);
        assert!(
            sigs.iter()
                .any(|s| matches!(s, Signal::DivergedLoss { .. })),
            "expected DivergedLoss for lm_loss: inf, got {sigs:?}"
        );
    }

    // DivergedLoss: grad norm inf (bare form).
    #[test]
    fn diverged_loss_grad_norm_inf() {
        let t = "grad norm: inf";
        let sigs = classify(t);
        assert!(
            sigs.iter()
                .any(|s| matches!(s, Signal::DivergedLoss { .. })),
            "expected DivergedLoss for grad norm: inf, got {sigs:?}"
        );
    }

    // DivergedLoss: clip_grad_norm inf.
    #[test]
    fn diverged_loss_clip_grad_norm_inf() {
        let t = "clip_grad_norm before clip: inf";
        let sigs = classify(t);
        assert!(
            sigs.iter()
                .any(|s| matches!(s, Signal::DivergedLoss { .. })),
            "expected DivergedLoss for clip_grad_norm inf, got {sigs:?}"
        );
    }

    // DivergedLoss must NOT fire when NanLoss already matched (priority guard).
    #[test]
    fn diverged_loss_does_not_fire_when_nan_matched() {
        // A log that matches NanLoss first; any potential inf match must be suppressed.
        let t = "loss is nan\ngrad norm: inf";
        let sigs = classify(t);
        assert!(sigs.iter().any(|s| matches!(s, Signal::NanLoss)));
        assert!(
            !sigs
                .iter()
                .any(|s| matches!(s, Signal::DivergedLoss { .. })),
            "DivergedLoss must not fire when NanLoss already matched"
        );
    }

    // SlowMfu: HuggingFace "mfu=15.3%" format.
    #[test]
    fn slow_mfu_hf_percent_format() {
        let t = "step 100 | loss 2.1 | mfu=15.3%";
        let sigs = classify(t);
        match sigs.iter().find(|s| matches!(s, Signal::SlowMfu { .. })) {
            Some(Signal::SlowMfu { mfu_percent }) => {
                assert!((mfu_percent - 15.3).abs() < 1e-9, "mfu={mfu_percent}")
            }
            _ => panic!("expected SlowMfu from mfu=15.3%, got {sigs:?}"),
        }
    }

    // SlowMfu: bare "MFU: 15.3" (case-insensitive).
    #[test]
    fn slow_mfu_bare_mfu_colon_format() {
        let t = "MFU: 15.3";
        let sigs = classify(t);
        match sigs.iter().find(|s| matches!(s, Signal::SlowMfu { .. })) {
            Some(Signal::SlowMfu { mfu_percent }) => {
                assert!((mfu_percent - 15.3).abs() < 1e-9, "mfu={mfu_percent}")
            }
            _ => panic!("expected SlowMfu from MFU: 15.3, got {sigs:?}"),
        }
    }

    // SlowMfu: "mfu=45.0%" must NOT fire (above threshold).
    #[test]
    fn slow_mfu_hf_above_threshold_no_signal() {
        let t = "step 200 | loss 1.9 | mfu=45.0%";
        assert!(
            !classify(t)
                .iter()
                .any(|s| matches!(s, Signal::SlowMfu { .. })),
            "unexpected SlowMfu for mfu=45.0%"
        );
    }

    // Timeout: SLURM wall-time kill.
    #[test]
    fn timeout_slurm_due_to_time_limit() {
        let t = "slurmstepd: error: *** JOB 12345 ON node01 CANCELLED AT 2024-01-01T00:00:00 DUE TO TIME LIMIT ***";
        assert!(
            classify(t).iter().any(|s| matches!(s, Signal::Timeout)),
            "expected Timeout from SLURM DUE TO TIME LIMIT"
        );
    }

    // Timeout: subprocess.TimeoutExpired.
    #[test]
    fn timeout_subprocess_timeout_expired() {
        let t = "subprocess.TimeoutExpired: Command '['python', 'train.py']' timed out after 300 seconds";
        assert!(
            classify(t).iter().any(|s| matches!(s, Signal::Timeout)),
            "expected Timeout from subprocess.TimeoutExpired"
        );
    }

    // Timeout: SIGALRM.
    #[test]
    fn timeout_sigalrm() {
        let t = "Killed by signal SIGALRM";
        assert!(
            classify(t).iter().any(|s| matches!(s, Signal::Timeout)),
            "expected Timeout from SIGALRM"
        );
    }

    // Negative test: normal training progress lines must yield no real signal.
    // Only Unknown (or no signals at all) is acceptable.
    #[test]
    fn negative_normal_training_log_no_real_signal() {
        let t = concat!(
            "iter 100/5000 | loss 3.21 | mfu 45.0%\n",
            "iter 200/5000 | loss 2.87 | mfu 46.1%\n",
            "saving checkpoint to checkpoints/step200.pt\n",
            "lr 0.001\n",
            "iter 300/5000 | loss 2.54 | mfu 45.8%\n",
        );
        let sigs = classify(t);
        for sig in &sigs {
            assert!(
                matches!(sig, Signal::Unknown { .. }),
                "unexpected real signal {sig:?} in normal training log"
            );
        }
    }
}
