use std::path::Path;
use chrono::Local;

use crate::model::Experiment;
use crate::store::load_all_runs;

pub fn cmd_report(data_dir: &Path, output: &Path) {
    let runs = load_all_runs(data_dir);
    if runs.is_empty() {
        eprintln!("No experiments found.");
        return;
    }

    let all: Vec<_> = runs.iter().flat_map(|r| r.experiments.clone()).collect();
    let kept: Vec<_> = all.iter().filter(|e| e.status == "keep" || e.status == "best").collect();
    let crashed = all.iter().filter(|e| e.status == "crash").count();

    let bpbs: Vec<f64> = kept.iter().map(|e| e.val_bpb).collect();
    let best = bpbs.iter().fold(f64::INFINITY, |a, b| a.min(*b));
    let worst = bpbs.iter().fold(f64::NEG_INFINITY, |a, b| a.max(*b));
    let improvement = worst - best;

    let svg = build_trend_svg(&kept);

    let rows: String = kept.iter().enumerate().map(|(i, e)| {
        format!("<tr><td>{}</td><td>{:.6}</td><td>{:.1}</td><td>{}</td><td>{}</td></tr>",
            i + 1, e.val_bpb, e.memory_gb, e.commit, e.description)
    }).collect();

    let run_table: String = runs.iter().map(|r| {
        let k: Vec<_> = r.experiments.iter().filter(|e| e.status == "keep" || e.status == "best").collect();
        let b = k.iter().min_by(|a, b| a.val_bpb.partial_cmp(&b.val_bpb).unwrap_or(std::cmp::Ordering::Equal))
            .map(|e| e.val_bpb).unwrap_or(0.0);
        format!("<tr><td>{}</td><td>{:.6}</td><td>{}</td></tr>", r.run_tag, b, k.len())
    }).collect();

    let html = format!(r#"<!DOCTYPE html>
<html><head><meta charset="utf-8"><title>ResMan Report</title>
<style>
body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', monospace; margin: 40px; background: #0f1115; color: #e0e0e0; }}
h1 {{ color: #61afef; }}
h2 {{ color: #98c379; margin-top: 40px; }}
table {{ border-collapse: collapse; width: 100%%; margin: 20px 0; }}
th {{ color: #c678dd; text-align: left; padding: 8px; border-bottom: 2px solid #5c6370; }}
td {{ padding: 8px; border-bottom: 1px solid #2c313c; }}
tr:hover {{ background: #1c1f26; }}
.stat {{ display: inline-block; margin: 10px 20px 10px 0; }}
.stat-val {{ font-size: 24px; color: #e5c07b; }}
.stat-label {{ font-size: 12px; color: #888; }}
svg {{ margin: 20px 0; }}
</style></head><body>
<h1>Research Experiment Report</h1>
<p>Generated: {}</p>
<div class="stats">
<div class="stat"><div class="stat-val">{}</div><div class="stat-label">Total Experiments</div></div>
<div class="stat"><div class="stat-val">{}</div><div class="stat-label">Kept</div></div>
<div class="stat"><div class="stat-val">{}</div><div class="stat-label">Crashed</div></div>
<div class="stat"><div class="stat-val">{:.6}</div><div class="stat-label">Best val_bpb</div></div>
<div class="stat"><div class="stat-val">{:.6}</div><div class="stat-label">Improvement</div></div>
</div>
<h2>Val BPB Trend</h2>
{svg}
<h2>All Kept Experiments</h2>
<table><thead><tr><th>#</th><th>val_bpb</th><th>mem_gb</th><th>commit</th><th>description</th></tr></thead>
<tbody>{rows}</tbody></table>
<h2>Runs</h2>
<table><thead><tr><th>Run</th><th>Best val_bpb</th><th>Kept</th></tr></thead>
<tbody>{run_table}</tbody></table>
</body></html>"#,
        Local::now().to_rfc3339(),
        all.len(), kept.len(), crashed, best, improvement
    );

    if let Err(e) = std::fs::write(output, html) {
        eprintln!("Failed to write report: {}", e);
    } else {
        println!("HTML report written to: {}", output.display());
    }
}

fn build_trend_svg(experiments: &[&Experiment]) -> String {
    if experiments.is_empty() {
        return String::from("<p>No data</p>");
    }

    let bpbs: Vec<f64> = experiments.iter().map(|e| e.val_bpb).collect();
    let max_bpb = bpbs.iter().fold(f64::NEG_INFINITY, |a, b| a.max(*b));
    let min_bpb = bpbs.iter().fold(f64::INFINITY, |a, b| a.min(*b));
    let range = if min_bpb == max_bpb { 1.0 } else { max_bpb - min_bpb };

    let w = 700.0f64;
    let h = 300.0f64;
    let pad_l = 60.0f64;
    let plot_w = w - pad_l - 20.0;
    let plot_h = h - 20.0 - 40.0;
    let n = bpbs.len() as f64;

    let points: Vec<String> = bpbs.iter().enumerate().map(|(i, b)| {
        let x = if n > 1.0 { pad_l + (i as f64 / (n - 1.0)) * plot_w } else { pad_l + plot_w / 2.0 };
        let y = 20.0 + (1.0 - (b - min_bpb) / range) * plot_h;
        format!("{:.1},{:.1}", x, y)
    }).collect();

    let dots: String = bpbs.iter().enumerate().map(|(i, b)| {
        let x = if n > 1.0 { pad_l + (i as f64 / (n - 1.0)) * plot_w } else { pad_l + plot_w / 2.0 };
        let y = 20.0 + (1.0 - (b - min_bpb) / range) * plot_h;
        format!("<circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"5\" fill=\"#61afef\"/>", x, y)
    }).collect();

    let y_labels: String = (0..=4).map(|i| {
        let val = min_bpb + range * i as f64 / 4.0;
        let y = 20.0 + plot_h - (i as f64 / 4.0) * plot_h;
        format!("<text x=\"{:.0}\" y=\"{:.0}\" fill=\"#888\" font-size=\"11\">{:.4}</text>", pad_l - 5.0, y + 4.0, val)
    }).collect();

    format!("<svg width=\"{:.0}\" height=\"{:.0}\" xmlns=\"http://www.w3.org/2000/svg\">
<rect width=\"{:.0}\" height=\"{:.0}\" fill=\"#1a1d23\"/>
<polyline points=\"{}\" fill=\"none\" stroke=\"#61afef\" stroke-width=\"2.5\" stroke-linejoin=\"round\"/>
{}
{}
<text x=\"{:.0}\" y=\"{:.0}\" fill=\"#888\" font-size=\"11\">experiment #</text>
</svg>",
        w, h, w, h,
        points.join(" "),
        y_labels,
        dots,
        pad_l + plot_w / 2.0, h - 8.0)
}
