//! Output rendering (Markdown, JSON, HTML) for distill reports.

use crate::html::{BadgeKind, badge, data_table, html_escape, section, trend_svg};
use crate::model::Status;
use crate::store::truncate;

use super::{CrossDistillReport, DistillReport, short_commit, status_glyph};

// ---------------------------------------------------------------------------
// Markdown rendering
// ---------------------------------------------------------------------------

pub fn render_markdown(report: &DistillReport) -> String {
    let mut out = String::new();
    let s = &report.summary;

    out.push_str(&format!("# Distill: {}\n\n", report.tag));
    out.push_str(&format!(
        "_Generated from {} experiments ({} keep, {} verified, {} discard, {} crash, {} best). Metric: {} ({})._\n",
        s.total, s.keep, s.verified, s.discard, s.crash, s.best, s.metric_name, s.direction
    ));
    if let Some(c) = &report.continues_from {
        out.push_str(&format!(
            "_continues from `{}` (commit `{}`)._\n",
            c.from_tag,
            short_commit(&c.from_commit)
        ));
    }
    out.push('\n');

    // --- Best result ---
    out.push_str("## Best result\n");
    match &report.best {
        None => out.push_str("_no kept experiments in this run_\n"),
        Some(b) => {
            out.push_str(&format!("- **{}**: `{:.6}`\n", s.metric_name, b.value));
            out.push_str(&format!("- **commit**: `{}`\n", short_commit(&b.commit)));
            out.push_str(&format!("- **description**: {}\n", b.description));
            let gpu = if b.gpu.is_empty() {
                "unspecified"
            } else {
                &b.gpu
            };
            out.push_str(&format!("- **GPU**: {gpu}\n"));
        }
    }
    out.push('\n');

    // --- Lineage ---
    out.push_str("## Lineage to best\n");
    if report.lineage.is_empty() {
        out.push_str("_no lineage recorded_\n");
    } else {
        for entry in &report.lineage {
            let status: Status = entry.status.parse().unwrap_or(Status::Keep);
            let glyph = status_glyph(status);
            out.push_str(&format!(
                "  `{}` {} {}={:.6}  {}\n",
                short_commit(&entry.commit),
                glyph,
                s.metric_name,
                entry.metric,
                truncate(&entry.description, 60)
            ));
        }
    }
    out.push('\n');

    // --- Other branches (non-best roots with verdict labels) ---
    if !report.branch_verdicts.is_empty() {
        out.push_str("## Other branches\n");
        for v in &report.branch_verdicts {
            let note = match &v.note {
                Some(n) => format!(" ({n})"),
                None => String::new(),
            };
            out.push_str(&format!(
                "- `{}` → … → `{}` [{}]  depth={} verdict={}{}\n",
                short_commit(&v.root_commit),
                short_commit(&v.terminal_commit),
                v.terminal_status,
                v.depth,
                v.verdict,
                note,
            ));
        }
        out.push('\n');
    }

    // --- Failure signals ---
    out.push_str("## Failure signals\n\n");
    let any_signals = report.failure_signals.values().any(|v| !v.is_empty());
    if !any_signals {
        out.push_str("_no crash signals recorded in this run_\n");
    } else {
        for kind in crate::signals::ALL_KINDS {
            let entries = match report.failure_signals.get(*kind) {
                Some(v) if !v.is_empty() => v,
                _ => continue,
            };
            out.push_str(&format!("### {} ({})\n", kind, entries.len()));
            for e in entries {
                let detail = if e.detail.is_empty() {
                    String::new()
                } else {
                    format!(" — {}", e.detail)
                };
                out.push_str(&format!(
                    "- `{}` — {}{}\n",
                    short_commit(&e.commit),
                    e.description,
                    detail
                ));
            }
            out.push('\n');
        }
    }
    out.push('\n');

    // --- Unexplored neighbors ---
    out.push_str("## Unexplored neighbors\n");
    if report.unexplored_neighbors.is_empty() {
        out.push_str("_no neighbors found_\n");
    } else {
        for n in &report.unexplored_neighbors {
            out.push_str(&format!(
                "- `{}` {}={:.6} (Δ={:+.4}) — {}\n",
                short_commit(&n.commit),
                s.metric_name,
                n.value,
                n.delta,
                n.description
            ));
        }
    }
    out.push('\n');

    // --- Suggestions ---
    out.push_str("## Suggestions\n");
    if report.suggestions.is_empty() {
        out.push_str("_no mechanical suggestions — run looks healthy._\n");
    } else {
        for (i, sug) in report.suggestions.iter().enumerate() {
            out.push_str(&format!("{}. {}\n", i + 1, sug));
        }
    }
    out.push('\n');

    out.push_str("---\n");
    out.push_str(&format!(
        "_resman distill v{} — {}_\n",
        env!("CARGO_PKG_VERSION"),
        report.generated_at
    ));

    out
}

