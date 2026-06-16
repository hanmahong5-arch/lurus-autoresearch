//! `resman usage` — analyze usage.jsonl telemetry.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use serde_json::Value;

use crate::cli::OutputFormat;
use crate::error::Result;

pub struct UsageOpts {
    pub by_tool: bool,
    pub errors: bool,
    pub sequences: bool,
    /// Explicit summary flag — present for CLI/MCP symmetry; default path when all other flavors are false.
    #[allow(dead_code)]
    pub summary: bool,
    pub tool: Option<String>,
    pub since: Option<String>,
    pub top: usize,
    pub format: OutputFormat,
}

/// Per-call telemetry event (crate-visible so distill can read usage.jsonl).
#[derive(Debug, Clone)]
pub(crate) struct Event {
    pub(crate) ts: String,
    pub(crate) tool: String,
    pub(crate) args: Value,
    pub(crate) ok: bool,
    pub(crate) duration_ms: u64,
    pub(crate) result_chars: u64,
}

/// Adoption-funnel counts for a single tag.
pub(crate) struct TagFunnel {
    pub(crate) added: u64,
    pub(crate) verified: u64,
    /// Included for completeness/tests; not yet used by distill heuristics.
    #[allow(dead_code)]
    pub(crate) distilled: u64,
}

/// Count funnel events for exactly one tag from a pre-loaded event slice.
/// Matches the same tool→field mapping used by `build_funnel`.
pub(crate) fn tag_funnel(events: &[Event], tag: &str) -> TagFunnel {
    let mut added = 0u64;
    let mut verified = 0u64;
    let mut distilled = 0u64;

    for e in events {
        let event_tag = e.args.get("tag").and_then(|v| v.as_str()).unwrap_or("_");
        if event_tag != tag {
            continue;
        }
        match e.tool.as_str() {
            "resman_add_experiment" => added += 1,
            "resman_verify" => verified += 1,
            "resman_distill" => distilled += 1,
            _ => {}
        }
    }

    TagFunnel {
        added,
        verified,
        distilled,
    }
}

/// Load all events from `<data_dir>/usage.jsonl` with no filter applied.
/// Graceful: missing file → empty Vec; unparseable lines → skipped; never panics.
pub(crate) fn load_events(data_dir: &Path) -> Vec<Event> {
    let path = data_dir.join("usage.jsonl");
    // Reuse the private loader with no filters; ignore any error (graceful).
    load_events_inner(&path, None, None).unwrap_or_default()
}

/// Build a JSON summary value suitable for `usage -o json` output.
/// Includes `cold_tools`: TOOL_NAMES entries that never appear as `e.tool`,
/// sorted alphabetically.
pub(crate) fn summary_json(events: &[Event]) -> serde_json::Value {
    let n = events.len();
    let total_ok = events.iter().filter(|e| e.ok).count();
    let funnel = build_funnel(events);
    let called_tools: std::collections::HashSet<&str> =
        events.iter().map(|e| e.tool.as_str()).collect();
    let mut cold: Vec<&str> = crate::commands::mcp::TOOL_NAMES
        .iter()
        .copied()
        .filter(|t| !called_tools.contains(t))
        .collect();
    cold.sort_unstable();
    serde_json::json!({
        "total_events": n,
        "ok": total_ok,
        "errors": n - total_ok,
        "funnel_by_tag": funnel,
        "cold_tools": cold,
    })
}

enum Flavor {
    Summary,
    ByTool,
    Errors,
    Sequences,
}

pub fn cmd_usage(data_dir: &Path, opts: UsageOpts) -> Result<()> {
    let path = data_dir.join("usage.jsonl");
    let events = load_events_inner(&path, opts.tool.as_deref(), opts.since.as_deref())?;

    let flavor = if opts.by_tool {
        Flavor::ByTool
    } else if opts.errors {
        Flavor::Errors
    } else if opts.sequences {
        Flavor::Sequences
    } else {
        Flavor::Summary
    };

    match flavor {
        Flavor::Summary => render_summary(&events, &opts),
        Flavor::ByTool => render_by_tool(&events, &opts),
        Flavor::Errors => render_errors(&events, &opts),
        Flavor::Sequences => render_sequences(&events, &opts),
    }
}

