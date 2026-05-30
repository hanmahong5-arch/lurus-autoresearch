//! Signal/failure clustering for distill reports.

use std::collections::HashMap;

use crate::model::RunLog;
use crate::store::truncate;

use super::FailureSignalEntry;

pub(super) fn signal_detail(sig: &crate::signals::Signal) -> String {
    use crate::signals::Signal::*;
    match sig {
        CudaError { hint } if !hint.is_empty() => format!("hint: {hint}"),
        AssertFail { location } if !location.is_empty() => format!("at {location}"),
        Unknown { pattern } if !pattern.is_empty() => format!("matched: {pattern}"),
        DivergedLoss { detail } if !detail.is_empty() => format!("diverged: {detail}"),
        SlowMfu { mfu_percent } => format!("{mfu_percent:.1}% MFU"),
        _ => String::new(),
    }
}

pub(super) fn build_failure_signals(run: &RunLog) -> HashMap<String, Vec<FailureSignalEntry>> {
    let mut failure_signals: HashMap<String, Vec<FailureSignalEntry>> = HashMap::new();
    for kind in crate::signals::ALL_KINDS {
        failure_signals.insert(kind.to_string(), vec![]);
    }
    for e in &run.experiments {
        for sig in &e.signals {
            let kind = sig.kind();
            let detail = signal_detail(sig);
            let entry = FailureSignalEntry {
                commit: e.commit.clone(),
                description: truncate(&e.description, 60),
                detail,
            };
            failure_signals
                .entry(kind.to_string())
                .or_default()
                .push(entry);
        }
    }
    failure_signals
}
