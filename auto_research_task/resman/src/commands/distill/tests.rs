//! Tests for `resman distill` — kept in a separate file to stay under the 600-line limit.

use super::*;
use crate::model::{RunLog, Status};
use crate::signals::Signal;
use std::collections::HashMap;
use std::collections::HashSet;

fn make_exp(
    commit: &str,
    val_bpb: f64,
    status: Status,
    desc: &str,
    parent: Option<&str>,
    signals: Vec<Signal>,
) -> Experiment {
    Experiment {
        commit: commit.to_string(),
        val_bpb,
        memory_gb: 0.0,
        status,
        description: desc.to_string(),
        timestamp: String::new(),
        params: HashMap::new(),
        parent_commit: parent.map(|s| s.to_string()),
        crash_excerpt: None,
        metric_name: None,
        metric_direction: None,
        signals,
    }
}

fn make_run(tag: &str, exps: Vec<Experiment>) -> RunLog {
    RunLog {
        run_tag: tag.to_string(),
        created_at: String::new(),
        experiments: exps,
        metric_name: None,
        metric_direction: None,
    }
}

/// Test 1: empty run produces sensible zero-value report.
/// Suggestions list may be empty (no heuristics trigger on zero experiments).
#[test]
fn build_distill_empty_run() {
    let run = make_run("empty", vec![]);
    let report = build_distill(&run);
    assert_eq!(report.summary.total, 0);
    assert!(report.best.is_none());
    assert!(report.lineage.is_empty());
    // No heuristics trigger on zero experiments — suggestions may be empty.
    // Document: suggestion 3 (no parent) only fires when best exists, so
    // no suggestions expected here.
    // Just verify it doesn't panic and returns a well-formed report.
    assert_eq!(report.tag, "empty");
}

/// Test 2: signal grouping — 3 OOMs and 1 NaN are classified correctly.
#[test]
fn build_distill_groups_signals() {
    let run = make_run(
        "signals",
        vec![
            make_exp("a1", 0.0, Status::Crash, "oom1", None, vec![Signal::Oom]),
            make_exp("a2", 0.0, Status::Crash, "oom2", None, vec![Signal::Oom]),
            make_exp("a3", 0.0, Status::Crash, "oom3", None, vec![Signal::Oom]),
            make_exp("a4", 0.0, Status::Crash, "nan", None, vec![Signal::NanLoss]),
        ],
    );
    let report = build_distill(&run);
    assert_eq!(report.failure_signals.get("oom").map(|v| v.len()), Some(3));
    assert_eq!(
        report.failure_signals.get("nan_loss").map(|v| v.len()),
        Some(1)
    );
}

/// Test 3: lineage walk returns 4 entries root→best in correct order.
#[test]
fn build_distill_lineage_to_best() {
    // Chain: root (a0) → a1 → a2 → best (a3), each pointing to the previous.
    let run = make_run(
        "lineage",
        vec![
            make_exp("a0", 1.0, Status::Keep, "root", None, vec![]),
            make_exp("a1", 0.9, Status::Keep, "step1", Some("a0"), vec![]),
            make_exp("a2", 0.8, Status::Keep, "step2", Some("a1"), vec![]),
            make_exp("a3", 0.7, Status::Best, "best", Some("a2"), vec![]),
        ],
    );
    let report = build_distill(&run);
    // Lineage should be [a0, a1, a2, a3] — root to best.
    assert_eq!(report.lineage.len(), 4, "expected 4 lineage entries");
    assert_eq!(report.lineage[0].commit, "a0");
    assert_eq!(report.lineage[3].commit, "a3");
}

/// Test 4: render_markdown produces all required section headers.
#[test]
fn render_markdown_contains_sections() {
    let run = make_run(
        "test",
        vec![
            make_exp("abc1234", 0.95, Status::Keep, "baseline", None, vec![]),
            make_exp(
                "def5678",
                0.0,
                Status::Crash,
                "oom run",
                None,
                vec![Signal::Oom],
            ),
        ],
    );
    let report = build_distill(&run);
    let md = render_markdown(&report);
    assert!(md.contains("## Best result"), "missing '## Best result'");
    assert!(
        md.contains("## Failure signals"),
        "missing '## Failure signals'"
    );
    assert!(md.contains("## Suggestions"), "missing '## Suggestions'");
    assert!(
        md.contains("## Lineage to best"),
        "missing '## Lineage to best'"
    );
    assert!(
        md.contains("## Unexplored neighbors"),
        "missing '## Unexplored neighbors'"
    );
}