fn load_events_inner(
    path: &Path,
    tool_filter: Option<&str>,
    since_filter: Option<&str>,
) -> Result<Vec<Event>> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut events = Vec::new();

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let v: Value = match serde_json::from_str(trimmed) {
            Ok(val) => val,
            Err(_) => continue, // skip malformed lines
        };

        let ts = v["ts"].as_str().unwrap_or("").to_string();
        let tool = v["tool"].as_str().unwrap_or("").to_string();
        let ok = v["ok"].as_bool().unwrap_or(true);
        let duration_ms = v["duration_ms"].as_u64().unwrap_or(0);
        let result_chars = v["result_chars"].as_u64().unwrap_or(0);
        let args = v["args"].clone();

        // Apply filters.
        if tool_filter.is_some_and(|tf| tool != tf) {
            continue;
        }
        if since_filter.is_some_and(|sf| ts.as_str() < sf) {
            continue;
        }

        events.push(Event {
            ts,
            tool,
            args,
            ok,
            duration_ms,
            result_chars,
        });
    }

    Ok(events)
}

fn percentile(sorted: &[u64], pct: usize) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let idx = (sorted.len() * pct) / 100;
    let idx = idx.min(sorted.len() - 1);
    sorted[idx]
}

fn date_range(events: &[Event]) -> (String, String) {
    if events.is_empty() {
        return ("—".to_string(), "—".to_string());
    }
    let first = events.iter().map(|e| e.ts.as_str()).min().unwrap_or("—");
    let last = events.iter().map(|e| e.ts.as_str()).max().unwrap_or("—");
    // Trim to date only (first 10 chars: YYYY-MM-DD).
    let fmt = |s: &str| {
        if s.len() >= 10 {
            s[..10].to_string()
        } else {
            s.to_string()
        }
    };
    (fmt(first), fmt(last))
}

// ── Summary flavor ────────────────────────────────────────────────────────────

fn render_summary(events: &[Event], opts: &UsageOpts) -> Result<()> {
    let n = events.len();

    match opts.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&summary_json(events))?);
        }
        OutputFormat::Tsv => {
            println!("tag\tadded\tverified\tdistilled");
            let funnel = build_funnel(events);
            for entry in &funnel {
                println!(
                    "{}\t{}\t{}\t{}",
                    entry["tag"].as_str().unwrap_or(""),
                    entry["added"].as_u64().unwrap_or(0),
                    entry["verified"].as_u64().unwrap_or(0),
                    entry["distilled"].as_u64().unwrap_or(0),
                );
            }
        }
        OutputFormat::Table => {
            if n == 0 {
                println!("no usage events recorded yet");
                println!("(run `resman mcp` and make some tool calls to populate usage.jsonl)");
                return Ok(());
            }
            let (first_date, last_date) = date_range(events);
            let total_ok = events.iter().filter(|e| e.ok).count();
            let err_count = n - total_ok;
            println!(
                "=== resman usage ({} events, {} → {}) ===",
                comma(n),
                first_date,
                last_date
            );
            println!();
            println!("total calls : {}", comma(n));
            println!(
                "ok          : {}  ({:.1}%)",
                comma(total_ok),
                pct(total_ok, n)
            );
            println!(
                "errors      : {}  ({:.1}%)",
                comma(err_count),
                pct(err_count, n)
            );
            println!();

            // adoption funnel
            let funnel = build_funnel(events);
            if !funnel.is_empty() {
                println!("adoption funnel (added → verified → distilled) per tag:");
                println!(
                    "  {:<20} {:>8} {:>10} {:>10}",
                    "tag", "added", "verified", "distilled"
                );
                println!("  {}", "-".repeat(52));
                for entry in &funnel {
                    println!(
                        "  {:<20} {:>8} {:>10} {:>10}",
                        entry["tag"].as_str().unwrap_or(""),
                        entry["added"].as_u64().unwrap_or(0),
                        entry["verified"].as_u64().unwrap_or(0),
                        entry["distilled"].as_u64().unwrap_or(0),
                    );
                }
                println!();
            }

            // cold tools
            let called_tools: std::collections::HashSet<&str> =
                events.iter().map(|e| e.tool.as_str()).collect();
            let cold: Vec<&str> = crate::commands::mcp::TOOL_NAMES
                .iter()
                .copied()
                .filter(|t| !called_tools.contains(t))
                .collect();
            if !cold.is_empty() {
                println!("cold tools (0 calls — discoverability gap):");
                println!("  {}", cold.join(", "));
            }
        }
    }
    Ok(())
}

