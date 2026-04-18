# Changelog

## [0.8.0] — Human-friendly terminal + HTML distill (2026-04-18)

v0.7 closed the agent-facing feature set (signals, distill, verify, composite).
v0.8 turns attention to the **human** sitting next to the agent — the person
who reads overnight results at 9am, shares a report with a manager, or debugs
why a run went sideways. Resman has always been a terminal-first tool; v0.8
makes the terminal a *nice place to be*, and promotes `distill` from a text
artifact to something you can email.

### Added (Wave A — terminal UX polish)
- **ANSI color output** on human-readable paths (`list`/`best -o table`/
  `compare -o table`/`distill` markdown/`verify` success). Status glyphs
  now colorize: `Keep` ✓ green · `Best` ★ bold cyan · `Discard` · dim ·
  `Crash` ✗ red · `Verified` ✔ bold green.
- **`--no-color` global flag** and **`NO_COLOR` env var** both disable
  color. Stdout-is-not-a-TTY defaults to no color (via stdlib
  `std::io::IsTerminal`, no new dep).
- **"Did you mean?" suggestions** on missing tags. `resman list --tag apr1`
  now prints `error: tag 'apr1' not found. Did you mean: apr17, apr18?`
  — prefix match first, Levenshtein ≤ 2 fallback. Hooked into `list`,
  `distill`, `verify`, `tree`, `diff`. Create-if-missing paths (`add`,
  `import`, `watch`) unchanged.
- **`long_about` help text** on `Init`/`Import`/`Add`/`ParseLog`/`List`/
  `Compare`/`Report`/`Export`/`Stats` — every subcommand now has a
  "when to use" sentence and, where meaningful, a one-line shell example.

### Invariants preserved
- `resman best -f value` output is **byte-identical** to v0.7 — a single
  float + newline, no ANSI, even on a TTY with color enabled. The public
  shell-script API is untouched.
- `-o json` and `-o tsv` outputs never contain ANSI escapes. Colors are
  table / markdown / human-readable stderr only.
- MCP server output (`src/commands/mcp.rs`) is untouched — agent-facing
  JSON-RPC stays structured and unambiguous.
- No new Cargo dependencies.

### Added (Wave B — distill --html)
- **`resman distill -t <tag> --html <out>`** — emits a self-contained,
  dark-mode HTML artifact (~5 KB, no JS, no CDN, no external images).
  The file you email your manager at 9am. Renders: summary badges,
  metric sparkline SVG, Best card, lineage list with status badges,
  failure-signal clusters in `<details>` collapsibles, unexplored-neighbor
  table, suggestions.
- **New `src/html.rs`** — shared dark-mode CSS, `html_escape`, `trend_svg`,
  `badge`/`BadgeKind`, and `page()` wrapper. `report.rs` refactored to
  use these helpers, eliminating CSS duplication (net −35 LOC there).
- `--html` is orthogonal to `-o`/`--out`: pass both to emit Markdown/JSON
  AND HTML in the same invocation. Writing status printed to stderr:
  `wrote HTML to {path}`.

### Added (Wave C — distill intelligence)
- **Verified-aware suggestions** in `resman distill`. When the best
  experiment of a tag is not Verified, distill now emits an actionable
  prompt like *"Best experiment is unverified — re-run and call
  `resman verify {commit}` before you rely on it."* When a run has
  ≥ 5 Keep/Best experiments with zero Verified, a stronger bulk
  prompt fires instead: *"No experiments have been verified yet…"*
  These are the first suggestions in the list — they're louder than
  the heuristic "lots of OOMs" type advice.
