//! Shared HTML helpers: CSS, escaping, SVG sparkline, badges, page wrapper.
//!
//! Used by both `commands/report.rs` and `commands/distill.rs`.

// ---------------------------------------------------------------------------
// Dark-mode CSS
// ---------------------------------------------------------------------------

pub const CSS_DARK: &str = r#"
  :root { color-scheme: dark; }
  body { font: 14px/1.5 ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif;
         margin: 0; padding: 32px; background: #0b0d10; color: #d8dee9; max-width: 1100px; }
  h1 { color: #88c0d0; margin: 0 0 4px; font-weight: 600; letter-spacing: -0.01em; }
  h2 { color: #a3be8c; margin: 36px 0 12px; font-weight: 500; font-size: 16px;
        text-transform: uppercase; letter-spacing: 0.08em; }
  h3 { color: #b48ead; margin: 20px 0 8px; font-weight: 500; font-size: 14px; }
  .sub { color: #6b7280; font-size: 13px; margin-bottom: 24px; }
  .stats { display: grid; grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
            gap: 16px; margin: 20px 0 8px; }
  .stat { background: #13161b; border: 1px solid #1f242c; border-radius: 8px; padding: 14px 16px; }
  .stat-val { font-size: 22px; color: #ebcb8b; font-variant-numeric: tabular-nums; font-weight: 600; }
  .stat-label { font-size: 11px; color: #6b7280; text-transform: uppercase;
                 letter-spacing: 0.08em; margin-top: 4px; }
  table { border-collapse: collapse; width: 100%; margin: 12px 0; font-variant-numeric: tabular-nums; }
  th { color: #b48ead; text-align: left; padding: 8px 10px; border-bottom: 2px solid #2e3440;
        font-size: 11px; text-transform: uppercase; letter-spacing: 0.06em; font-weight: 600; }
  td { padding: 8px 10px; border-bottom: 1px solid #1f242c; }
  tr:hover td { background: #13161b; }
  code { font: 13px ui-monospace, "Cascadia Code", monospace; color: #81a1c1; }
  .chart { background: #13161b; border: 1px solid #1f242c; border-radius: 8px; padding: 8px; }
  footer { margin-top: 48px; color: #4c566a; font-size: 12px; }
  /* distill-specific */
  .best-card { background: #13161b; border: 1px solid #2e3440; border-radius: 8px;
               padding: 20px 24px; margin: 16px 0; }
  .best-card .metric { font-size: 28px; color: #ebcb8b; font-variant-numeric: tabular-nums;
                        font-weight: 700; }
  .best-card .commit-hash { font: 13px ui-monospace, "Cascadia Code", monospace;
                              color: #81a1c1; margin: 8px 0; }
  .best-card .desc { color: #d8dee9; margin-top: 8px; }
  .best-card .gpu { color: #6b7280; font-size: 12px; margin-top: 4px; }
  .signal-cluster { margin: 12px 0; }
  .signal-cluster details { background: #13161b; border: 1px solid #1f242c;
                              border-radius: 6px; padding: 8px 14px; }
  .signal-cluster summary { cursor: pointer; color: #d8dee9; font-size: 13px; }
  .signal-cluster ul { margin: 8px 0 4px 16px; color: #d8dee9; font-size: 13px; }
  .no-best { background: #13161b; border: 1px solid #2e3440; border-radius: 8px;
              padding: 16px 20px; color: #6b7280; font-style: italic; margin: 16px 0; }
  /* badges */
  .badge { display: inline-block; padding: 1px 7px; border-radius: 10px; font-size: 11px;
            font-weight: 600; letter-spacing: 0.03em; margin: 0 3px; }
  .badge-keep    { background: #2a3c2a; color: #a3be8c; }
  .badge-best    { background: #1e3040; color: #88c0d0; }
  .badge-verified { background: #1e3520; color: #a3be8c; font-weight: 700; border: 1px solid #a3be8c44; }
  .badge-crash   { background: #3c1e1e; color: #bf616a; }
  .badge-discard { background: #1c1e24; color: #4c566a; }
  .badge-neutral { background: #1c1f27; color: #616e88; }
"#;

// ---------------------------------------------------------------------------
// HTML escaping
// ---------------------------------------------------------------------------

pub fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

// ---------------------------------------------------------------------------
// Badges
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub enum BadgeKind {
    Keep,
    Best,
    Verified,
    Crash,
    Discard,
    Neutral,
}

impl BadgeKind {
    fn css_class(self) -> &'static str {
        match self {
            BadgeKind::Keep => "keep",
            BadgeKind::Best => "best",
            BadgeKind::Verified => "verified",
            BadgeKind::Crash => "crash",
            BadgeKind::Discard => "discard",
            BadgeKind::Neutral => "neutral",
        }
    }
}

pub fn badge(label: &str, kind: BadgeKind) -> String {
    format!(
        r#"<span class="badge badge-{}">{}</span>"#,
        kind.css_class(),
        html_escape(label)
    )
}

// ---------------------------------------------------------------------------
// Trend SVG
// ---------------------------------------------------------------------------

/// Build a trend SVG from `(index, value)` pairs.
/// Returns an empty `<svg>` placeholder when the slice is empty.
pub fn trend_svg(metric_points: &[(usize, f64)], width: usize, height: usize) -> String {
    if metric_points.is_empty() {
        return format!(
            "<svg viewBox='0 0 {width} {height}' \
             width='100%' style='max-width:{width}px'></svg>"
        );
    }

    let bpbs: Vec<f64> = metric_points.iter().map(|(_, v)| *v).collect();
    let max = bpbs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let min = bpbs.iter().copied().fold(f64::INFINITY, f64::min);
    let range = if (max - min).abs() < f64::EPSILON {
        1.0
    } else {
        max - min
    };

    let w = width as f64;
    let h = height as f64;
    let pad_l: f64 = 70.0;
    let pad_r: f64 = 20.0;
    let pad_t: f64 = 20.0;
    let pad_b: f64 = 32.0;
    let plot_w = w - pad_l - pad_r;
    let plot_h = h - pad_t - pad_b;
    let n = bpbs.len() as f64;

    let xy = |i: usize, v: f64| {
        let x = if n > 1.0 {
            pad_l + (i as f64 / (n - 1.0)) * plot_w
        } else {
            pad_l + plot_w / 2.0
        };
        let y = pad_t + (1.0 - (v - min) / range) * plot_h;
        (x, y)
    };

    let points: String = bpbs
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let (x, y) = xy(i, *v);
            format!("{x:.1},{y:.1}")
        })
        .collect::<Vec<_>>()
        .join(" ");

    let dots: String = bpbs
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let (x, y) = xy(i, *v);
            format!("<circle cx='{x:.1}' cy='{y:.1}' r='3.5' fill='#88c0d0'/>")
        })
        .collect();

    let y_axis: String = (0..=4)
        .map(|i| {
            let val = min + range * (i as f64 / 4.0);
            let y = pad_t + plot_h - (i as f64 / 4.0) * plot_h;
            format!(
                "<line x1='{pad_l}' y1='{y:.0}' x2='{:.0}' y2='{y:.0}' stroke='#1f242c' stroke-width='1'/>\
                 <text x='{:.0}' y='{:.0}' fill='#6b7280' font-size='10' text-anchor='end'>{val:.4}</text>",
                pad_l + plot_w,
                pad_l - 6.0,
                y + 3.0,
            )
        })
        .collect();

    format!(
        "<svg viewBox='0 0 {w} {h}' width='100%' style='max-width:{w}px'>\
           {y_axis}\
           <polyline points='{points}' fill='none' stroke='#88c0d0' stroke-width='2' stroke-linejoin='round'/>\
           {dots}\
           <text x='{tx:.0}' y='{ty:.0}' fill='#6b7280' font-size='10' text-anchor='middle'>experiment #</text>\
         </svg>",
        tx = pad_l + plot_w / 2.0,
        ty = h - 8.0,
    )
}

// ---------------------------------------------------------------------------
// Page wrapper
// ---------------------------------------------------------------------------

pub fn page(title: &str, body: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>{escaped_title}</title>
<style>{css}</style>
</head>
<body>
{body}
<footer>generated by resman &middot; local-first experiment tracker</footer>
</body></html>"#,
        escaped_title = html_escape(title),
        css = CSS_DARK,
        body = body,
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_escape_basic() {
        assert_eq!(html_escape("<script>&"), "&lt;script&gt;&amp;");
        assert_eq!(html_escape("hello"), "hello");
    }

    #[test]
    fn html_escape_no_change_plain() {
        let s = "plain text 123";
        assert_eq!(html_escape(s), s);
    }

    #[test]
    fn badge_keep_contains_class() {
        let b = badge("keep", BadgeKind::Keep);
        assert!(b.contains("badge-keep"));
        assert!(b.contains("keep"));
    }

    #[test]
    fn badge_crash_escapes_html() {
        let b = badge("<crash>", BadgeKind::Crash);
        assert!(b.contains("&lt;crash&gt;"));
        assert!(!b.contains("<crash>"));
    }

    #[test]
    fn badge_kinds_all_have_distinct_classes() {
        let kinds = [
            BadgeKind::Keep,
            BadgeKind::Best,
            BadgeKind::Verified,
            BadgeKind::Crash,
            BadgeKind::Discard,
            BadgeKind::Neutral,
        ];
        let classes: Vec<_> = kinds.iter().map(|k| k.css_class()).collect();
        // All distinct
        let unique: std::collections::HashSet<_> = classes.iter().collect();
        assert_eq!(unique.len(), kinds.len());
    }

    #[test]
    fn trend_svg_empty_returns_placeholder() {
        let svg = trend_svg(&[], 800, 200);
        assert!(svg.contains("<svg"));
        assert!(svg.contains("</svg>"));
        // No polyline when empty
        assert!(!svg.contains("polyline"));
    }

    #[test]
    fn trend_svg_single_point() {
        let svg = trend_svg(&[(0, 1.23)], 800, 200);
        assert!(svg.contains("circle"));
    }

    #[test]
    fn trend_svg_no_external_refs() {
        let svg = trend_svg(&[(0, 1.0), (1, 0.9)], 1040, 280);
        assert!(!svg.contains("http://"));
        assert!(!svg.contains("https://"));
    }

    #[test]
    fn page_contains_title_and_style() {
        let p = page("My Title", "<p>body</p>");
        assert!(p.contains("<title>My Title</title>"));
        assert!(p.contains("<style>"));
        assert!(p.contains("<p>body</p>"));
        assert!(!p.contains("http://"));
    }
}
