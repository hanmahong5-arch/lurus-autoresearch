//! Shared HTML helpers: CSS, escaping, SVG sparkline, badges, page wrapper.
//!
//! Used by both `commands/report.rs` and `commands/distill.rs`.

// ---------------------------------------------------------------------------
// Dual-theme CSS
// ---------------------------------------------------------------------------

pub const CSS: &str = r#"
/* ── DESIGN TOKENS ─────────────────────────────────────────────────────── */
:root {
  color-scheme: light dark;
  --bg:             #f7f8fa;
  --surface:        #ffffff;
  --surface-raised: #f0f2f5;
  --border:         #e2e6ed;
  --border-strong:  #c5ccd8;
  --text:           #1a1d23;
  --muted:          #6b7280;
  --footer:         #9aa1ad;
  --accent:         #2b7a8f;
  --ok:             #3d7a46;
  --warn:           #9c7a1a;
  --err:            #a0373f;
  --mauve:          #7d5b8a;
  --steel:          #3a5a8a;
  --badge-keep-bg:      #d9f0dc; --badge-keep-fg:      #2a6b35;
  --badge-best-bg:      #d3e8f7; --badge-best-fg:      #1a4f80;
  --badge-verified-bg:  #d5edd8; --badge-verified-fg:  #1e5c28;
  --badge-crash-bg:     #fde0e0; --badge-crash-fg:     #8a2020;
  --badge-discard-bg:   #e8eaed; --badge-discard-fg:   #5a6370;
  --badge-neutral-bg:   #eaecf0; --badge-neutral-fg:   #5a6475;
  --font-sans: ui-sans-serif, system-ui, -apple-system, "Segoe UI", Roboto, sans-serif;
  --font-mono: ui-monospace, "Cascadia Code", "JetBrains Mono", "SF Mono", Menlo, monospace;
  --font-size-base: 14px;
  --font-size-h1:   28px;
  --line-height:    1.55;
  --space-1: 4px; --space-2: 8px; --space-3: 16px;
  --space-4: 24px; --space-5: 32px; --space-6: 48px;
  --radius-sm: 6px; --radius-md: 8px; --radius-lg: 12px;
  --shadow-sm: 0 1px 3px rgba(0,0,0,.08), 0 1px 2px rgba(0,0,0,.05);
  --shadow-md: 0 6px 20px rgba(0,0,0,.10), 0 2px 6px rgba(0,0,0,.06);
  --transition: .15s ease;
  --content-w: 1080px;
}
@media (prefers-color-scheme: dark) {
  :root {
    color-scheme: dark;
    --bg:#0b0d10; --surface:#13161b; --surface-raised:#1a1e24;
    --border:#1f242c; --border-strong:#2e3440;
    --text:#d8dee9; --muted:#6b7280; --footer:#4c566a;
    --accent:#88c0d0; --ok:#a3be8c; --warn:#ebcb8b; --err:#bf616a;
    --mauve:#b48ead; --steel:#81a1c1;
    --badge-keep-bg:#2a3c2a; --badge-keep-fg:#a3be8c;
    --badge-best-bg:#1e3040; --badge-best-fg:#88c0d0;
    --badge-verified-bg:#1e3520; --badge-verified-fg:#a3be8c;
    --badge-crash-bg:#3c1e1e; --badge-crash-fg:#bf616a;
    --badge-discard-bg:#1c1e24; --badge-discard-fg:#4c566a;
    --badge-neutral-bg:#1c1f27; --badge-neutral-fg:#616e88;
  }
}
:root[data-theme="dark"] {
  color-scheme: dark;
  --bg:#0b0d10; --surface:#13161b; --surface-raised:#1a1e24;
  --border:#1f242c; --border-strong:#2e3440;
  --text:#d8dee9; --muted:#6b7280; --footer:#4c566a;
  --accent:#88c0d0; --ok:#a3be8c; --warn:#ebcb8b; --err:#bf616a;
  --mauve:#b48ead; --steel:#81a1c1;
  --badge-keep-bg:#2a3c2a; --badge-keep-fg:#a3be8c;
  --badge-best-bg:#1e3040; --badge-best-fg:#88c0d0;
  --badge-verified-bg:#1e3520; --badge-verified-fg:#a3be8c;
  --badge-crash-bg:#3c1e1e; --badge-crash-fg:#bf616a;
  --badge-discard-bg:#1c1e24; --badge-discard-fg:#4c566a;
  --badge-neutral-bg:#1c1f27; --badge-neutral-fg:#616e88;
}
:root[data-theme="light"] {
  color-scheme: light;
  --bg:#f7f8fa; --surface:#ffffff; --surface-raised:#f0f2f5;
  --border:#e2e6ed; --border-strong:#c5ccd8;
  --text:#1a1d23; --muted:#6b7280; --footer:#9aa1ad;
  --accent:#2b7a8f; --ok:#3d7a46; --warn:#9c7a1a; --err:#a0373f;
  --mauve:#7d5b8a; --steel:#3a5a8a;
  --badge-keep-bg:#d9f0dc; --badge-keep-fg:#2a6b35;
  --badge-best-bg:#d3e8f7; --badge-best-fg:#1a4f80;
  --badge-verified-bg:#d5edd8; --badge-verified-fg:#1e5c28;
  --badge-crash-bg:#fde0e0; --badge-crash-fg:#8a2020;
  --badge-discard-bg:#e8eaed; --badge-discard-fg:#5a6370;
  --badge-neutral-bg:#eaecf0; --badge-neutral-fg:#5a6475;
}
html { -webkit-font-smoothing:antialiased; -moz-osx-font-smoothing:grayscale; }
body { font: var(--font-size-base)/var(--line-height) var(--font-sans); margin:0 auto; padding:var(--space-5); background:var(--bg); color:var(--text); max-width:var(--content-w); }
::selection { background:color-mix(in srgb, var(--accent) 26%, transparent); }
h1 { color:var(--accent); margin:0 0 var(--space-1); font-size:var(--font-size-h1); line-height:1.2; font-weight:600; letter-spacing:-0.02em; }
h2 { color:var(--ok); margin:var(--space-5) 0 var(--space-3); font-weight:600; font-size:12px; text-transform:uppercase; letter-spacing:0.09em; }
h3 { color:var(--mauve); margin:var(--space-4) 0 var(--space-2); font-weight:600; font-size:13px; }
.sub { color:var(--muted); font-size:13px; margin-bottom:var(--space-4); }
.empty { color:var(--muted); font-style:italic; }
.stats { display:grid; grid-template-columns:repeat(auto-fit,minmax(150px,1fr)); gap:var(--space-3); margin:var(--space-4) 0 var(--space-2); }
.stat { background:var(--surface); border:1px solid var(--border); border-radius:var(--radius-md); padding:var(--space-3); box-shadow:var(--shadow-sm); transition:transform var(--transition), box-shadow var(--transition); }
.stat:hover { transform:translateY(-1px); box-shadow:var(--shadow-md); }
.stat-val { font-size:24px; color:var(--warn); font-variant-numeric:tabular-nums; font-weight:700; letter-spacing:-0.01em; }
.stat-label { font-size:11px; color:var(--muted); text-transform:uppercase; letter-spacing:0.08em; margin-top:var(--space-1); }
table { border-collapse:collapse; width:100%; margin:var(--space-3) 0; font-variant-numeric:tabular-nums; }
th { color:var(--mauve); text-align:left; padding:var(--space-2) 10px; border-bottom:2px solid var(--border-strong); font-size:11px; text-transform:uppercase; letter-spacing:0.06em; font-weight:700; }
td { padding:var(--space-2) 10px; border-bottom:1px solid var(--border); }
tbody tr { transition:background var(--transition); }
tr:hover td { background:var(--surface-raised); }
code { font:13px var(--font-mono); color:var(--steel); }
.chart { background:var(--surface); border:1px solid var(--border); border-radius:var(--radius-md); padding:var(--space-2); box-shadow:var(--shadow-sm); }
footer { margin-top:var(--space-6); padding-top:var(--space-3); border-top:1px solid var(--border); color:var(--footer); font-size:12px; }
.best-card { background:var(--surface); border:1px solid var(--border-strong); border-left:3px solid var(--accent); border-radius:var(--radius-lg); padding:var(--space-4); margin:var(--space-3) 0; box-shadow:var(--shadow-md); }
.best-card .metric { font-size:30px; color:var(--accent); font-variant-numeric:tabular-nums; font-weight:700; letter-spacing:-0.02em; }
.best-card .commit-hash { font:13px var(--font-mono); color:var(--steel); margin:var(--space-2) 0; }
.best-card .desc { color:var(--text); margin-top:var(--space-2); }
.best-card .gpu { color:var(--muted); font-size:12px; margin-top:var(--space-1); }
.signal-cluster { margin:var(--space-3) 0; }
.signal-cluster details { background:var(--surface); border:1px solid var(--border); border-radius:var(--radius-sm); padding:var(--space-2) 14px; transition:border-color var(--transition); }
.signal-cluster details[open] { border-color:var(--border-strong); }
.signal-cluster summary { cursor:pointer; color:var(--text); font-size:13px; }
.signal-cluster summary:focus-visible { outline:2px solid var(--accent); outline-offset:2px; border-radius:2px; }
.signal-cluster ul { margin:var(--space-2) 0 var(--space-1) var(--space-3); color:var(--text); font-size:13px; }
.no-best { background:var(--surface); border:1px solid var(--border-strong); border-radius:var(--radius-md); padding:var(--space-3) var(--space-4); color:var(--muted); font-style:italic; margin:var(--space-3) 0; }
.detail { color:var(--muted); font-size:12px; }
.badge { display:inline-block; padding:1px 7px; border-radius:10px; font-size:11px; font-weight:600; letter-spacing:0.03em; margin:0 3px; }
.badge-keep { background:var(--badge-keep-bg); color:var(--badge-keep-fg); }
.badge-best { background:var(--badge-best-bg); color:var(--badge-best-fg); }
.badge-verified { background:var(--badge-verified-bg); color:var(--badge-verified-fg); font-weight:700; border:1px solid color-mix(in srgb, var(--badge-verified-fg) 30%, transparent); }
.badge-crash { background:var(--badge-crash-bg); color:var(--badge-crash-fg); }
.badge-discard { background:var(--badge-discard-bg); color:var(--badge-discard-fg); }
.badge-neutral { background:var(--badge-neutral-bg); color:var(--badge-neutral-fg); }
"#;