pub(crate) fn build_funnel(events: &[Event]) -> Vec<Value> {
    // Group by args.tag (if present).
    let mut tag_map: HashMap<String, (u64, u64, u64)> = HashMap::new(); // (added, verified, distilled)

    for e in events {
        let tag = e
            .args
            .get("tag")
            .and_then(|v| v.as_str())
            .unwrap_or("_")
            .to_string();
        let entry = tag_map.entry(tag).or_insert((0, 0, 0));
        match e.tool.as_str() {
            "resman_add_experiment" => entry.0 += 1,
            "resman_verify" => entry.1 += 1,
            "resman_distill" => entry.2 += 1,
            _ => {}
        }
    }

    let mut rows: Vec<Value> = tag_map
        .into_iter()
        .map(|(tag, (added, verified, distilled))| {
            serde_json::json!({
                "tag": tag,
                "added": added,
                "verified": verified,
                "distilled": distilled,
            })
        })
        .collect();

    rows.sort_by(|a, b| {
        b["added"]
            .as_u64()
            .unwrap_or(0)
            .cmp(&a["added"].as_u64().unwrap_or(0))
    });
    rows
}

// ── ByTool flavor ─────────────────────────────────────────────────────────────

struct ToolStats {
    tool: String,
    n: usize,
    ok: usize,
    durations: Vec<u64>,
    total_chars: u64,
}

fn build_tool_stats(events: &[Event]) -> Vec<ToolStats> {
    let mut map: HashMap<String, ToolStats> = HashMap::new();

    for e in events {
        let entry = map.entry(e.tool.clone()).or_insert_with(|| ToolStats {
            tool: e.tool.clone(),
            n: 0,
            ok: 0,
            durations: Vec::new(),
            total_chars: 0,
        });
        entry.n += 1;
        if e.ok {
            entry.ok += 1;
        }
        entry.durations.push(e.duration_ms);
        entry.total_chars += e.result_chars;
    }

    let mut rows: Vec<ToolStats> = map.into_values().collect();
    rows.sort_by_key(|x| std::cmp::Reverse(x.n));
    for r in &mut rows {
        r.durations.sort_unstable();
    }
    rows
}