// ---------------------------------------------------------------------------
// HTML rendering
// ---------------------------------------------------------------------------

fn status_badge(status_str: &str) -> String {
    let kind = match status_str {
        "keep" => BadgeKind::Keep,
        "best" => BadgeKind::Best,
        "verified" => BadgeKind::Verified,
        "crash" => BadgeKind::Crash,
        "discard" => BadgeKind::Discard,
        _ => BadgeKind::Neutral,
    };
    badge(status_str, kind)
}

/// Render the distill report as a self-contained HTML string.
/// Pure function — no IO, no ANSI escapes.
pub fn render_html(report: &DistillReport) -> String {
    let s = &report.summary;

    // --- header badges ---
    let mut header_badges = String::new();
    if s.keep > 0 {
        header_badges.push_str(&badge(&format!("{} keep", s.keep), BadgeKind::Keep));
    }
    if s.best > 0 {
        header_badges.push_str(&badge(&format!("{} best", s.best), BadgeKind::Best));
    }
    if s.crash > 0 {
        header_badges.push_str(&badge(&format!("{} crash", s.crash), BadgeKind::Crash));
    }
    if s.discard > 0 {
        header_badges.push_str(&badge(
            &format!("{} discard", s.discard),
            BadgeKind::Discard,
        ));
    }
    let verified_count = report
        .lineage
        .iter()
        .filter(|e| e.status == "verified")
        .count();
    if verified_count > 0 {
        header_badges.push_str(&badge(
            &format!("{verified_count} verified"),
            BadgeKind::Verified,
        ));
    }
    header_badges.push_str(&badge(
        &format!("{} ({})", s.metric_name, s.direction),
        BadgeKind::Neutral,
    ));

    let mut body = format!(
        "<h1>Distill &mdash; {tag}</h1>\n\
         <div class=\"sub\">{gen} &middot; {total} experiments {badges}</div>\n",
        tag = html_escape(&report.tag),
        gen = html_escape(&report.generated_at),
        total = s.total,
        badges = header_badges,
    );

    // --- sparkline ---
    let kept_entries: Vec<_> = report
        .lineage
        .iter()
        .enumerate()
        .filter(|(_, e)| e.metric > 0.0)
        .collect();
    let kept_points: Vec<(usize, f64)> = kept_entries.iter().map(|(i, e)| (*i, e.metric)).collect();
    if kept_points.len() >= 2 {
        let label_strings: Vec<String> = kept_entries
            .iter()
            .map(|(_, e)| short_commit(&e.commit).to_string())
            .collect();
        let labels: Vec<&str> = label_strings.iter().map(|s| s.as_str()).collect();
        let svg = trend_svg(&kept_points, &labels, 1040, 280);
        body.push_str(&section(
            "Metric trajectory",
            &format!("<div class=\"chart\">{svg}</div>"),
        ));
        body.push('\n');
    }

    // --- best card ---
    {
        let best_content = match &report.best {
            None => "<div class=\"no-best\">No best experiment in this run.</div>\n".to_string(),
            Some(b) => {
                let gpu_line = if b.gpu.is_empty() {
                    String::new()
                } else {
                    format!("<div class=\"gpu\">GPU: {}</div>", html_escape(&b.gpu))
                };
                format!(
                    "<section class=\"best-card\">\
                       <div class=\"metric\">{metric_name}: {value:.6}</div>\
                       <div class=\"commit-hash\"><code>{commit}</code></div>\
                       <div class=\"desc\">{desc}</div>\
                       {gpu_line}\
                     </section>\n",
                    metric_name = html_escape(&s.metric_name),
                    value = b.value,
                    commit = html_escape(&b.commit),
                    desc = html_escape(&b.description),
                )
            }
        };
        body.push_str(&section("Best result", &best_content));
        body.push('\n');
    }

    // --- lineage ---
    {
        let lineage_content = if report.lineage.is_empty() {
            crate::html::empty("no lineage recorded")
        } else {
            let mut ol = "<ol>\n".to_string();
            for entry in &report.lineage {
                let sc = short_commit(&entry.commit);
                ol.push_str(&format!(
                    "<li>{status_badge} <code>{commit}</code> &mdash; \
                     {metric_name}={value:.6} &mdash; {desc}</li>\n",
                    status_badge = status_badge(&entry.status),
                    commit = html_escape(sc),
                    metric_name = html_escape(&s.metric_name),
                    value = entry.metric,
                    desc = html_escape(&truncate(&entry.description, 80)),
                ));
            }
            ol.push_str("</ol>\n");
            ol
        };
        body.push_str(&section("Lineage to best", &lineage_content));
        body.push('\n');
    }

    // --- other branches (non-best roots with verdict labels) ---
    if !report.branch_verdicts.is_empty() {
        let mut ul = "<ul>\n".to_string();
        for v in &report.branch_verdicts {
            let badge_kind = match v.verdict.as_str() {
                "converged" => BadgeKind::Keep,
                "broke" => BadgeKind::Crash,
                _ => BadgeKind::Discard, // abandoned
            };
            let verdict_badge = badge(&v.verdict, badge_kind);
            let note_html = match &v.note {
                Some(n) => format!(" <span class=\"detail\">({})</span>", html_escape(n)),
                None => String::new(),
            };
            ul.push_str(&format!(
                "<li><code>{root}</code> → … → <code>{term}</code> [{status}]  depth={depth}  {badge}{note}</li>\n",
                root = html_escape(short_commit(&v.root_commit)),
                term = html_escape(short_commit(&v.terminal_commit)),
                status = html_escape(&v.terminal_status),
                depth = v.depth,
                badge = verdict_badge,
                note = note_html,
            ));
        }
        ul.push_str("</ul>\n");
        body.push_str(&section("Other branches", &ul));
        body.push('\n');
    }

    // --- failure signals ---
    {
        let any_signals = report.failure_signals.values().any(|v| !v.is_empty());
        let signals_content = if !any_signals {
            crate::html::empty("no crash signals recorded in this run")
        } else {
            // Sort kinds by count desc.
            let mut kinds: Vec<(&String, &Vec<super::FailureSignalEntry>)> = report
                .failure_signals
                .iter()
                .filter(|(_, v)| !v.is_empty())
                .collect();
            kinds.sort_by_key(|x| std::cmp::Reverse(x.1.len()));

            let mut clusters = String::new();
            for (kind, entries) in &kinds {
                let mut items = String::new();
                for e in *entries {
                    let detail = if e.detail.is_empty() {
                        String::new()
                    } else {
                        format!(" &mdash; {}", html_escape(&e.detail))
                    };
                    items.push_str(&format!(
                        "<li><code>{commit}</code> &mdash; {desc}{detail}</li>\n",
                        commit = html_escape(short_commit(&e.commit)),
                        desc = html_escape(&e.description),
                    ));
                }
                clusters.push_str(&format!(
                    "<div class=\"signal-cluster\">\
                       <details>\
                         <summary>{kind} &times; {count}</summary>\
                         <ul>{items}</ul>\
                       </details>\
                     </div>\n",
                    kind = html_escape(kind),
                    count = entries.len(),
                ));
            }
            clusters
        };
        body.push_str(&section("Failure signals", &signals_content));
        body.push('\n');
    }

    // --- unexplored neighbors ---
    {
        let neighbors_content = if report.unexplored_neighbors.is_empty() {
            crate::html::empty("no neighbors found")
        } else {
            let neighbor_rows: String = report
                .unexplored_neighbors
                .iter()
                .map(|n| {
                    format!(
                        "<tr><td><code>{commit}</code></td>\
                             <td>{value:.6}</td>\
                             <td>{delta:+.4}</td>\
                             <td>{desc}</td></tr>\n",
                        commit = html_escape(short_commit(&n.commit)),
                        value = n.value,
                        delta = n.delta,
                        desc = html_escape(&n.description),
                    )
                })
                .collect();
            data_table(&["commit", "value", "Δ", "description"], &neighbor_rows)
        };
        body.push_str(&section("Unexplored neighbors", &neighbors_content));
        body.push('\n');
    }

    // --- suggestions ---
    {
        let suggestions_content = if report.suggestions.is_empty() {
            crate::html::empty("no mechanical suggestions — run looks healthy.")
        } else {
            let mut ul = "<ul>\n".to_string();
            for sug in &report.suggestions {
                ul.push_str(&format!("<li>{}</li>\n", html_escape(sug)));
            }
            ul.push_str("</ul>\n");
            ul
        };
        body.push_str(&section("Suggestions", &suggestions_content));
        body.push('\n');
    }

    let page_title = format!("resman distill — {}", &report.tag);
    crate::html::page(&page_title, &body)
}

