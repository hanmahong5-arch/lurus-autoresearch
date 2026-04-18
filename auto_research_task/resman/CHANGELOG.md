# Changelog

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
