use std::fs;
use std::path::Path;

use chrono::Local;

use crate::error::Result;
use crate::html::{data_table, html_escape, section, stat_card, stats_grid, trend_svg};
use crate::model::Status;
use crate::store::load_all_runs;

/// Pure function — builds the full HTML string for the report.
/// No IO, no side effects.
pub fn render_report_html(runs: &[crate::model::RunLog], title: &str) -> String {
    let all: Vec<_> = runs.iter().flat_map(|r| r.experiments.clone()).collect();
    let kept: Vec<_> = all.iter().filter(|e| e.status.is_kept()).collect();
    let crashed = all.iter().filter(|e| e.status == Status::Crash).count();

    let bpbs: Vec<f64> = kept
        .iter()
        .map(|e| e.val_bpb)
        .filter(|v| *v > 0.0)
        .collect();
    let best = bpbs.iter().copied().fold(f64::INFINITY, f64::min);
    let worst = bpbs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let improvement = if worst.is_finite() && best.is_finite() {
        worst - best
    } else {
        0.0
    };

    let best_s = if best.is_finite() {
        format!("{best:.6}")
    } else {
        "—".into()
    };
    let imp_s = if improvement > 0.0 {
        format!("{improvement:.6}")
    } else {
        "—".into()
    };

    let metric_points: Vec<(usize, f64)> = kept
        .iter()
        .enumerate()
        .map(|(i, e)| (i, e.val_bpb))
        .collect();
    let svg = if metric_points.is_empty() {
        crate::html::empty("no data")
    } else {
        let label_strings: Vec<String> = kept
            .iter()
            .map(|e| e.commit.chars().take(8).collect())
            .collect();
        let labels: Vec<&str> = label_strings.iter().map(|s| s.as_str()).collect();
        trend_svg(&metric_points, &labels, 1040, 280)
    };

    let rows = kept
        .iter()
        .enumerate()
        .map(|(i, e)| {
            format!(
                "<tr><td>{}</td><td>{:.6}</td><td>{:.1}</td><td><code>{}</code></td><td>{}</td></tr>",
                i + 1,
                e.val_bpb,
                e.memory_gb,
                html_escape(&e.commit),
                html_escape(&e.description)
            )
        })
        .collect::<String>();

    let run_rows = runs
        .iter()
        .map(|r| {
            let b = r.best();
            format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                html_escape(&r.run_tag),
                b.map(|e| format!("{:.6}", e.val_bpb))
                    .unwrap_or_else(|| "—".into()),
                r.kept().count(),
                r.experiments
                    .iter()
                    .filter(|e| e.status == Status::Crash)
                    .count(),
            )
        })
        .collect::<String>();

    let total = all.len();
    let kept_n = kept.len();

    let grid = stats_grid(&[
        stat_card(&total.to_string(), "total"),
        stat_card(&kept_n.to_string(), "kept"),
        stat_card(&crashed.to_string(), "crashed"),
        stat_card(&best_s, "best val_bpb"),
        stat_card(&imp_s, "improvement"),
    ]);

    let chart_section = section(
        "val_bpb trend",
        &format!("<div class=\"chart\">{svg}</div>"),
    );
    let kept_table = section(
        "kept experiments",
        &data_table(&["#", "val_bpb", "mem_gb", "commit", "description"], &rows),
    );
    let runs_table = section(
        "runs",
        &data_table(&["run", "best_val_bpb", "kept", "crashed"], &run_rows),
    );

    let body = format!(
        "<h1>{title}</h1>\n\
         <div class=\"sub\">generated {generated} &middot; {total} experiments across {n_runs} run(s)</div>\n\
         {grid}\n\
         {chart_section}\n\
         {kept_table}\n\
         {runs_table}\n",
        title = html_escape(title),
        generated = Local::now().format("%Y-%m-%d %H:%M"),
        n_runs = runs.len(),
    );

    let page_title = format!("resman — {}", html_escape(title));
    crate::html::page(&page_title, &body)
}

pub fn cmd_report(data_dir: &Path, output: &Path, title: Option<&str>) -> Result<()> {
    let runs = load_all_runs(data_dir)?;
    if runs.is_empty() {
        eprintln!(
            "nothing to report yet — add or import experiments first (`resman add ...` or `resman import <file>`)."
        );
        return Ok(());
    }

    let title = title.unwrap_or("research experiment report");
    let html = render_report_html(&runs, title);
    fs::write(output, html)?;
    println!("html report written to: {}", output.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Experiment, RunLog, Status};

    fn make_run(tag: &str, exps: Vec<Experiment>) -> RunLog {
        RunLog {
            run_tag: tag.to_string(),
            experiments: exps,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            metric_name: None,
            metric_direction: None,
            schema_version: 1,
        }
    }

    fn make_kept(commit: &str, val: f64, desc: &str) -> Experiment {
        Experiment {
            commit: commit.to_string(),
            val_bpb: val,
            memory_gb: 8.0,
            status: Status::Keep,
            description: desc.to_string(),
            timestamp: String::new(),
            params: std::collections::HashMap::new(),
            parent_commit: None,
            crash_excerpt: None,
            metric_name: None,
            metric_direction: None,
            signals: vec![],
        }
    }

    fn sample_runs() -> Vec<RunLog> {
        vec![make_run(
            "smoke",
            vec![
                make_kept("abc1234", 0.9, "baseline"),
                make_kept("def5678", 0.85, "improved"),
            ],
        )]
    }

    #[test]
    fn report_html_is_valid_envelope() {
        let html = render_report_html(&sample_runs(), "test report");
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("</html>"));
        assert!(html.contains("<footer>"));
    }

    #[test]
    fn report_html_has_exactly_one_style_block() {
        let html = render_report_html(&sample_runs(), "test report");
        let count = html.matches("<style>").count();
        assert_eq!(count, 1, "expected exactly 1 <style> block, got {count}");
    }

    #[test]
    fn report_html_no_external_refs() {
        let html = render_report_html(&sample_runs(), "test report");
        assert!(!html.contains("http://"), "found http:// reference");
        assert!(!html.contains("https://"), "found https:// reference");
    }

    #[test]
    fn report_html_uses_stat_and_table() {
        let html = render_report_html(&sample_runs(), "test report");
        assert!(html.contains("stat-val"), "missing stat-val class");
        assert!(html.contains("<table>"), "missing <table>");
    }
}