// ---------------------------------------------------------------------------
// Cross-run HTML rendering
// ---------------------------------------------------------------------------

/// Render the cross-run distill report as a self-contained HTML page.
/// Mirrors `render_html` in structure: same CSS, same helpers, no external refs.
pub fn render_cross_html(report: &CrossDistillReport) -> String {
    use crate::html::{BadgeKind, badge, data_table, empty, html_escape, section};

    // --- header badges ---
    let mut header_badges = String::new();
    if report.total_keep > 0 {
        header_badges.push_str(&badge(
            &format!("{} keep", report.total_keep),
            BadgeKind::Keep,
        ));
    }
    if report.total_verified > 0 {
        header_badges.push_str(&badge(
            &format!("{} verified", report.total_verified),
            BadgeKind::Verified,
        ));
    }
    if report.total_crash > 0 {
        header_badges.push_str(&badge(
            &format!("{} crash", report.total_crash),
            BadgeKind::Crash,
        ));
    }
    if report.total_discard > 0 {
        header_badges.push_str(&badge(
            &format!("{} discard", report.total_discard),
            BadgeKind::Discard,
        ));
    }

    let mut body = format!(
        "<h1>Distill &mdash; cross-run summary</h1>\n\
         <div class=\"sub\">{gen} &middot; {runs} runs &middot; {exps} experiments total {badges}</div>\n",
        gen = html_escape(&report.generated_at),
        runs = report.total_runs,
        exps = report.total_experiments,
        badges = header_badges,
    );

    // --- top tags by best metric ---
    {
        let tags_content = if report.top_tags.is_empty() {
            empty("no kept experiments across all runs")
        } else {
            let rows: String = report
                .top_tags
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    format!(
                        "<tr><td>{rank}</td>\
                             <td>{tag}</td>\
                             <td>{best:.6}</td>\
                             <td>{dir}</td>\
                             <td>{exps}</td></tr>\n",
                        rank = i + 1,
                        tag = html_escape(&t.tag),
                        best = t.best_value,
                        dir = html_escape(&t.direction),
                        exps = t.experiment_count,
                    )
                })
                .collect();
            data_table(&["#", "tag", "best", "direction", "experiments"], &rows)
        };
        body.push_str(&section("Top tags by best metric", &tags_content));
        body.push('\n');
    }

    // --- top failure signals ---
    {
        let signals_content = if report.top_failure_signals.is_empty() {
            empty("no failure signals recorded")
        } else {
            let mut clusters = String::new();
            for sig in &report.top_failure_signals {
                let mut items = String::new();
                for ex in &sig.examples {
                    items.push_str(&format!(
                        "<li><code>{commit}</code> [{tag}] &mdash; {desc}</li>\n",
                        commit = html_escape(&ex.commit),
                        tag = html_escape(&ex.tag),
                        desc = html_escape(&ex.description),
                    ));
                }
                clusters.push_str(&format!(
                    "<div class=\"signal-cluster\">\
                       <details>\
                         <summary>{kind} ({count})</summary>\
                         <ul>{items}</ul>\
                       </details>\
                     </div>\n",
                    kind = html_escape(&sig.kind),
                    count = sig.count,
                ));
            }
            clusters
        };
        body.push_str(&section("Top failure signals", &signals_content));
        body.push('\n');
    }

    // --- suggestions ---
    {
        let suggestions_content = if report.suggestions.is_empty() {
            empty("no mechanical suggestions — runs look healthy.")
        } else {
            let mut ul = "<ul>\n".to_string();
            for sug in &report.suggestions {
                ul.push_str(&format!("<li>{}</li>\n", html_escape(sug)));
            }
            ul.push_str("</ul>\n");
            ul
        };
        body.push_str(&section("Suggestions", &suggestions_content));
        body.push('\n');
    }

    crate::html::page("resman distill — cross-run", &body)
}

