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
        schema_version: 1,
    }
}

/// Test 1: empty run produces sensible zero-value report.
/// Suggestions list may be empty (no heuristics trigger on zero experiments).
#[test]
fn build_distill_empty_run() {
    let run = make_run("empty", vec![]);
    let report = build_distill(&run, None);
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
    let report = build_distill(&run, None);
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
    let report = build_distill(&run, None);
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
    let report = build_distill(&run, None);
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
    let report = build_distill(&run, None);
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
    let report = build_distill(&run, None);
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
    let report = build_distill(&run, None);
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
    let report = build_distill(&run, None);
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
    let report = build_distill(&run, None);
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
    let report = build_distill(&run, None);
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
    let report = build_distill(&run, None);
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

/// Stagnation suggestion: 10+ experiments, last >=8 kept ones without improvement.
#[test]
fn suggestions_include_stagnation_when_no_improvement_in_8_runs() {
    // Build 11 keep experiments: improvement at t=0 (0.99), then 10 stalled at 0.99..
    // (each strictly equal-or-worse so rolling best does not advance).
    let mut exps = Vec::new();
    // Anchor improvement.
    let mut e0 = make_exp("anchor0", 0.99, Status::Keep, "baseline", None, vec![]);
    e0.timestamp = "2026-05-01T00:00:00Z".to_string();
    exps.push(e0);
    // 10 stale follow-ups, val_bpb >= 0.99 so they never beat the anchor.
    for i in 1..=10 {
        let mut e = make_exp(
            &format!("stale{i:03}"),
            0.99 + (i as f64) * 0.0001,
            Status::Keep,
            &format!("tweak {i}"),
            Some("anchor0"),
            vec![],
        );
        e.timestamp = format!("2026-05-{:02}T00:00:00Z", i + 1);
        exps.push(e);
    }
    let run = make_run("stalemate", exps);
    let report = build_distill(&run, None);

    let has_stagnation = report
        .suggestions
        .iter()
        .any(|s| s.contains("stagnant") && s.contains("anchor0"));
    assert!(
        has_stagnation,
        "expected stagnation suggestion pointing at the last improvement; got: {:?}",
        report.suggestions
    );
}

/// Stagnation does NOT fire when there are fewer than 10 experiments.
#[test]
fn suggestions_no_stagnation_under_threshold() {
    let exps: Vec<Experiment> = (0..5)
        .map(|i| {
            let mut e = make_exp(
                &format!("c{i}"),
                0.99 + (i as f64) * 0.001,
                Status::Keep,
                "tweak",
                None,
                vec![],
            );
            e.timestamp = format!("2026-05-{:02}T00:00:00Z", i + 1);
            e
        })
        .collect();
    let run = make_run("small", exps);
    let report = build_distill(&run, None);
    assert!(
        !report.suggestions.iter().any(|s| s.contains("stagnant")),
        "should not fire stagnation under threshold; got: {:?}",
        report.suggestions
    );
}

/// Keep-but-reverted: a `keep` experiment is the lineage ancestor of a strictly-
/// better `verified` descendant on the same chain.
#[test]
fn suggestions_include_keep_but_reverted_when_verified_surpasses_keep_ancestor() {
    // Chain: keep_root (0.99) → keep_mid (0.98) → verified_top (0.96).
    // verified_top is strictly better (minimize) than keep_mid AND keep_root.
    // Expect one keep-but-reverted suggestion mentioning the kept commit.
    let exps = vec![
        make_exp("keepRoot", 0.99, Status::Keep, "baseline", None, vec![]),
        make_exp(
            "keepMid",
            0.98,
            Status::Keep,
            "tried novel idea",
            Some("keepRoot"),
            vec![],
        ),
        make_exp(
            "veriTop",
            0.96,
            Status::Verified,
            "verified breakthrough",
            Some("keepMid"),
            vec![],
        ),
    ];
    let run = make_run("ladder", exps);
    let report = build_distill(&run, None);

    let has_under_explored = report.suggestions.iter().any(|s| {
        s.contains("Under-explored")
            && s.contains("keepMid")
            && (s.contains("veriTop") || s.contains("verified"))
    });
    assert!(
        has_under_explored,
        "expected keep-but-reverted suggestion linking keepMid to veriTop; got: {:?}",
        report.suggestions
    );
}

/// Branch verdict: non-best branch whose terminal is `keep` is `converged`.
#[test]
fn branch_verdicts_converged_for_keep_terminal() {
    // Best is `bestC` (val 0.9, keep). Side branch root `sideR` keep → `sideT` keep.
    let exps = vec![
        make_exp("bestC", 0.9, Status::Keep, "main", None, vec![]),
        make_exp("sideR", 0.95, Status::Keep, "side-baseline", None, vec![]),
        make_exp(
            "sideT",
            0.93,
            Status::Keep,
            "side-tweak",
            Some("sideR"),
            vec![],
        ),
    ];
    let run = make_run("conv", exps);
    let report = build_distill(&run, None);
    assert_eq!(report.branch_verdicts.len(), 1);
    let v = &report.branch_verdicts[0];
    assert_eq!(v.root_commit, "sideR");
    assert_eq!(v.terminal_commit, "sideT");
    assert_eq!(v.verdict, "converged");
    assert_eq!(v.depth, 2);
    assert!(v.note.is_none());
}

/// Branch verdict: non-best branch whose terminal is `crash` is `broke`
/// and carries the first signal kind as a note.
#[test]
fn branch_verdicts_broke_with_signal_note() {
    let exps = vec![
        make_exp("bestC", 0.9, Status::Keep, "main", None, vec![]),
        make_exp("sideR", 0.95, Status::Keep, "side-baseline", None, vec![]),
        make_exp(
            "sideX",
            0.0,
            Status::Crash,
            "OOM crash",
            Some("sideR"),
            vec![Signal::Oom],
        ),
    ];
    let run = make_run("broke", exps);
    let report = build_distill(&run, None);
    assert_eq!(report.branch_verdicts.len(), 1);
    let v = &report.branch_verdicts[0];
    assert_eq!(v.verdict, "broke");
    assert_eq!(v.note.as_deref(), Some("oom"));
    assert_eq!(v.terminal_status, "crash");
}

/// Branch verdict: non-best branch terminating in `discard` is `abandoned`.
#[test]
fn branch_verdicts_abandoned_for_discard_terminal() {
    let exps = vec![
        make_exp("bestC", 0.9, Status::Keep, "main", None, vec![]),
        make_exp("sideR", 0.95, Status::Discard, "tried thing", None, vec![]),
        make_exp(
            "sideX",
            0.97,
            Status::Discard,
            "tried again",
            Some("sideR"),
            vec![],
        ),
    ];
    let run = make_run("abandon", exps);
    let report = build_distill(&run, None);
    assert_eq!(report.branch_verdicts.len(), 1);
    let v = &report.branch_verdicts[0];
    assert_eq!(v.verdict, "abandoned");
    assert_eq!(v.terminal_status, "discard");
}

/// Branch verdicts: empty when every experiment is on the best lineage.
#[test]
fn branch_verdicts_empty_when_all_on_best_chain() {
    let exps = vec![
        make_exp("a", 0.95, Status::Keep, "base", None, vec![]),
        make_exp("b", 0.92, Status::Keep, "tweak", Some("a"), vec![]),
        make_exp("c", 0.90, Status::Best, "winner", Some("b"), vec![]),
    ];
    let run = make_run("linear", exps);
    let report = build_distill(&run, None);
    assert!(
        report.branch_verdicts.is_empty(),
        "all experiments on best lineage; expected no other branches, got: {:?}",
        report.branch_verdicts
    );
}

/// Cross-tag continuation: tag's root parent_commit lives in another tag.
#[test]
fn continuation_link_found_when_parent_lives_in_other_tag() {
    use super::find_continuation;

    let prior = make_run(
        "yesterday",
        vec![
            make_exp("aaa111", 0.99, Status::Keep, "base", None, vec![]),
            make_exp("bbb222", 0.97, Status::Best, "win", Some("aaa111"), vec![]),
        ],
    );
    let today = make_run(
        "today",
        vec![make_exp(
            "ccc333",
            0.96,
            Status::Keep,
            "build on yesterday",
            Some("bbb222"),
            vec![],
        )],
    );

    let link = find_continuation(&today, &[prior.clone(), today.clone()]);
    assert!(link.is_some(), "expected continuation link");
    let link = link.unwrap();
    assert_eq!(link.from_tag, "yesterday");
    assert_eq!(link.from_commit, "bbb222");
}

/// No link when the root's parent isn't in any other tag.
#[test]
fn continuation_link_none_when_parent_external() {
    use super::find_continuation;
    let today = make_run(
        "today",
        vec![make_exp(
            "ccc333",
            0.96,
            Status::Keep,
            "external parent",
            Some("zzz999"),
            vec![],
        )],
    );
    let link = find_continuation(&today, std::slice::from_ref(&today));
    assert!(link.is_none(), "expected no link; got: {:?}", link);
}

/// No link when the run has no external parents.
#[test]
fn continuation_link_none_when_no_external_parents() {
    use super::find_continuation;
    let today = make_run(
        "today",
        vec![
            make_exp("c1", 0.99, Status::Keep, "base", None, vec![]),
            make_exp("c2", 0.98, Status::Keep, "next", Some("c1"), vec![]),
        ],
    );
    let link = find_continuation(&today, std::slice::from_ref(&today));
    assert!(link.is_none());
}

/// Markdown header includes the continuation line when set.
#[test]
fn render_markdown_includes_continuation_line() {
    let mut report = build_distill(
        &make_run(
            "today",
            vec![make_exp("c1", 0.99, Status::Keep, "x", None, vec![])],
        ),
        None,
    );
    report.continues_from = Some(super::ContinuationLink {
        from_tag: "yesterday".to_string(),
        from_commit: "bbb222deadbeef".to_string(),
    });
    let md = super::render::render_markdown(&report);
    assert!(
        md.contains("continues from `yesterday`"),
        "markdown must surface continuation; got:\n{md}"
    );
}

/// Branch verdict markdown rendering exposes the verdict section.
#[test]
fn render_markdown_includes_branch_verdicts_section() {
    let exps = vec![
        make_exp("bestC", 0.9, Status::Keep, "main", None, vec![]),
        make_exp("sideR", 0.0, Status::Crash, "oom", None, vec![Signal::Oom]),
    ];
    let run = make_run("mix", exps);
    let report = build_distill(&run, None);
    let md = super::render::render_markdown(&report);
    assert!(
        md.contains("## Other branches"),
        "expected branch-verdict section in markdown"
    );
    assert!(
        md.contains("verdict=broke"),
        "expected verdict=broke in markdown"
    );
}

/// Keep-but-reverted does NOT fire when no verified ancestor chain exists.
#[test]
fn suggestions_no_keep_reverted_without_verified() {
    let exps = vec![
        make_exp("a", 0.99, Status::Keep, "baseline", None, vec![]),
        make_exp("b", 0.98, Status::Keep, "tweak", Some("a"), vec![]),
        make_exp("c", 0.97, Status::Keep, "tweak", Some("b"), vec![]),
    ];
    let run = make_run("nover", exps);
    let report = build_distill(&run, None);
    assert!(
        !report
            .suggestions
            .iter()
            .any(|s| s.contains("Under-explored")),
        "should not fire keep-but-reverted without any verified node; got: {:?}",
        report.suggestions
    );
}

// ── usage-aware distill tests ────────────────────────────────────────────────

fn make_funnel(added: u64, verified: u64, distilled: u64) -> crate::commands::usage::TagFunnel {
    crate::commands::usage::TagFunnel {
        added,
        verified,
        distilled,
    }
}

fn base_run() -> RunLog {
    make_run(
        "tag1",
        vec![make_exp(
            "abc1",
            0.9,
            Status::Keep,
            "baseline",
            None,
            vec![],
        )],
    )
}

/// Fires when added>=10 and verified==0.
#[test]
fn usage_suggestion_fires_at_threshold() {
    let run = base_run();
    let funnel = make_funnel(10, 0, 0);
    let report = build_distill(&run, Some(&funnel));
    let has = report
        .suggestions
        .iter()
        .any(|s| s.contains("10 experiments added for this tag") && s.contains("resman verify"));
    assert!(
        has,
        "expected usage suggestion for added=10, verified=0; got: {:?}",
        report.suggestions
    );
}

/// Does NOT fire when added==9 (below threshold).
#[test]
fn usage_suggestion_does_not_fire_below_threshold() {
    let run = base_run();
    let funnel = make_funnel(9, 0, 0);
    let report = build_distill(&run, Some(&funnel));
    let has = report
        .suggestions
        .iter()
        .any(|s| s.contains("added for this tag"));
    assert!(
        !has,
        "should not fire when added=9; got: {:?}",
        report.suggestions
    );
}

/// Does NOT fire when verified>=1 (gap already closed).
#[test]
fn usage_suggestion_does_not_fire_when_verified_nonzero() {
    let run = base_run();
    let funnel = make_funnel(20, 1, 0);
    let report = build_distill(&run, Some(&funnel));
    let has = report
        .suggestions
        .iter()
        .any(|s| s.contains("added for this tag"));
    assert!(
        !has,
        "should not fire when verified=1; got: {:?}",
        report.suggestions
    );
}

/// build_distill with None is identical to today — does NOT contain the new string.
#[test]
fn usage_suggestion_absent_when_usage_none() {
    let run = base_run();
    let report = build_distill(&run, None);
    let has = report
        .suggestions
        .iter()
        .any(|s| s.contains("added for this tag"));
    assert!(
        !has,
        "None usage must not produce MCP suggestion; got: {:?}",
        report.suggestions
    );
}

/// tag_funnel counts correctly for the right tag and ignores others.
#[test]
fn tag_funnel_counts_for_correct_tag_only() {
    use crate::commands::usage::{Event, tag_funnel};
    use serde_json::json;

    let events: Vec<Event> = vec![
        Event {
            ts: "t".into(),
            tool: "resman_add_experiment".into(),
            args: json!({"tag": "mytag"}),
            ok: true,
            duration_ms: 1,
            result_chars: 0,
        },
        Event {
            ts: "t".into(),
            tool: "resman_add_experiment".into(),
            args: json!({"tag": "mytag"}),
            ok: true,
            duration_ms: 1,
            result_chars: 0,
        },
        Event {
            ts: "t".into(),
            tool: "resman_verify".into(),
            args: json!({"tag": "mytag"}),
            ok: true,
            duration_ms: 1,
            result_chars: 0,
        },
        // Different tag — must be ignored.
        Event {
            ts: "t".into(),
            tool: "resman_add_experiment".into(),
            args: json!({"tag": "other"}),
            ok: true,
            duration_ms: 1,
            result_chars: 0,
        },
        // Unknown tool — must be ignored.
        Event {
            ts: "t".into(),
            tool: "resman_best".into(),
            args: json!({"tag": "mytag"}),
            ok: true,
            duration_ms: 1,
            result_chars: 0,
        },
    ];

    let f = tag_funnel(&events, "mytag");
    assert_eq!(f.added, 2, "added should be 2");
    assert_eq!(f.verified, 1, "verified should be 1");
    assert_eq!(f.distilled, 0, "distilled should be 0");
}

/// Reproducibility-gap suggestion must be SUPPRESSED when added>=10 & verified==0
/// but every run is a crash (no verifiable runs exist).
#[test]
fn usage_suggestion_suppressed_when_all_crashes() {
    let run = make_run(
        "crashonly",
        vec![
            make_exp("c1", 9.9, Status::Crash, "run1", None, vec![]),
            make_exp("c2", 9.9, Status::Crash, "run2", None, vec![]),
        ],
    );
    let funnel = make_funnel(10, 0, 0);
    let report = build_distill(&run, Some(&funnel));
    let fires = report
        .suggestions
        .iter()
        .any(|s| s.contains("added for this tag") && s.contains("resman verify"));
    assert!(
        !fires,
        "usage suggestion must not fire when all runs are crashes; got: {:?}",
        report.suggestions
    );
}

/// Reproducibility-gap suggestion STILL FIRES when added>=10, verified==0, and at least
/// one keeper (Keep status) exists.
#[test]
fn usage_suggestion_fires_when_keeper_exists() {
    let run = make_run(
        "withkeeper",
        vec![
            make_exp("c1", 9.9, Status::Crash, "run1", None, vec![]),
            make_exp("c2", 0.9, Status::Keep, "run2", None, vec![]),
        ],
    );
    let funnel = make_funnel(10, 0, 0);
    let report = build_distill(&run, Some(&funnel));
    let fires = report
        .suggestions
        .iter()
        .any(|s| s.contains("10 experiments added for this tag") && s.contains("resman verify"));
    assert!(
        fires,
        "usage suggestion must fire when keeper exists and added>=10, verified==0; got: {:?}",
        report.suggestions
    );
}

/// load_events on a missing file returns empty Vec (never panics).
#[test]
fn load_events_missing_file_returns_empty() {
    use crate::commands::usage::load_events;
    let dir = tempfile::tempdir().unwrap();
    let events = load_events(dir.path());
    assert!(
        events.is_empty(),
        "expected empty Vec for missing usage.jsonl"
    );
}