fn render_by_tool(events: &[Event], opts: &UsageOpts) -> Result<()> {
    let mut stats = build_tool_stats(events);
    stats.truncate(opts.top);

    match opts.format {
        OutputFormat::Json => {
            let rows: Vec<Value> = stats
                .iter()
                .map(|s| {
                    let p50 = percentile(&s.durations, 50);
                    let p95 = percentile(&s.durations, 95);
                    let avg_chars = if s.n > 0 {
                        s.total_chars / s.n as u64
                    } else {
                        0
                    };
                    serde_json::json!({
                        "tool": s.tool,
                        "n": s.n,
                        "ok_pct": pct(s.ok, s.n),
                        "p50ms": p50,
                        "p95ms": p95,
                        "avg_chars": avg_chars,
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&rows)?);
        }
        OutputFormat::Tsv => {
            println!("tool\tn\tok_pct\tp50ms\tp95ms\tavg_chars");
            for s in &stats {
                let p50 = percentile(&s.durations, 50);
                let p95 = percentile(&s.durations, 95);
                let avg_chars = if s.n > 0 {
                    s.total_chars / s.n as u64
                } else {
                    0
                };
                println!(
                    "{}\t{}\t{:.1}\t{}\t{}\t{}",
                    s.tool,
                    s.n,
                    pct(s.ok, s.n),
                    p50,
                    p95,
                    avg_chars
                );
            }
        }
        OutputFormat::Table => {
            if stats.is_empty() {
                println!("no usage events recorded yet");
                return Ok(());
            }
            println!(
                "{:<30} {:>6} {:>7} {:>7} {:>7} {:>10}",
                "tool", "n", "ok%", "p50ms", "p95ms", "avg_chars"
            );
            println!("{}", "-".repeat(72));
            for s in &stats {
                let p50 = percentile(&s.durations, 50);
                let p95 = percentile(&s.durations, 95);
                let avg_chars = if s.n > 0 {
                    s.total_chars / s.n as u64
                } else {
                    0
                };
                println!(
                    "{:<30} {:>6} {:>6.1}% {:>7} {:>7} {:>10}",
                    s.tool,
                    s.n,
                    pct(s.ok, s.n),
                    p50,
                    p95,
                    avg_chars,
                );
            }

            // cold tools
            let called: std::collections::HashSet<&str> =
                stats.iter().map(|s| s.tool.as_str()).collect();
            let cold: Vec<&str> = crate::commands::mcp::TOOL_NAMES
                .iter()
                .copied()
                .filter(|t| !called.contains(t))
                .collect();
            if !cold.is_empty() {
                println!();
                println!(
                    "cold tools: {}  (0 calls — discoverability gap)",
                    cold.join(", ")
                );
            }
        }
    }
    Ok(())
}

// ── Errors flavor ─────────────────────────────────────────────────────────────

fn render_errors(events: &[Event], opts: &UsageOpts) -> Result<()> {
    let mut errors: Vec<&Event> = events.iter().filter(|e| !e.ok).collect();
    // Most recent first (lexicographic ts desc).
    errors.sort_by(|a, b| b.ts.cmp(&a.ts));
    errors.truncate(opts.top);

    match opts.format {
        OutputFormat::Json => {
            let rows: Vec<Value> = errors
                .iter()
                .map(|e| {
                    serde_json::json!({
                        "ts": e.ts,
                        "tool": e.tool,
                        "args": e.args,
                        "duration_ms": e.duration_ms,
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&rows)?);
        }
        OutputFormat::Tsv => {
            println!("ts\ttool\tduration_ms\targs");
            for e in &errors {
                println!("{}\t{}\t{}\t{}", e.ts, e.tool, e.duration_ms, e.args);
            }
        }
        OutputFormat::Table => {
            if errors.is_empty() {
                println!("no error events found");
                return Ok(());
            }
            println!("{:<28} {:<26} {:>8}  args", "ts", "tool", "dur_ms");
            println!("{}", "-".repeat(90));
            for e in &errors {
                let args_str = e.args.to_string();
                let args_short = if args_str.len() > 35 {
                    format!("{}...", &args_str[..32])
                } else {
                    args_str
                };
                println!(
                    "{:<28} {:<26} {:>8}  {}",
                    e.ts, e.tool, e.duration_ms, args_short
                );
            }
        }
    }
    Ok(())
}

// ── Sequences flavor ──────────────────────────────────────────────────────────

fn render_sequences(events: &[Event], opts: &UsageOpts) -> Result<()> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for pair in events.windows(2) {
        let key = format!("{}→{}", pair[0].tool, pair[1].tool);
        *counts.entry(key).or_insert(0) += 1;
    }

    let mut pairs: Vec<(String, usize)> = counts.into_iter().collect();
    pairs.sort_by_key(|x| std::cmp::Reverse(x.1));
    pairs.truncate(opts.top);

    match opts.format {
        OutputFormat::Json => {
            let rows: Vec<Value> = pairs
                .iter()
                .map(|(seq, cnt)| serde_json::json!({"sequence": seq, "count": cnt}))
                .collect();
            println!("{}", serde_json::to_string_pretty(&rows)?);
        }
        OutputFormat::Tsv => {
            println!("sequence\tcount");
            for (seq, cnt) in &pairs {
                println!("{seq}\t{cnt}");
            }
        }
        OutputFormat::Table => {
            if pairs.is_empty() {
                println!("no transition sequences found (need at least 2 events)");
                return Ok(());
            }
            println!("{:<60} {:>8}", "sequence", "count");
            println!("{}", "-".repeat(70));
            for (seq, cnt) in &pairs {
                println!("{:<60} {:>8}", seq, cnt);
            }
        }
    }
    Ok(())
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn pct(part: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        part as f64 / total as f64 * 100.0
    }
}

fn comma(n: usize) -> String {
    // Simple thousands-separator formatting.
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out.chars().rev().collect()
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_jsonl(dir: &std::path::Path, lines: &[&str]) {
        let path = dir.join("usage.jsonl");
        let mut f = std::fs::File::create(&path).unwrap();
        for line in lines {
            writeln!(f, "{line}").unwrap();
        }
    }

    fn default_opts() -> UsageOpts {
        UsageOpts {
            by_tool: false,
            errors: false,
            sequences: false,
            summary: false,
            tool: None,
            since: None,
            top: 20,
            format: OutputFormat::Table,
        }
    }

    #[test]
    fn usage_empty_file_graceful() {
        // Non-existent usage.jsonl — summary handler must exit Ok, no panic.
        let dir = tempfile::tempdir().unwrap();
        let result = cmd_usage(
            dir.path(),
            UsageOpts {
                format: OutputFormat::Table,
                ..default_opts()
            },
        );
        assert!(result.is_ok(), "expected Ok, got {:?}", result);
    }

    #[test]
    fn usage_summary_counts_correct() {
        let dir = tempfile::tempdir().unwrap();
        let lines = &[
            r#"{"ts":"2026-05-01T10:00:00.000Z","tool":"resman_add_experiment","args":{"tag":"x"},"ok":true,"duration_ms":5,"result_chars":70}"#,
            r#"{"ts":"2026-05-01T10:01:00.000Z","tool":"resman_add_experiment","args":{"tag":"x"},"ok":true,"duration_ms":5,"result_chars":70}"#,
            r#"{"ts":"2026-05-01T10:02:00.000Z","tool":"resman_add_experiment","args":{"tag":"x"},"ok":true,"duration_ms":5,"result_chars":70}"#,
            r#"{"ts":"2026-05-01T10:03:00.000Z","tool":"resman_best","args":{},"ok":true,"duration_ms":3,"result_chars":144}"#,
            r#"{"ts":"2026-05-01T10:04:00.000Z","tool":"resman_distill","args":{"tag":"x"},"ok":true,"duration_ms":12,"result_chars":482}"#,
        ];
        write_jsonl(dir.path(), lines);

        // Load events and check funnel
        let path = dir.path().join("usage.jsonl");
        let events = load_events_inner(&path, None, None).unwrap();
        let funnel = build_funnel(&events);
        // Find the "x" tag entry
        let x_entry = funnel
            .iter()
            .find(|e| e["tag"] == "x")
            .expect("tag x missing");
        assert_eq!(x_entry["added"].as_u64().unwrap(), 3, "added should be 3");
        assert_eq!(
            x_entry["verified"].as_u64().unwrap(),
            0,
            "verified should be 0"
        );
        assert_eq!(
            x_entry["distilled"].as_u64().unwrap(),
            1,
            "distilled should be 1"
        );
    }

    #[test]
    fn usage_by_tool_latencies() {
        let dir = tempfile::tempdir().unwrap();
        let lines: Vec<String> = (1..=5)
            .map(|i| {
                format!(
                    r#"{{"ts":"2026-05-01T10:0{}:00.000Z","tool":"resman_best","args":{{}},"ok":true,"duration_ms":{},"result_chars":100}}"#,
                    i,
                    i * 10
                )
            })
            .collect();
        let line_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
        write_jsonl(dir.path(), &line_refs);

        let path = dir.path().join("usage.jsonl");
        let events = load_events_inner(&path, None, None).unwrap();
        let stats = build_tool_stats(&events);
        assert_eq!(stats.len(), 1);
        let s = &stats[0];
        assert_eq!(s.tool, "resman_best");
        let p50 = percentile(&s.durations, 50);
        let p95 = percentile(&s.durations, 95);
        assert_eq!(p50, 30, "p50 should be 30, got {p50}");
        assert_eq!(p95, 50, "p95 should be 50, got {p95}");
    }

    #[test]
    fn usage_errors_filter() {
        let dir = tempfile::tempdir().unwrap();
        let lines = &[
            r#"{"ts":"2026-05-01T10:00:00.000Z","tool":"resman_best","args":{},"ok":true,"duration_ms":3,"result_chars":100}"#,
            r#"{"ts":"2026-05-01T10:01:00.000Z","tool":"resman_best","args":{},"ok":true,"duration_ms":3,"result_chars":100}"#,
            r#"{"ts":"2026-05-01T10:02:00.000Z","tool":"resman_best","args":{},"ok":true,"duration_ms":3,"result_chars":100}"#,
            r#"{"ts":"2026-05-01T10:03:00.000Z","tool":"resman_verify","args":{"commit":"abc"},"ok":false,"duration_ms":5,"result_chars":50}"#,
            r#"{"ts":"2026-05-01T10:04:00.000Z","tool":"resman_verify","args":{"commit":"def"},"ok":false,"duration_ms":5,"result_chars":50}"#,
        ];
        write_jsonl(dir.path(), lines);

        let path = dir.path().join("usage.jsonl");
        let events = load_events_inner(&path, None, None).unwrap();
        let errors: Vec<&Event> = events.iter().filter(|e| !e.ok).collect();
        assert_eq!(errors.len(), 2, "should have exactly 2 error events");
    }

    #[test]
    fn usage_sequences_top() {
        let dir = tempfile::tempdir().unwrap();
        // 6 events alternating tool_a / tool_b
        let tools = ["tool_a", "tool_b", "tool_a", "tool_b", "tool_a", "tool_b"];
        let lines: Vec<String> = tools
            .iter()
            .enumerate()
            .map(|(i, t)| {
                format!(
                    r#"{{"ts":"2026-05-01T10:0{}:00.000Z","tool":"{}","args":{{}},"ok":true,"duration_ms":1,"result_chars":10}}"#,
                    i, t
                )
            })
            .collect();
        let line_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
        write_jsonl(dir.path(), &line_refs);

        let path = dir.path().join("usage.jsonl");
        let events = load_events_inner(&path, None, None).unwrap();

        let mut counts: HashMap<String, usize> = HashMap::new();
        for pair in events.windows(2) {
            let key = format!("{}→{}", pair[0].tool, pair[1].tool);
            *counts.entry(key).or_insert(0) += 1;
        }

        let ab = counts.get("tool_a→tool_b").copied().unwrap_or(0);
        let ba = counts.get("tool_b→tool_a").copied().unwrap_or(0);
        assert!(ab >= 2, "tool_a→tool_b should appear >= 2 times, got {ab}");
        assert!(ba >= 2, "tool_b→tool_a should appear >= 2 times, got {ba}");
    }

    #[test]
    fn usage_tool_filter() {
        let dir = tempfile::tempdir().unwrap();
        let lines = &[
            r#"{"ts":"2026-05-01T10:00:00.000Z","tool":"resman_best","args":{},"ok":true,"duration_ms":3,"result_chars":100}"#,
            r#"{"ts":"2026-05-01T10:01:00.000Z","tool":"resman_best","args":{},"ok":true,"duration_ms":4,"result_chars":110}"#,
            r#"{"ts":"2026-05-01T10:02:00.000Z","tool":"resman_search","args":{"pattern":"x"},"ok":true,"duration_ms":5,"result_chars":200}"#,
            r#"{"ts":"2026-05-01T10:03:00.000Z","tool":"resman_add_experiment","args":{"tag":"t"},"ok":true,"duration_ms":6,"result_chars":70}"#,
            r#"{"ts":"2026-05-01T10:04:00.000Z","tool":"resman_distill","args":{"tag":"t"},"ok":true,"duration_ms":12,"result_chars":400}"#,
        ];
        write_jsonl(dir.path(), lines);

        let path = dir.path().join("usage.jsonl");
        let events = load_events_inner(&path, Some("resman_best"), None).unwrap();
        assert_eq!(events.len(), 2, "tool filter should yield exactly 2 events");
        assert!(
            events.iter().all(|e| e.tool == "resman_best"),
            "all events should be resman_best"
        );
    }

    #[test]
    fn tag_funnel_counts_correct_tag() {
        use serde_json::json;
        let events = vec![
            Event {
                ts: "t".into(),
                tool: "resman_add_experiment".into(),
                args: json!({"tag": "x"}),
                ok: true,
                duration_ms: 1,
                result_chars: 0,
            },
            Event {
                ts: "t".into(),
                tool: "resman_add_experiment".into(),
                args: json!({"tag": "x"}),
                ok: true,
                duration_ms: 1,
                result_chars: 0,
            },
            Event {
                ts: "t".into(),
                tool: "resman_verify".into(),
                args: json!({"tag": "x"}),
                ok: true,
                duration_ms: 1,
                result_chars: 0,
            },
            Event {
                ts: "t".into(),
                tool: "resman_distill".into(),
                args: json!({"tag": "x"}),
                ok: true,
                duration_ms: 1,
                result_chars: 0,
            },
            // Different tag — must not count.
            Event {
                ts: "t".into(),
                tool: "resman_add_experiment".into(),
                args: json!({"tag": "other"}),
                ok: true,
                duration_ms: 1,
                result_chars: 0,
            },
        ];
        let f = tag_funnel(&events, "x");
        assert_eq!(f.added, 2);
        assert_eq!(f.verified, 1);
        assert_eq!(f.distilled, 1);
    }

    #[test]
    fn tag_funnel_ignores_unknown_tools() {
        use serde_json::json;
        let events = vec![
            Event {
                ts: "t".into(),
                tool: "resman_best".into(),
                args: json!({"tag": "x"}),
                ok: true,
                duration_ms: 1,
                result_chars: 0,
            },
            Event {
                ts: "t".into(),
                tool: "resman_search".into(),
                args: json!({"tag": "x"}),
                ok: true,
                duration_ms: 1,
                result_chars: 0,
            },
        ];
        let f = tag_funnel(&events, "x");
        assert_eq!(f.added, 0);
        assert_eq!(f.verified, 0);
        assert_eq!(f.distilled, 0);
    }

    #[test]
    fn build_funnel_output_unchanged_with_tag_funnel() {
        // Verify build_funnel still assembles correct JSON rows after refactor.
        use serde_json::json;
        let events = vec![
            Event {
                ts: "t".into(),
                tool: "resman_add_experiment".into(),
                args: json!({"tag": "a"}),
                ok: true,
                duration_ms: 1,
                result_chars: 0,
            },
            Event {
                ts: "t".into(),
                tool: "resman_verify".into(),
                args: json!({"tag": "a"}),
                ok: true,
                duration_ms: 1,
                result_chars: 0,
            },
            Event {
                ts: "t".into(),
                tool: "resman_distill".into(),
                args: json!({"tag": "b"}),
                ok: true,
                duration_ms: 1,
                result_chars: 0,
            },
        ];
        let funnel = build_funnel(&events);
        let a = funnel.iter().find(|e| e["tag"] == "a").unwrap();
        assert_eq!(a["added"].as_u64().unwrap(), 1);
        assert_eq!(a["verified"].as_u64().unwrap(), 1);
        assert_eq!(a["distilled"].as_u64().unwrap(), 0);
        let b = funnel.iter().find(|e| e["tag"] == "b").unwrap();
        assert_eq!(b["distilled"].as_u64().unwrap(), 1);
    }

    #[test]
    fn summary_json_cold_tools_lists_uncalled() {
        use serde_json::json;
        // Only resman_best and resman_add_experiment are "called"
        let events = vec![
            Event {
                ts: "t".into(),
                tool: "resman_best".into(),
                args: json!({}),
                ok: true,
                duration_ms: 1,
                result_chars: 0,
            },
            Event {
                ts: "t".into(),
                tool: "resman_add_experiment".into(),
                args: json!({"tag": "x"}),
                ok: true,
                duration_ms: 1,
                result_chars: 0,
            },
        ];
        let val = summary_json(&events);

        // total_events, ok, errors
        assert_eq!(val["total_events"].as_u64().unwrap(), 2);
        assert_eq!(val["ok"].as_u64().unwrap(), 2);
        assert_eq!(val["errors"].as_u64().unwrap(), 0);

        // cold_tools must be sorted and must not contain the two called tools
        let cold = val["cold_tools"].as_array().unwrap();
        let cold_strs: Vec<&str> = cold.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(
            !cold_strs.contains(&"resman_best"),
            "resman_best was called, must not be cold"
        );
        assert!(
            !cold_strs.contains(&"resman_add_experiment"),
            "resman_add_experiment was called, must not be cold"
        );
        // All remaining TOOL_NAMES should appear
        for tool in crate::commands::mcp::TOOL_NAMES {
            if *tool != "resman_best" && *tool != "resman_add_experiment" {
                assert!(cold_strs.contains(tool), "expected {tool} in cold_tools");
            }
        }
        // Verify sorted order
        let mut sorted = cold_strs.clone();
        sorted.sort_unstable();
        assert_eq!(cold_strs, sorted, "cold_tools must be sorted");
    }
}
