use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Keep,
    Discard,
    Crash,
    Best,
    Verified,
}

impl Status {
    pub fn is_kept(self) -> bool {
        matches!(self, Status::Keep | Status::Best | Status::Verified)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Status::Keep => "keep",
            Status::Discard => "discard",
            Status::Crash => "crash",
            Status::Best => "best",
            Status::Verified => "verified",
        }
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Status {
    type Err = crate::error::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "keep" | "k" => Ok(Status::Keep),
            "discard" | "d" | "drop" | "revert" => Ok(Status::Discard),
            "crash" | "c" | "fail" | "oom" => Ok(Status::Crash),
            "best" | "b" => Ok(Status::Best),
            "verified" | "v" => Ok(Status::Verified),
            other => Err(crate::error::Error::InvalidStatus(other.to_string())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Minimize,
    Maximize,
}

impl Direction {
    pub fn as_str(self) -> &'static str {
        match self {
            Direction::Minimize => "minimize",
            Direction::Maximize => "maximize",
        }
    }
}

impl FromStr for Direction {
    type Err = crate::error::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "min" | "minimize" | "lower" => Ok(Direction::Minimize),
            "max" | "maximize" | "higher" => Ok(Direction::Maximize),
            other => Err(crate::error::Error::InvalidDirection(other.to_string())),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Experiment {
    pub commit: String,
    pub val_bpb: f64,
    pub memory_gb: f64,
    pub status: Status,
    pub description: String,
    #[serde(default)]
    pub timestamp: String,
    #[serde(default)]
    pub params: HashMap<String, String>,

    // --- fields added in v0.3 (all backwards-compat via serde default) ---
    /// Parent commit this experiment was branched from. Enables lineage/tree.
    /// Responds to upstream PR #472-style demand for an experiment graph.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_commit: Option<String>,

    /// Short excerpt from `run.log` captured at crash time — usually the last
    /// ~50 lines of the training log. Responds to upstream PR #101 / bd75534.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crash_excerpt: Option<String>,

    // --- fields added in v0.5 ---
    /// Per-experiment metric name override. When None, falls back to run-level,
    /// then to "val_bpb". Added v0.5 to generalize beyond karpathy nanoGPT.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metric_name: Option<String>,
    /// Per-experiment direction override (minimize|maximize). When None, falls
    /// back to run-level, then to Minimize.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metric_direction: Option<Direction>,

    // --- fields added in v0.6 ---
    /// Typed classification of the crash tail. When empty, no patterns matched
    /// or `--log` wasn't provided. Populated by `signals::classify()`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub signals: Vec<crate::signals::Signal>,
}

impl Experiment {
    /// Returns the effective metric name: experiment override > run default > "val_bpb".
    pub fn effective_metric_name<'a>(&'a self, run: &'a RunLog) -> &'a str {
        self.metric_name
            .as_deref()
            .or(run.metric_name.as_deref())
            .unwrap_or("val_bpb")
    }
    /// Returns the effective direction: experiment override > run default > Minimize.
    pub fn effective_direction(&self, run: &RunLog) -> Direction {
        self.metric_direction
            .or(run.metric_direction)
            .unwrap_or(Direction::Minimize)
    }
}

/// Returns the canonical schema version for newly-written RunLogs.
///
/// Bump only on a v1.0-incompatible change. Older stores omit this field;
/// `#[serde(default)]` backfills it to 1, which is the v0.8 contract.
pub fn default_schema_version() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunLog {
    pub experiments: Vec<Experiment>,
    pub run_tag: String,
    pub created_at: String,

    // --- fields added in v0.5 ---
    /// Run-level default metric name. Set on `resman add`/`import` when a
    /// new run is created, or via a future migrate command.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metric_name: Option<String>,
    /// Run-level default direction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metric_direction: Option<Direction>,

    // --- fields added in v0.8 ---
    /// On-disk schema version. v0.1-v0.7 stores omit it and load with the
    /// serde default of 1. Bump only on incompatible changes. v1.0 reading
    /// a higher version SHOULD silently drop unknown fields (we do not set
    /// `deny_unknown_fields`); reading a record with a missing field MUST
    /// fall back to type defaults. See docs/SCHEMA.md.
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
}

impl RunLog {
    pub fn best(&self) -> Option<&Experiment> {
        let direction = self
            .metric_direction
            .or_else(|| self.experiments.first().and_then(|e| e.metric_direction))
            .unwrap_or(Direction::Minimize);

        let candidates: Vec<&Experiment> = self
            .experiments
            .iter()
            .filter(|e| e.status.is_kept())
            // Exclude non-finite metrics (NaN/±inf): a NaN compares as Equal under
            // partial_cmp and could otherwise be selected as "best". Minimize's
            // >0.0 filter already drops NaN/-inf, but this also covers +inf and
            // protects Maximize, which has no value filter.
            .filter(|e| e.val_bpb.is_finite())
            // Preserve v0.4 behavior: when minimizing, also reject val_bpb <= 0
            // as a safety for "crash that wasn't marked crash". For Maximize,
            // 0 is a legitimate value (e.g. accuracy=0), so no filter.
            .filter(|e| match direction {
                Direction::Minimize => e.val_bpb > 0.0,
                Direction::Maximize => true,
            })
            .collect();

        candidates.into_iter().min_by(|a, b| {
            let cmp = a
                .val_bpb
                .partial_cmp(&b.val_bpb)
                .unwrap_or(std::cmp::Ordering::Equal);
            match direction {
                Direction::Minimize => cmp,
                Direction::Maximize => cmp.reverse(),
            }
        })
    }