- **`resman distill --all`** — cross-run aggregation. Answers the
  9am question *"what happened across every tag overnight?"*. Renders:
  totals, top-5 failure signals globally (with example entries from
  any tag), top-3 tags ranked by best metric value (direction-aware
  per tag's own `effective_direction`), and cross-run Verified /
  failure-concentration suggestions. Markdown by default; `-o json`
  for downstream tools. `--out <path>` writes to file. Mutually
  exclusive with `--tag` (enforced at CLI parse time).

### Explicitly deferred (not in v0.8)
- **`Signal::DivergedLoss` / `Signal::SlowMfu`** — thresholds need
  real log corpus to tune; premature without usage data. Planned v0.9.
- **Composite-weight tuning** — v0.7's `0.5 / 0.2 / 0.2 / 0.1` weights
  stay. Tune once we have data on how agents actually rank.
- **MCP tool for `distill --all`** — single-tag `resman_distill`
  stays the primary agent surface; cross-run aggregation is a
  human-facing 9am report for now.
- **HTML render for `distill --all`** — not needed in v0.8. Markdown
  and JSON only for cross-distill.

## [0.7.0] — Reproducibility + composite scoring

v0.6 gave agents structured failure signals. v0.7 gives them
**reproducibility as a first-class property** plus a multi-dim "which
experiment should I resume from?" ranker. Two additions:

1. A new `Status::Verified` that can only be set via `resman verify`
   after a successful reproduction.
2. An opt-in `resman best --composite` that blends metric quality with
   verification status, lineage depth, and description richness.

### Added
- **`Status::Verified`** — a seventh status variant. Cannot be set
  manually via `add -s verified` (the CLI rejects it with a clear
  error); only `resman verify` can promote an experiment into this
  state. Preserves the "verified means actually re-run" invariant.
- **`resman verify <commit> --value <new_value> [--tolerance 0.01] [--tag <t>]`**
  — directional, tolerance-based promotion. For Minimize runs, new
  must be ≤ original + tolerance; for Maximize, new must be ≥ original
  − tolerance. On pass: status → Verified and val_bpb is updated to
  the new measurement. On fail: stored record untouched, print a clear
  "not verified" summary (exit 0 — a failed reproduction is a
  legitimate result, not an error). Re-verify of an already-Verified
  experiment is allowed (re-reverify). Crash experiments are refused
  (nothing to reproduce). Accepts short-hash prefixes; ambiguous
  matches error with the candidate list.
- **`resman_verify` MCP tool** — same inputs, same text body. Intended
  to be called by the agent harness after a reproduction run.
- **`resman best --composite`** — opt-in multi-dim scoring. Formula:
  `0.5 × metric + 0.2 × verified + 0.2 × lineage + 0.1 × desc`. Every
  subscore in [0, 1]:
  - `metric` = run-local normalization of val_bpb by direction
  - `verified` = Verified 1.0 · Best 0.5 · Keep 0.3 · Discard/Crash 0.0
  - `lineage` = min(depth/5, 1.0) where depth walks `parent_commit`
    back to a root
  - `desc` = min(len/80, 1.0)
  Weights are fixed in v0.7 (tune in v0.8 once we have usage data).
  Tiebreak on metric, then insertion order.
- **`composite: true` on `resman_best` MCP tool** — same behavior. The
  MCP `instructions` field names it as the preferred "resume-from-here"
  selector.

### Changed
- `resman best` default path is **byte-identical to v0.6** — no
  scoring runs unless `--composite` is passed. Existing shell scripts
  calling `resman best -f value` keep working unchanged. This is a
  non-negotiable invariant; see CLAUDE.md.
- When `--composite` is set, `-f table` appends a subscore breakdown,
  `-f json` adds a `composite` object, `-f tsv` appends five columns,
  `-f value` prints the composite score as the single float.

### Not in scope (deferred)
- Exposing composite weights on the CLI. They're hardcoded in v0.7 —
  we'll tune them with real user runs before v0.8.
- `Status::Verified` in `resman distill` (the status glyph is
  updated; the suggestion-rules don't yet weight verified runs
  specially).
- `DivergedLoss` and `SlowMfu` signal variants — still deferred to
  v0.8 alongside multi-pass log parsing.

### Migration
None. v0.3–v0.6 JSON stores load unchanged. The `status` field accepts
`verified` as an input when decoding but the CLI is the only producer.

## [0.6.1] — CI hotfix

v0.6.0's tag push failed CI at the `cargo fmt --check` gate
(`src/commands/distill.rs` was committed unformatted). No release
artifacts were produced under `resman-v0.6.0`, so v0.6.1 is the
effective first public v0.6 release. No code changes beyond running
`cargo fmt` on `distill.rs` — logic, tests, and CLI/MCP surface
identical to the v0.6.0 tree.

## [0.6.0] — Typed crash signals

v0.3 added `crash_excerpt` — a raw log tail stored on crashes. Useful as
evidence, but every query like "how many OOMs did we get overnight?" still
required a regex in the agent's head. v0.6 converts those tails into a
structured `Vec<Signal>` so the store is *indexed* by failure mode, not just
*annotated*.

### Added
- **`Signal` enum** with six variants: `Oom`, `CudaError { hint }`,
  `NanLoss`, `AssertFail { location }`, `Timeout`, `Unknown { pattern }`.
  Serialized as tagged JSON (`{"type": "oom"}`). Field-variants carry
  just enough context for triage without bloating the store.
- **`signals::classify(tail: &str) -> Vec<Signal>`** — regex-based,
  order-matters (OOM matched before `CudaError` so a CUDA OOM doesn't
  double-count). Always returns ≥ 1 signal; the `Unknown` fallback
  captures the last non-empty line for forensic later.
- **`Experiment.signals`** field, additive, `skip_serializing_if` Vec
  empty so v0.5 and earlier JSON loads unchanged.
- **`resman add --log <path>`** — now runs the classifier on the tail
  regardless of status (a `keep` experiment can still have signal-worthy
  log patterns; though crash_excerpt storage is still crash-only).
- **`resman list --signal <type>`** — filters to experiments whose
  signals include the requested kind. Repeatable, AND-semantics across
  multiple values. Unknown names fail at the CLI boundary with a
  helpful enumeration.
- **`resman_find_by_signal` MCP tool** — agent-callable equivalent of
  the CLI filter. Returns experiment summaries with per-signal context
  (hint for CudaError, location for AssertFail, pattern for Unknown).
- **`log_tail` on `resman_add_experiment`** — MCP callers can pass the
  last ~50 lines of their run.log directly; resman classifies
  server-side and attaches the signals atomically with the record.
- **`resman distill -t <tag>`** — emits a structured Markdown summary
  of a run: best result, lineage chain to best, failure-signal
  clusters, unexplored neighbors (top-3 runs that almost beat best),
  and a short list of mechanical heuristic suggestions (e.g. "≥3 OOMs —
  consider reducing batch size"). Template-rendered, no LLM
  dependency. First concrete form of the "agent long-term memory"
  artifact that v0.8 will generalize.
- **`resman_distill` MCP tool** — same payload in Markdown or JSON.
  The MCP `instructions` now names it as the preferred end-of-session
  summary tool.
- `resman distill -o json` — structured output for programmatic
  consumption; same section shape as the Markdown.

### Not in scope (deferred)
- `DivergedLoss` and `SlowMfu` variants (require multi-pass parsing +
  workload-specific thresholds; need more data to tune defaults).
- `resman stats --by-signal` breakdown — coming with the v0.7 composite
  scoring work.
- `resman distill` — experimental first pass lands next.

## [0.5.0] — Schema generalization: resman is no longer just for val_bpb

Before v0.5 the primary metric was hard-coded to `val_bpb`, which made sense
for karpathy nanoGPT but quietly excluded every other agent-training workload
(LoRA → `eval_loss`, RL → `mean_return`, diffusion fine-tune → `clip_score`,
anything eval-accuracy-based → a higher-is-better metric). This release
generalizes the metric name and direction without breaking any v0.3/v0.4 data.

### Added
- **`Direction` enum** (`minimize` | `maximize`). Accepted on the CLI as
  `min`/`max`/`minimize`/`maximize`/`lower`/`higher`.
- **`metric_name` / `metric_direction` fields** on `Experiment` and `RunLog`.
  Both optional, both `#[serde(default, skip_serializing_if = "Option::is_none")]`
  — pre-v0.5 JSON stores load unchanged.
- **Effective-name cascade**: `experiment.metric_name` → `run.metric_name` →
  `"val_bpb"`. Same cascade for direction, defaulting to `Minimize`.
- **`--metric-name <str>` / `--metric-direction <min|max>`** flags on
  `resman add` and `resman import`. First-set-wins: the run's defaults are
  fixed at the first `add` that creates the tag.
- MCP `resman_add_experiment` input schema gained `metric_name` /
  `metric_direction`. MCP initialize `instructions` mentions both.

### Changed
- `RunLog::best()` now honors `effective_direction()` — picks the max when
  the run is a Maximize one. The v0.4 `val_bpb > 0.0` safety filter only
  applies under Minimize, so a legitimate `accuracy=0` is never silently
  dropped under Maximize.
- `best`, `list`, `compare`, and the MCP `tool_best` / `tool_list_recent`
  text outputs display the effective metric name in place of the literal
  `val_bpb` label. When a table mixes multiple names, the label is `metric`.
- `resman best -f value` still prints just the float — unchanged,
  still a public shell-script API.

### Migration
None required. Every pre-v0.5 run continues to behave as a Minimize run with
metric name `val_bpb`. Opt into the new world by passing `--metric-name` /
`--metric-direction` on the first `resman add` for a new tag.

## [0.4.0] — Infrastructure Week (diff, tree, one-line install)

Two agent-facing analysis commands land, plus the plumbing to actually ship a
binary people can install in five seconds. This is the first release in the
12-week road to v1.0 ("memory layer for agent training loops"); see
`STRATEGY.md` for the full plan.

### Added
- **`resman diff <tagA> <tagB>`** — config/metric diff between the
  representative experiment of two runs. `--against best|latest`, three output
  formats. Answers "why did this branch win?" in one command instead of a
  two-jq-pipeline hack. Mirrored as `resman_diff_tags` MCP tool.
- **`resman tree -t <tag>`** — renders the lineage forest of a run via
  `parent_commit` links. ASCII tree for humans (with ★ on the best-lineage
  chain), JSON for agents, TSV with a `depth` column. Cycle-safe. Mirrored as
  `resman_lineage` MCP tool. Finally makes the v0.3 `parent_commit` field
  a first-class object.
- **`install.sh`** — one-line install for Linux/macOS, detects OS+arch and
  pulls the prebuilt binary from the latest GitHub Release. Customize via
  `RESMAN_INSTALL_DIR` / `RESMAN_VERSION`.

### Changed
- README install sections (both root and crate) now lead with `curl | sh`,
  then `cargo install resman`, then source. Previously the only path was
  "install Rust + cargo install --path ." which lost ~80% of the install
  funnel at the toolchain-install step.
- Unit test count 7 → 15 (+8 for diff and tree paths).

## [0.3.0] — Agent-native features (informed by upstream community signal)

After studying the top-voted issues & PRs on `karpathy/autoresearch`, three pain
points kept recurring: (1) "has the agent already tried this?" (#47, #418, #80);
(2) "save crash context, not just a 'crash' status" (#101, bd75534); (3) "let
agents talk to tools natively, not through bash" (#98, MCP). This release
addresses all three.

### Added
- **`resman mcp`** — minimal Model Context Protocol server over stdio. Exposes
  five tools (`resman_best`, `resman_search`, `resman_near`, `resman_list_recent`,
  `resman_add_experiment`) to Claude Code / Cursor / Codex / any MCP-speaking
  harness. See `docs/MCP.md`.
- **`resman search <regex>`** — case-insensitive search across every experiment's
  description, commit, and params. Answers "has the agent tried this before?".
- **`resman near <val_bpb>`** — list the N experiments whose val_bpb is closest
  to a target. Grounds new results against neighbors.
- **`resman add --log run.log`** — on crash, siphon the last 50 log lines into
  `Experiment.crash_excerpt`. The raw log can then be deleted.
- **`resman add --parent <commit>`** — record the experiment's parent commit.
  Enables future lineage/tree commands.
- **`resman add` auto-probes `nvidia-smi`** for GPU name and attaches it as
  `params.gpu` (skip with `--no-gpu-probe`). Responds to upstream PR #102.

### Changed
- `Experiment` gained two optional fields (`parent_commit`, `crash_excerpt`),
  both `skip_serializing_if = "Option::is_none"` — schema stays clean for old
  records. Backwards-compatible: v0.2 JSON stores load unchanged in v0.3.

## [0.2.0] — Pivot to product

Repositioned from an internal autoresearch helper to a standalone product: **a local-first experiment tracker for AI-agent training loops**.

### Added
- `resman add` — append a single experiment to a run. No TSV required. Designed to be called from inside an agent loop (`resman add -t $TAG -c $(git rev-parse --short HEAD) -v $BPB -s keep -d "$IDEA"`).
- `resman best` — print the single best experiment. `-f value` emits only the `val_bpb` float, so shell scripts can do `BEST=$(resman best -f value)`.
- `resman watch` — poll a `results.tsv`; auto re-import on every mtime change. For overnight agent sessions.
- `--format json|tsv|table` on `list`, `compare`, `best`. JSON is canonical for agents piping to `jq`.
- `--tag` flag on `list` / `stats` / `best` to scope queries to a single run.
- `$RESMAN_HOME` and `$XDG_DATA_HOME` precedence for the data dir.
- `resman import --force` to allow overwriting an existing tag (needed by `watch`).
- Stable JSON schema (`RunLog` / `Experiment`) with `serde(default)` for forward-compat.
- 7 unit tests; `cargo clippy -- -D warnings` clean.

### Changed
- **Status is now a typed enum** (`Keep`/`Discard`/`Crash`/`Best`) instead of a free-form string. Typos fail at CLI parse time, not silently at analysis time.
- **All errors are typed** (`thiserror`) and propagated via `Result`. No `.unwrap()` in non-test code.
- **Atomic writes** — all `save_run` calls write to `<tag>.json.tmp` then rename. An agent crashing mid-write cannot corrupt the store.
- Redesigned HTML report: dark mode, responsive, tabular-numeric fonts, inline SVG chart (no JS, no CDN).
- `parse-log` scales to any number of regexes without code duplication.

### Fixed
- `compare` ignored `--data-dir` and always read from `~/.resman`. Now respects the flag.
- TSV import silently dropped rows with `val_bpb` parse errors. Now surfaces a typed `InvalidFloat` error with line/column.
- `truncate` could split UTF-8 char boundaries and produce invalid strings.

### Removed
- The `.unwrap()`-happy single-path code from 0.1.
- Implicit "any JSON in runs/ works" loader — now emits a warning for malformed files but continues.

---

## [0.1.0]

Initial release. Internal tool for importing `results.tsv` from karpathy/autoresearch overnight runs. Single-status-as-string model, no error types, `compare` had a data-dir bug.