// ---------------------------------------------------------------------------
// HTML escaping
// ---------------------------------------------------------------------------

pub fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Muted italic empty-state line. Replaces ad-hoc inline `style="color:#..."`.
pub fn empty(msg: &str) -> String {
    format!("<p class=\"empty\"><em>{}</em></p>", html_escape(msg))
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
/// `labels` is parallel to `points` (same length) OR empty.
/// When `labels.len() == points.len()`, point i's tooltip is `"{labels[i]}: {value:.6}"`;
/// otherwise just `"{value:.6}"`.
/// Returns a themed empty-state `<svg>` when the slice is empty.
pub fn trend_svg(
    metric_points: &[(usize, f64)],
    labels: &[&str],
    width: usize,
    height: usize,
) -> String {
    if metric_points.is_empty() {
        return format!(
            "<svg viewBox='0 0 {width} {height}' width='100%' style='max-width:{width}px'>\
             <text x='50%' y='50%' text-anchor='middle' dominant-baseline='middle' \
             fill='var(--muted)' font-size='13'>no data</text></svg>"
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
    let plot_bottom = h - pad_b;
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

    let line_points: String = bpbs
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let (x, y) = xy(i, *v);
            format!("{x:.1},{y:.1}")
        })
        .collect::<Vec<_>>()
        .join(" ");

    // Area fill polygon: close down to plot baseline
    let area_fill = if bpbs.len() >= 2 {
        let (x0, _) = xy(0, bpbs[0]);
        let (xn, _) = xy(bpbs.len() - 1, bpbs[bpbs.len() - 1]);
        let interior: String = bpbs
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let (x, y) = xy(i, *v);
                format!("{x:.1},{y:.1}")
            })
            .collect::<Vec<_>>()
            .join(" ");
        format!(
            "<polygon points='{x0:.1},{plot_bottom:.1} {interior} {xn:.1},{plot_bottom:.1}' \
             fill='url(#resman-area)' stroke='none'/>",
        )
    } else {
        String::new()
    };

    let with_labels = labels.len() == bpbs.len();
    let last_idx = bpbs.len() - 1; // safe: early-returned when empty
    let dots: String = bpbs
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let (x, y) = xy(i, *v);
            let tip = if with_labels {
                format!("{}: {:.6}", html_escape(labels[i]), v)
            } else {
                format!("{:.6}", v)
            };
            if i == last_idx {
                // Emphasize the latest experiment: a soft halo ring + larger dot.
                format!(
                    "<g><circle cx='{x:.1}' cy='{y:.1}' r='6.5' fill='var(--accent)' fill-opacity='0.18'/>\
                     <circle cx='{x:.1}' cy='{y:.1}' r='4' fill='var(--accent)'/>\
                     <title>{tip}</title></g>"
                )
            } else {
                format!(
                    "<g><circle cx='{x:.1}' cy='{y:.1}' r='3.5' fill='var(--accent)'/>\
                     <title>{tip}</title></g>"
                )
            }
        })
        .collect();

    let y_axis: String = (0..=4)
        .map(|i| {
            let val = min + range * (i as f64 / 4.0);
            let y = pad_t + plot_h - (i as f64 / 4.0) * plot_h;
            format!(
                "<line x1='{pad_l}' y1='{y:.0}' x2='{:.0}' y2='{y:.0}' \
                 stroke='var(--border)' stroke-width='1'/>\
                 <text x='{:.0}' y='{:.0}' fill='var(--muted)' font-size='10' \
                 text-anchor='end'>{val:.4}</text>",
                pad_l + plot_w,
                pad_l - 6.0,
                y + 3.0,
            )
        })
        .collect();

    let defs = "<defs><linearGradient id='resman-area' x1='0' y1='0' x2='0' y2='1'>\
                <stop offset='0' stop-color='var(--accent)' stop-opacity='0.22'/>\
                <stop offset='1' stop-color='var(--accent)' stop-opacity='0.012'/>\
                </linearGradient></defs>";
    format!(
        "<svg viewBox='0 0 {w} {h}' width='100%' style='max-width:{w}px'>\
           {defs}\
           {y_axis}\
           {area_fill}\
           <polyline points='{line_points}' fill='none' stroke='var(--accent)' \
           stroke-width='2' stroke-linejoin='round'/>\
           {dots}\
           <text x='{tx:.0}' y='{ty:.0}' fill='var(--muted)' font-size='10' \
           text-anchor='middle'>experiment #</text>\
         </svg>",
        tx = pad_l + plot_w / 2.0,
        ty = h - 8.0,
    )
}