// ---------------------------------------------------------------------------
// Cross-run Markdown rendering
// ---------------------------------------------------------------------------

pub fn render_cross_markdown(report: &CrossDistillReport) -> String {
    let mut out = String::new();
    out.push_str("# Distill: cross-run summary\n\n");
    out.push_str(&format!(
        "_Generated from {} runs, {} experiments total \
         ({} keep, {} discard, {} crash, {} verified)._\n\n",
        report.total_runs,
        report.total_experiments,
        report.total_keep,
        report.total_discard,
        report.total_crash,
        report.total_verified,
    ));

    // Top tags
    out.push_str("## Top tags by best metric\n");
    if report.top_tags.is_empty() {
        out.push_str("_no kept experiments across all runs_\n");
    } else {
        for (i, t) in report.top_tags.iter().enumerate() {
            out.push_str(&format!(
                "{}. **{}** — {}={:.6} ({}) — {} experiments\n",
                i + 1,
                t.tag,
                t.metric_name,
                t.best_value,
                t.direction,
                t.experiment_count,
            ));
        }
    }
    out.push('\n');

    // Top failure signals
    out.push_str("## Top failure signals\n");
    if report.top_failure_signals.is_empty() {
        out.push_str("_no failure signals recorded_\n");
    } else {
        for sig in &report.top_failure_signals {
            out.push_str(&format!("### {} ({})\n", sig.kind, sig.count));
            for ex in &sig.examples {
                out.push_str(&format!(
                    "- `{}` [{}] — {}\n",
                    ex.commit, ex.tag, ex.description
                ));
            }
            out.push('\n');
        }
    }

    // Suggestions
    out.push_str("## Suggestions\n");
    if report.suggestions.is_empty() {
        out.push_str("_no mechanical suggestions — runs look healthy._\n");
    } else {
        for (i, s) in report.suggestions.iter().enumerate() {
            out.push_str(&format!("{}. {}\n", i + 1, s));
        }
    }
    out.push('\n');

    out.push_str("---\n");
    out.push_str(&format!(
        "_resman distill --all v0.8 — {}_\n",
        report.generated_at
    ));
    out
}