/// HTML Test 1: output contains title with tag name and a <style> block.
#[test]
fn render_html_contains_title_and_tag() {
    let run = make_run(
        "my-tag",
        vec![make_exp(
            "abc1234",
            0.95,
            Status::Keep,
            "baseline",
            None,
            vec![],
        )],
    );
    let report = build_distill(&run);
    let html = render_html(&report);
    assert!(
        html.contains("my-tag"),
        "tag name must appear in HTML output"
    );
    assert!(html.contains("<style>"), "must have <style> block");
    // Self-contained: no external references
    assert!(!html.contains("http://"), "must not reference http://");
    assert!(!html.contains("src=\"http"), "must not have external src");
}

/// HTML Test 2: empty run renders without best card but still produces valid HTML.
#[test]
fn render_html_empty_run_has_no_best_card_but_still_renders() {
    let run = make_run("empty-run", vec![]);
    let report = build_distill(&run);
    let html = render_html(&report);
    // Should contain the "no best" fallback text
    assert!(
        html.contains("No best experiment"),
        "should render no-best placeholder"
    );
    // Should NOT contain best-card (only rendered when best is Some)
    assert!(
        !html.contains("class=\"best-card\""),
        "best-card should not appear when no best"
    );
    // Must still be valid HTML envelope
    assert!(html.contains("<!DOCTYPE html>"));
    assert!(html.contains("</html>"));
}

/// HTML Test 3: signal sections appear with <details> when signals present.
#[test]
fn render_html_with_signals_groups_by_kind() {
    let run = make_run(
        "sigs",
        vec![
            make_exp("c1", 0.0, Status::Crash, "oom1", None, vec![Signal::Oom]),
            make_exp("c2", 0.0, Status::Crash, "oom2", None, vec![Signal::Oom]),
            make_exp("c3", 0.0, Status::Crash, "nan", None, vec![Signal::NanLoss]),
        ],
    );
    let report = build_distill(&run);
    let html = render_html(&report);
    assert!(
        html.contains("<details>"),
        "must have <details> elements for signals"
    );
    assert!(html.contains("oom"), "oom kind must appear");
    assert!(html.contains("nan_loss"), "nan_loss kind must appear");
}

/// HTML Test 4: HTML-special chars in description are escaped.
#[test]
fn render_html_escapes_html_in_description() {
    let run = make_run(
        "xss-test",
        vec![make_exp(
            "abc1234",
            0.95,
            Status::Keep,
            "<script>alert(1)</script>",
            None,
            vec![],
        )],
    );
    let report = build_distill(&run);
    let html = render_html(&report);
    assert!(
        html.contains("&lt;script&gt;"),
        "< and > must be HTML-escaped"
    );
    assert!(
        !html.contains("<script>alert"),
        "raw <script> tag must not appear"
    );
}

// -----------------------------------------------------------------------
// Wave C tests
// -----------------------------------------------------------------------

/// Wave C Test 1: When best experiment is Status::Keep (unverified),
/// suggestions must include the unverified-best prompt.
#[test]
fn suggestions_include_unverified_best_when_best_is_keep() {
    let run = make_run(
        "uvtest",
        vec![
            make_exp("aaa11111", 1.2, Status::Keep, "baseline", None, vec![]),
            make_exp("bbb22222", 0.8, Status::Best, "improved", None, vec![]),
        ],
    );
    let report = build_distill(&run);
    let has_verify_hint = report
        .suggestions
        .iter()
        .any(|s| s.contains("unverified") && s.contains("resman verify"));
    assert!(
        has_verify_hint,
        "expected unverified-best suggestion, got: {:?}",
        report.suggestions
    );
    // Must reference short commit of best
    let short = &"bbb22222"[..8];
    let has_commit = report.suggestions.iter().any(|s| s.contains(short));
    assert!(has_commit, "suggestion should contain short commit {short}");
}

/// Wave C Test 2: When ≥5 keep/best and zero verified, the bulk prompt
/// fires and the single-best prompt does NOT.
#[test]
fn suggestions_prefer_bulk_unverified_prompt_over_single_when_no_verified() {
    let exps = (0..6)
        .map(|i| {
            make_exp(
                &format!("c{i}aabbcc"),
                1.0 - i as f64 * 0.05,
                Status::Keep,
                "desc",
                None,
                vec![],
            )
        })
        .collect();
    let run = make_run("bulktest", exps);
    let report = build_distill(&run);
    let bulk = report
        .suggestions
        .iter()
        .any(|s| s.contains("No experiments have been verified yet"));
    let single = report
        .suggestions
        .iter()
        .any(|s| s.contains("Best experiment is unverified"));
    assert!(bulk, "bulk prompt must fire when ≥5 keep and 0 verified");
    assert!(!single, "single-best prompt must NOT fire when bulk fires");
}