// ---------------------------------------------------------------------------
// Component builders
// ---------------------------------------------------------------------------

/// One stat tile. Label is shown uppercase via CSS (.stat-label) — pass it lowercase.
pub fn stat_card(value: &str, label: &str) -> String {
    format!(
        "<div class=\"stat\"><div class=\"stat-val\">{}</div><div class=\"stat-label\">{}</div></div>",
        html_escape(value),
        html_escape(label)
    )
}

/// Wrap stat_card()s in the .stats grid.
pub fn stats_grid(cards: &[String]) -> String {
    format!("<div class=\"stats\">{}</div>", cards.concat())
}

/// An <h2> section: heading + body HTML (body is trusted pre-built HTML).
pub fn section(title: &str, body: &str) -> String {
    format!("<h2>{}</h2>\n{}", html_escape(title), body)
}

/// A full-width table. headers are static literals emitted verbatim (NOT escaped);
/// rows is pre-built <tr>… HTML emitted verbatim.
pub fn data_table(headers: &[&str], rows: &str) -> String {
    let th: String = headers.iter().map(|h| format!("<th>{h}</th>")).collect();
    format!("<table><thead><tr>{th}</tr></thead><tbody>{rows}</tbody></table>")
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
        css = CSS,
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
        // Attribute-unsafe characters must also be escaped.
        assert_eq!(html_escape("say \"hi\""), "say &quot;hi&quot;");
        assert_eq!(html_escape("it's"), "it&#39;s");
        // Verify & is escaped first so subsequent entity sequences aren't double-escaped.
        assert_eq!(
            html_escape("a&b<c>d\"e'f"),
            "a&amp;b&lt;c&gt;d&quot;e&#39;f"
        );
    }

    #[test]
    fn empty_helper_escapes_and_has_class() {
        let out = empty("<script>alert(1)</script>");
        assert!(out.contains("class=\"empty\""));
        assert!(out.contains("&lt;script&gt;"));
        assert!(!out.contains("<script>"));
        assert!(!out.contains("</script>"));
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
        let svg = trend_svg(&[], &[], 800, 200);
        assert!(svg.contains("<svg"));
        assert!(svg.contains("</svg>"));
        // No polyline when empty
        assert!(!svg.contains("polyline"));
    }

    #[test]
    fn trend_svg_single_point() {
        let svg = trend_svg(&[(0, 1.23)], &[], 800, 200);
        assert!(svg.contains("circle"));
    }

    #[test]
    fn trend_svg_no_external_refs() {
        let svg = trend_svg(&[(0, 1.0), (1, 0.9)], &[], 1040, 280);
        assert!(!svg.contains("http://"));
        assert!(!svg.contains("https://"));
    }

    #[test]
    fn trend_svg_two_points_with_labels() {
        let points = &[(0usize, 1.0f64), (1, 0.9)];
        let labels: &[&str] = &["abc12345", "def67890"];
        let svg = trend_svg(points, labels, 1040, 280);
        // has tooltip elements
        assert!(svg.contains("<title>"), "missing <title>");
        assert!(svg.contains("abc12345"), "missing label text");
        // has area fill polygon
        assert!(svg.contains("<polygon"), "missing <polygon>");
        // uses CSS variable for accent color
        assert!(svg.contains("var(--accent)"), "missing var(--accent)");
        // no hardcoded hex color
        assert!(!svg.contains("#88c0d0"), "found hardcoded #88c0d0");
    }

    #[test]
    fn stat_card_contains_stat_val_and_escaped_value() {
        let card = stat_card("<42>", "total");
        assert!(card.contains("stat-val"));
        assert!(card.contains("stat-label"));
        assert!(card.contains("&lt;42&gt;"));
        assert!(!card.contains("<42>"));
    }

    #[test]
    fn stats_grid_wraps_cards() {
        let cards = vec![stat_card("5", "total"), stat_card("3", "kept")];
        let grid = stats_grid(&cards);
        assert!(grid.contains("class=\"stats\""));
        assert!(grid.contains("stat-val"));
    }

    #[test]
    fn section_contains_h2() {
        let s = section("My Title", "<p>body</p>");
        assert!(s.contains("<h2>My Title</h2>"));
        assert!(s.contains("<p>body</p>"));
    }

    #[test]
    fn section_escapes_title() {
        let s = section("<script>", "<p>body</p>");
        assert!(s.contains("&lt;script&gt;"));
        assert!(!s.contains("<script>"));
    }

    #[test]
    fn data_table_contains_headers_and_tbody() {
        let t = data_table(&["col1", "col2"], "<tr><td>r1</td></tr>");
        assert!(t.contains("<th>col1</th>"));
        assert!(t.contains("<th>col2</th>"));
        assert!(t.contains("<tbody>"));
        assert!(t.contains("<tr><td>r1</td></tr>"));
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