    pub fn kept(&self) -> impl Iterator<Item = &Experiment> {
        self.experiments.iter().filter(|e| e.status.is_kept())
    }
}

#[cfg(test)]
mod model_tests {
    use super::*;

    fn make_run(dir: Option<Direction>, name: Option<&str>) -> RunLog {
        RunLog {
            experiments: vec![],
            run_tag: "test".into(),
            created_at: String::new(),
            metric_name: name.map(str::to_string),
            metric_direction: dir,
            schema_version: 1,
        }
    }

    fn make_exp(
        dir: Option<Direction>,
        name: Option<&str>,
        val: f64,
        status: Status,
    ) -> Experiment {
        Experiment {
            commit: "abc".into(),
            val_bpb: val,
            memory_gb: 0.0,
            status,
            description: String::new(),
            timestamp: String::new(),
            params: HashMap::new(),
            parent_commit: None,
            crash_excerpt: None,
            metric_name: name.map(str::to_string),
            metric_direction: dir,
            signals: Vec::new(),
        }
    }

    #[test]
    fn effective_name_cascades_experiment_to_run_to_default() {
        let run_no_name = make_run(None, None);
        let run_with_name = make_run(None, Some("eval_loss"));

        let exp_no_name = make_exp(None, None, 0.5, Status::Keep);
        let exp_with_name = make_exp(None, Some("rouge_l"), 0.5, Status::Keep);

        // Experiment override wins
        assert_eq!(
            exp_with_name.effective_metric_name(&run_with_name),
            "rouge_l"
        );
        // Run-level wins when experiment has none
        assert_eq!(
            exp_no_name.effective_metric_name(&run_with_name),
            "eval_loss"
        );
        // Default when both are None
        assert_eq!(exp_no_name.effective_metric_name(&run_no_name), "val_bpb");
    }

    #[test]
    fn effective_direction_cascades_similarly() {
        let run_max = make_run(Some(Direction::Maximize), None);
        let run_none = make_run(None, None);

        let exp_min = make_exp(Some(Direction::Minimize), None, 0.5, Status::Keep);
        let exp_none = make_exp(None, None, 0.5, Status::Keep);

        // Experiment override wins
        assert_eq!(exp_min.effective_direction(&run_max), Direction::Minimize);
        // Run-level wins when experiment has none
        assert_eq!(exp_none.effective_direction(&run_max), Direction::Maximize);
        // Default Minimize when both are None
        assert_eq!(exp_none.effective_direction(&run_none), Direction::Minimize);
    }