/// Wave C Test 3: build_cross_distill aggregates signal counts across runs.
#[test]
fn build_cross_distill_aggregates_signals_across_runs() {
    let run_a = make_run(
        "run_a",
        vec![
            make_exp("a1", 0.0, Status::Crash, "oom1", None, vec![Signal::Oom]),
            make_exp("a2", 0.0, Status::Crash, "oom2", None, vec![Signal::Oom]),
        ],
    );
    let run_b = make_run(
        "run_b",
        vec![
            make_exp("b1", 0.0, Status::Crash, "oom3", None, vec![Signal::Oom]),
            make_exp(
                "b2",
                0.0,
                Status::Crash,
                "nan1",
                None,
                vec![Signal::NanLoss],
            ),
        ],
    );
    let report = build_cross_distill(&[run_a, run_b]);
    // Total oom across both runs = 3
    let oom_summary = report
        .top_failure_signals
        .iter()
        .find(|s| s.kind == "oom")
        .expect("oom must be in top signals");
    assert_eq!(
        oom_summary.count, 3,
        "expected 3 OOMs across runs, got {}",
        oom_summary.count
    );
    // nan_loss = 1
    let nan_summary = report
        .top_failure_signals
        .iter()
        .find(|s| s.kind == "nan_loss");
    assert!(nan_summary.is_some());
    assert_eq!(nan_summary.unwrap().count, 1);
    // totals
    assert_eq!(report.total_runs, 2);
    assert_eq!(report.total_experiments, 4);
    assert_eq!(report.total_crash, 4);
}

/// Wave C Test 4: build_cross_distill ranks tags by direction.
/// Minimize tag: lower is better. Maximize tag: higher is better.
#[test]
fn build_cross_distill_ranks_tags_by_direction() {
    let mut run_min = make_run(
        "min_tag",
        vec![
            make_exp("m1", 0.5, Status::Best, "best-min", None, vec![]),
            make_exp("m2", 0.9, Status::Keep, "worse", None, vec![]),
        ],
    );
    run_min.metric_direction = Some(Direction::Minimize);

    let mut run_max = make_run(
        "max_tag",
        vec![
            make_exp("x1", 0.95, Status::Best, "best-max", None, vec![]),
            make_exp("x2", 0.5, Status::Keep, "worse", None, vec![]),
        ],
    );
    run_max.metric_direction = Some(Direction::Maximize);

    let report = build_cross_distill(&[run_min, run_max]);
    // max_tag has best_value=0.95 with maximize => score=+0.95
    // min_tag has best_value=0.5 with minimize => score=-0.5
    // Highest score first: max_tag before min_tag
    assert!(!report.top_tags.is_empty());
    assert_eq!(
        report.top_tags[0].tag, "max_tag",
        "maximize tag with 0.95 should rank first"
    );
    assert_eq!(report.top_tags[1].tag, "min_tag");
}

/// Wave C Test 5: render_cross_markdown contains the Top failure signals section.
#[test]
fn render_cross_markdown_contains_top_failures_section() {
    let run = make_run(
        "sigrun",
        vec![
            make_exp("x1", 0.0, Status::Crash, "oom run", None, vec![Signal::Oom]),
            make_exp("x2", 0.8, Status::Keep, "good run", None, vec![]),
        ],
    );
    let report = build_cross_distill(&[run]);
    let md = render_cross_markdown(&report);
    assert!(
        md.contains("## Top failure signals"),
        "must have '## Top failure signals'"
    );
    assert!(
        md.contains("## Top tags by best metric"),
        "must have '## Top tags by best metric'"
    );
    assert!(md.contains("oom"), "oom must appear in cross markdown");
}

/// HTML Test 5: output contains no HTTP references and has exactly one <style> block.
#[test]
fn render_html_no_external_refs() {
    let run = make_run(
        "netcheck",
        vec![make_exp("a1", 0.9, Status::Best, "best one", None, vec![])],
    );
    let report = build_distill(&run);
    let html = render_html(&report);
    assert!(!html.contains("http://"));
    assert!(!html.contains("https://"));
    // Count <style> occurrences — must be exactly 1
    let style_count = html.matches("<style>").count();
    assert_eq!(style_count, 1, "expected exactly 1 <style> block");
    // Must contain tag in output
    let tag_count = html.matches("netcheck").count();
    assert!(tag_count >= 1, "tag must appear at least once");
    // All badge classes referenced from CSS
    let badge_classes: HashSet<&str> = [
        "badge-keep",
        "badge-best",
        "badge-crash",
        "badge-discard",
        "badge-verified",
        "badge-neutral",
    ]
    .iter()
    .copied()
    .collect();
    for cls in &badge_classes {
        assert!(html.contains(cls), "CSS must define {cls}");
    }
}