    #[test]
    fn best_excludes_non_finite_metrics() {
        // Maximize has no >0.0 filter, so a NaN/inf could otherwise be selected
        // as "best". best() must skip non-finite values and pick the finite max.
        let mut run = make_run(Some(Direction::Maximize), None);
        run.experiments = vec![
            make_exp(None, None, 0.80, Status::Keep),
            make_exp(None, None, f64::NAN, Status::Keep),
            make_exp(None, None, 0.95, Status::Keep), // the true finite maximum
            make_exp(None, None, f64::INFINITY, Status::Keep),
        ];
        let best = run.best().expect("a finite best exists");
        assert_eq!(
            best.val_bpb, 0.95,
            "best() must skip NaN/inf and pick the finite maximum"
        );
    }

    #[test]
    fn best_respects_maximize() {
        let mut run = make_run(Some(Direction::Maximize), None);
        run.experiments = vec![
            make_exp(None, None, 0.8, Status::Keep),
            make_exp(None, None, 0.9, Status::Keep),
            make_exp(None, None, 0.7, Status::Keep),
        ];
        let best = run.best().expect("should have best");
        assert!((best.val_bpb - 0.9).abs() < f64::EPSILON);
    }

    #[test]
    fn best_default_still_minimize() {
        let mut run = make_run(None, None);
        run.experiments = vec![
            make_exp(None, None, 0.98, Status::Keep),
            make_exp(None, None, 0.99, Status::Keep),
            make_exp(None, None, 0.97, Status::Keep),
        ];
        let best = run.best().expect("should have best");
        assert!((best.val_bpb - 0.97).abs() < f64::EPSILON);
    }

    #[test]
    fn direction_from_str_accepts_aliases() {
        assert_eq!("min".parse::<Direction>().unwrap(), Direction::Minimize);
        assert_eq!(
            "minimize".parse::<Direction>().unwrap(),
            Direction::Minimize
        );
        assert_eq!("lower".parse::<Direction>().unwrap(), Direction::Minimize);
        assert_eq!("max".parse::<Direction>().unwrap(), Direction::Maximize);
        assert_eq!(
            "maximize".parse::<Direction>().unwrap(),
            Direction::Maximize
        );
        assert_eq!("higher".parse::<Direction>().unwrap(), Direction::Maximize);
        assert!("bad".parse::<Direction>().is_err());
    }

    /// Guard the wire-format invariant: every Status variant must serialize to
    /// its exact lowercase string. A serde rename or variant rename would silently
    /// corrupt existing stores; this test catches that at compile+test time.
    #[test]
    fn status_serde_wire_format_all_variants() {
        let cases = [
            (Status::Keep, "\"keep\""),
            (Status::Discard, "\"discard\""),
            (Status::Crash, "\"crash\""),
            (Status::Best, "\"best\""),
            (Status::Verified, "\"verified\""),
        ];
        for (variant, expected_json) in cases {
            let serialized = serde_json::to_string(&variant).unwrap();
            assert_eq!(
                serialized, expected_json,
                "Status::{variant:?} must serialize to {expected_json}, got {serialized}"
            );
            let roundtrip: Status = serde_json::from_str(&serialized).unwrap();
            assert_eq!(
                roundtrip, variant,
                "Status::{variant:?} must deserialize back from {serialized}"
            );
        }
    }

    #[test]
    fn direction_serde_lowercase() {
        let s = serde_json::to_string(&Direction::Minimize).unwrap();
        assert_eq!(s, "\"minimize\"");
        let d: Direction = serde_json::from_str("\"minimize\"").unwrap();
        assert_eq!(d, Direction::Minimize);
        let s2 = serde_json::to_string(&Direction::Maximize).unwrap();
        assert_eq!(s2, "\"maximize\"");
        let d2: Direction = serde_json::from_str("\"maximize\"").unwrap();
        assert_eq!(d2, Direction::Maximize);
    }
}
