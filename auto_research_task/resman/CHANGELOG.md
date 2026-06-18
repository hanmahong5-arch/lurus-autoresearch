# Changelog

## [0.17.9] — MCP composite parity (2026-06-18)

A final audit round found the last instance of the recurring "fixed the CLI,
missed the MCP mirror" pattern. No schema change; `best -f value` unchanged.

### Fixed

- **`resman_best` MCP tool composite scoring diverged from the CLI.** The MCP
  `tool_best` composite path still used a single global min/max + the first
  candidate's direction (the pre-0.17.4 logic), so in a multi-run workspace with
  mixed metrics/directions it could pick a different — wrong — winner than
  `resman best --composite`. It now calls the same `composite_winner` the CLI
  uses (per-run normalization), so the two cannot diverge again.

### Tests

295 (unchanged): the MCP path now delegates to the already-tested
`composite_winner`; the now-internal `composite_candidates` is `#[cfg(test)]`.
Clippy `--all-targets` clean, fmt clean.

---

## [0.17.8] — maximize fixes in report / near / distill chart (2026-06-18)

A second completeness audit found three more spots still assuming "lower is
better", all in code the first audit didn't cover. `-o json`/`-o tsv` schema and
`best -f value` unchanged.

### Fixed

- **HTML report (`resman report`) was direction-blind.** It filtered `val_bpb > 0`
  and computed `improvement = worst - best` (negative for maximize → shown as
  "—"). It now uses the run's effective direction: keeps 0.0 for maximize, reports
  the positive `(best - worst)` magnitude, and shows the correct best.
- **`resman_near` MCP tool dropped maximize zero-values.** It filtered
  `val_bpb > 0` unconditionally — the CLI `near` was fixed in 0.17.5 but its MCP
  mirror was missed. Now applies the `> 0` guard only for minimize, per run.
- **distill HTML sparkline dropped maximize zero-points.** Same `> 0` guard, now
  direction-aware.

### Tests

295 (was 293): +2 (sparkline maximize/minimize). The report/near fixes mirror
already-tested siblings (stats/near). Clippy `--all-targets` clean, fmt clean.

---

## [0.17.7] — closeout fixes: maximize stats + unverify TOCTOU (2026-06-18)

A completeness audit after the v0.17.4–v0.17.6 hardening surfaced four real bugs
the earlier passes missed — all fixed here. `-o json`/`-o tsv` schema and
`best -f value` unchanged; maximize-metric `stats` numbers change (they were
wrong before).

### Fixed

- **`stats` was broken for maximize metrics** in three ways, now all corrected:
  (1) `improvement` was `worst - best` → negative for maximize, making the
  percent negative and `improvement_rate` clamp to 0; it is now the magnitude
  `(best - worst).abs()`. (2) The value filter dropped legitimate `0.0` values
  for maximize (it applied the minimize `> 0` guard unconditionally); `0.0` is
  now kept for maximize. (3) Direction was read only from the first experiment's
  override, ignoring the run-level `metric_direction`; `stats` (CLI and the
  `resman_stats` MCP tool) now uses the full run→experiment→default cascade.
- **`unverify` could mutate the wrong experiment under concurrent writes.** Like
  the `verify` TOCTOU fixed in 0.17.4, it computed an index from one store
  snapshot then reloaded; a concurrent `add` made the index stale. `unverify` now
  re-locates the target by commit after reload (the stale index is gone entirely).

### Tests

293 (was 289): +4 (maximize improvement sign, maximize zero-value inclusion,
unverify relocation ×2). Clippy `--all-targets` clean, fmt clean.

---

## [0.17.6] — usability & robustness polish (2026-06-18)

Onboarding, error guidance, and defensive hardening from the audit's usability
findings. No change to `-o json`/`-o tsv` schema or `best -f value`.

### Added

- **First-run quickstart.** Bare `resman` now ends its help with an `init → add
  → best -f value` quickstart so a new user knows `init` comes first.
- **`doctor` flags leftover temp files.** A crash mid-write can leave a
  `<tag>.json.tmp`; `doctor` now counts them and warns (safe to delete) instead
  of leaving them invisible.

### Changed

- **Actionable error messages.** `Error::Empty` and `Error::NotFound` now point
  at `resman init` for the fresh-store case (previously a dead end).
- **Mixed-metric aggregation warning.** `stats` and `compare` without `--tag`
  aggregate across all runs; when those runs use different metric names or
  directions they now print a one-line stderr warning that the cross-run numbers
  may not be comparable (use `--tag` to scope). Stdout is unchanged.
- **Complete HTML escaping (defense-in-depth).** `html_escape` now also escapes
  `"` and `'`. No user text currently lands in an HTML attribute, so this is not
  an active fix — it makes the helper correct for future attribute use; the
  rendered page is unchanged.

### Tests

289 (was 281): +8. Clippy `--all-targets` clean, fmt clean.

---

## [0.17.5] — direction-aware correctness sweep (2026-06-18)

The metric generalization (v0.5) left several paths still assuming "lower is
better". This makes the rest of the codebase honor `metric_direction`, plus two
parsing/classification fixes. `-o json`/`-o tsv` schema and `best -f value` are
unchanged; table/Markdown VALUES change where they were wrong for maximize metrics.

### Fixed

- **`stats` reported best/worst inverted for maximize metrics** (best was the
  minimum). Best/worst are now direction-aware. The improvement-rate denominator
  was the total experiment count (incl. crashes/discards); it is now the count of
  scored (kept, finite) experiments, matching "per experiment".
- **`list` default sort ignored direction**, so a maximize run listed its worst
  result first. The default metric sort is now best-first per direction
  (`--reverse` still flips it). Order only — JSON array and TSV header unchanged.
- **`near` and `distill` unexplored-neighbors dropped legitimate maximize
  zero-values.** The `val_bpb > 0` guard now applies only to Minimize; for
  Maximize, 0.0 (e.g. accuracy at init) is valid.
- **`distill` stagnation count over-counted.** "Runs since last improvement"
  included crashes/discards; it now counts only kept experiments.
- **OOM mis-classification.** The signal regex matched any line containing
  "GiB total capacity", so a benign CUDA memory-stats line was tagged `oom`. The
  pattern now requires a genuine out-of-memory indicator.
- **wandb import treated incomplete runs as kept.** Any non-final state
  (`running`, `stopped`, …) mapped to `keep` with metric 0.0. Now `finished` →
  keep; crashed/failed/killed/preempted → crash; other states → discard.

### Tests

281 (was 267): +14 regression tests across the fixes. Clippy `--all-targets`
clean, fmt clean.

---

## [0.17.4] — reliability hardening (2026-06-18)

A correctness + robustness pass from a three-angle reliability audit (panic
paths, wrong-output bugs, edge-case robustness). No change to `-o json` /
`-o tsv` schema, table layout, or `best -f value`; the one behavior change is
that `best --composite` now ranks mixed-direction stores correctly (below).

### Fixed

- **`best --composite` mis-ranked stores that mix metrics/directions.** It
  normalized every candidate on a single global min/max using the first
  candidate's direction, so e.g. the *worst* experiment of a maximize-accuracy
  run could score as the global best against a minimize-bpb run. Each candidate
  is now normalized within its OWN run (own [min,max] + own direction), making
  the metric component a comparable [0,1] "how good within its run" score. The
  scoring + selection is extracted to a testable `composite_winner`.
- **`diff` reported improvements as regressions for maximize metrics.** The
  `regression` flag (table label + JSON field) hardcoded "delta > 0 = worse";
  it is now direction-aware (for Maximize, a lower value is the regression).
- **`RunLog::best()` could select a non-finite metric.** A `NaN`/`±inf`
  `val_bpb` (e.g. a hand-edited or pre-guard store) compares as Equal under
  `partial_cmp` and could win — especially for Maximize, which has no `> 0`
  filter. Non-finite metrics are now excluded from selection.
- **`verify` could panic under concurrent writes (TOCTOU).** The experiment
  index was computed from one store snapshot, then the run was reloaded; a
  concurrent `add` between the two reads left the stale index pointing out of
  bounds. `verify` now re-locates the target by commit after reload and errors
  cleanly if it has vanished.
- **TSV output could be corrupted by embedded tabs/newlines.** Descriptions
  (which `import` preserves verbatim) are now sanitized in every `-o tsv` field
  (tab/newline/CR → space) so a value can't inject extra columns or rows. JSON,
  table, the TSV header, and `best -f value` are unchanged.

### Tests

267 (was 261): +6 regression tests, one per fix above. Clippy `--all-targets`
clean, fmt clean.

---

## [0.17.3] — JSON wire-format guards (2026-06-17)

Locks the agent-facing JSON output contracts so a refactor can't silently rename
or drop a field. No behavior change — binary output is byte-identical to 0.17.2;
this release is test coverage only.

### Tests

- **6 new integration tests** in `tests/cli.rs` (Test 10–15) pin the JSON shape
  of `best -f json`, `list -o json`, `compare -o json`, `tags -o json`,
  `doctor -o json`, and `distill -o json`. They assert key presence + type
  (never values), so they fire only on a genuine wire-format break, not on data
  changes. Closes the "JSON shapes not pinned" gap from the 0.17.2 audit and is
  groundwork for the v1.0 schema freeze. 261 total (246 unit + 15 integration).
  Clippy `--all-targets` clean, fmt clean.

---

## [0.17.2] — scorecard hardening (2026-06-17)

Follow-ups from an industrial-grade audit: an installer security fix, a
supply-chain CI gate, and an error-variant correctness fix. No change to any
`-o json` / `-o tsv` / `best -f value` output.

### Fixed

- **`Direction::from_str` reported through the wrong error variant.** A bad
  metric direction surfaced as `Error::InvalidStatus`, mislabeling it as a status
  parse error. Added a dedicated `Error::InvalidDirection`; the message is now
  `invalid direction: <value> (expected min|max)`.

### Security

- **`install.sh` no longer silently downgrades to an unverified install.** A
  failed checksum fetch previously fell through to "older release; skipping
  integrity check", so a transient network error — or a tampered mirror dropping
  the `.sha256` — would install an unverified binary. Now only a definitive HTTP
  404 (a release that genuinely predates checksums) skips; any other failure
  (network/TLS/5xx) aborts unless `RESMAN_ALLOW_UNVERIFIED=1` is set. An
  empty/malformed checksum file, or a missing sha256 tool, also aborts.

### CI

- **New `resman-audit` workflow** runs `cargo audit` on dependency changes and
  weekly, surfacing RustSec advisories. Deliberately decoupled from the release
  workflow so an externally-timed advisory can never block shipping a fix.

### Tests

255, unchanged (the Direction fix swaps an error variant; no assertion covered
the old text). Clippy `--all-targets` clean, fmt clean.

---

## [0.17.1] — code-review fixes (2026-06-17)

Fixes from an extra-high-effort multi-angle review of the v0.16–v0.17 UI/UX work.

### Fixed

- **Table alignment with color on.** `term::status_cell` padded the *colored*
  status label with `{:<8}`, which counts the ANSI escape sequence toward the
  width — so `best`-status rows lost their padding and `list`/`compare`/`best`
  tables went ragged whenever color was enabled (the interactive default). The
  plain word is now padded first, then colored.
- **`best` description-row alignment.** The key/value column width didn't cover
  the longest label (`description:`, 12 chars), so the description value
  misaligned for any metric name shorter than 11 chars (both the default and
  `--composite` table paths). Width now floors at the longest label and counts
  chars, not bytes.
- **distill HTML `verified` badge undercount.** The HTML header counted verified
  experiments from the best-lineage subset only, while every other badge — and
  the Markdown — used the run-wide summary. Verified experiments off the best
  path were silently missed. Now uses `summary.verified`, matching Markdown.
- **Panic hardening (char-safe slicing).** `short_commit` and the `usage
  --errors` arg preview truncated by byte index, which panics mid-codepoint on a
  non-ASCII commit/arg value. Both now slice on char boundaries (the latter via
  the shared `store::truncate`).

### Tests

255 (presentation/edge fixes; existing suite stays green). Clippy
`--all-targets` clean, fmt clean. All `-o json`/`-o tsv`/`best -f value` output
unchanged.

---

## [0.17.0] — unified terminal presentation (2026-06-17)

The second half of the comprehensive UI/UX pass: one consistent visual
language across every command's human/table output, via a shared `term.rs`
vocabulary.

### Changed

- **All 7 table commands** (`list`, `compare`, `stats`, `best`, `tags`,
  `doctor`, `usage`) now share: a uniform `=== resman <cmd> (<context>) ===`
  header (carrying useful context — experiment / run / tag / event counts), a
  single 80-char rule, consistent glyph+label status cells, one empty-state
  tone, and shared description-truncation widths (`DESC_TRUNC` 48 /
  `DESC_TRUNC_NARROW` 30). Previously each command invented its own header
  style, separator width (96/97/72/90/70/52/none), status rendering, and
  empty-state phrasing.
- `best` key/value labels now align for ANY metric name (alignment was
  hardcoded for the 7-char `val_bpb`).

### Fixed

- Error tag-quoting unified to backticks (`TagNotFound` used single quotes).
- `distill --all` markdown footer showed a hardcoded `v0.8`; now reports the
  real crate version (the single-run path was already fixed in v0.15.3).

### Added

- `--help` examples for `near` / `diff` / `tree` / `verify` / `tags` /
  `unverify` (previously example-less).

### Internal

- Dead ANSI color helpers removed; the reserved colors are now wired
  (`bold`→headers, `cyan`→status) or deleted. Zero `#[allow(dead_code)]` in
  `term.rs`.

### Tests

250 → **255**. Clippy `--all-targets` clean, fmt clean. Every `-o json` /
`-o tsv` output and `best -f value` remain byte-stable (only the human/table
branches changed).

---

## [0.16.2] — cross-run distill HTML (2026-06-17)

### Added

- **`resman distill --all --html <file>`** now emits a self-contained, themed
  HTML page for the cross-run summary — previously `--html` was silently ignored
  in `--all` mode. It mirrors the single-run HTML via the shared
  `page()`/`section()`/`data_table()`/`badge()` components: status-count badges,
  a "Top tags by best metric" table, failure-signal clusters (`<details>`), and
  suggestions — all dual-theme. Plain `distill --all` (markdown/JSON) is
  unchanged.

### Tests

248 → **250** (`render_cross_html_self_contained` + `_empty`). Clippy
`--all-targets` clean, fmt clean.

---

## [0.16.1] — themed trend chart (2026-06-16)

### Changed

- `trend_svg` (the chart in `resman report` and `resman distill --html`) now
  draws with the design tokens — `var(--accent)` line/points, `var(--border)`
  grid, `var(--muted)` axis labels — so it themes with the page (light / dark /
  system) instead of being locked to dark Nord. Added a subtle area fill under
  the line, per-point `<title>` hover tooltips (commit + value), and a themed
  "no data" empty state. This removes the last hardcoded hex from HTML output.

### Tests

247 → **248** (a labels+area-fill chart test). Clippy `--all-targets` clean,
fmt clean. `trend_svg` stays JS-free and self-contained (no external refs).

---

## [0.16.0] — dual-theme HTML design system (2026-06-16)

Visual overhaul of the self-contained HTML artifacts (`resman report`,
`resman distill --html`) — the first slice of a comprehensive UI/UX pass.

### Added

- **Dual-theme design tokens.** `src/html.rs` now defines a full set of CSS
  custom properties driving **light + dark + follow-system**
  (`prefers-color-scheme`), with `:root[data-theme="dark"|"light"]` override
  hooks reserved for a future `--theme` flag. Dark Nord stays the default
  brand; the light palette meets WCAG AA on white.
- **Reusable component builders** in `src/html.rs`: `stat_card`, `stats_grid`,
  `section`, `data_table`, `empty` — `report` and `distill --html` now share one
  structural vocabulary instead of duplicated inline markup.

### Changed

- `resman report` is rendered via the shared `page()` shell (was a hand-rolled
  duplicate) — guarantees a single `<style>`/`<footer>` and drops the stray
  `·`-vs-`&middot;` footer mismatch. The HTML builder is extracted as the pure
  `render_report_html()` for testability.
- `CSS_DARK` renamed `CSS` (it now carries both themes).

### Fixed

- The `.detail` CSS class (referenced by distill branch verdicts) was undefined;
  it is now styled. Empty states use a `.empty` class instead of inline `style=`.

### Tests

237 → **247** (5 component-builder tests + 4 `render_report_html` smoke tests,
which `report` previously had zero of). Clippy `--all-targets` clean, fmt clean.
All HTML self-containment invariants (single `<style>`, no external refs,
escaping, DOCTYPE) preserved; `best -f value` and json/tsv byte-stable.

---

## [0.15.4] — doctor path display on Windows (2026-06-16)

Found by dogfooding `resman doctor` on a Windows host.

### Fixed

- `resman doctor` printed the data dir with Windows' `\\?\C:\...`
  verbatim/extended-length prefix (from `canonicalize`), inconsistent with
  every other line. Stripped for display via a `strip_verbatim_prefix`
  helper (cosmetic only — the writability probe path is unchanged; no-op on
  non-Windows paths).

---

## [0.15.3] — distill accuracy polish (2026-06-16)

Found by dogfooding the flagship `distill` artifact against a realistic
lineage + signals + verify store.

### Fixed

- **Header counts now sum to the total.** The single-tag distill header
  omitted `verified` experiments — "5 experiments (2 crashes, 2 keep, 0
  discard, 0 best)" summed to 4. Added a `verified` count to
  `DistillSummary` and the header line. (The `--all` aggregate already
  counted verified.)
- **Unexplored neighbors no longer list the best experiment itself.**
  The filter excluded `status == Best`, but the best is selected by
  metric value and is usually `keep`/`verified`, so it appeared as its
  own neighbor with Δ=0. Now excluded by commit identity.
- **Footer version is no longer hard-coded `v0.6`** — it reflects the
  actual crate version via `CARGO_PKG_VERSION`.

---

## [0.15.2] — CI green: clippy --all-targets (2026-06-16)

Build-only fix; the shipped binary matches the v0.15.1 intent exactly.

### Fixed

- A **test-only** clippy lint (`unnecessary_get_then_check`:
  `.get(k).is_none()` → `!contains_key(k)`) failed CI's
  `cargo clippy --release --all-targets -- -D warnings` gate, which lints
  test code. v0.15.0 and v0.15.1 passed `cargo clippy --release` locally
  (that does *not* lint tests) but their release CI failed at clippy, so
  neither published binaries. v0.15.2 is the first 0.15.x to publish.

---

## [0.15.1] — import error UX (2026-06-16)

Error-message-only patch; no happy-path behaviour change.

### Improved

- **Missing `--metric` now lists detected metric columns.** `parse_wandb` and
  `parse_mlflow` accept `Option<&str>` and, when `None`, parse the CSV header
  and emit a helpful message: numeric non-meta candidates for wandb (capped at
  12), `metrics.*`-prefixed columns (prefix stripped) for mlflow, with a
  concrete `--metric <first-candidate>` hint. The existing "column not found"
  error for a wrong `--metric` is unchanged.
- **Default tsv path detects a CSV file.** `cmd_import` on `ImportSource::Tsv`
  checks the first non-empty line: if it contains `,` and no `\t` it returns
  `Err(Import(...))` explaining the format mismatch and pointing to `--from
  wandb` / `--from mlflow`. Valid TSV files (always tab-containing) are
  unaffected.

### Tests

234 → **236 passing** (`wandb_missing_metric_lists_columns`,
`mlflow_missing_metric_lists_columns`, `tsv_source_detects_csv_file`,
`tsv_source_valid_tsv_still_works`; old `require_metric_*` tests removed —
`require_metric` helper deleted). Clippy 0 warnings, fmt clean.

---

## [0.15.0] — W&B and MLflow CSV import (2026-06-16)

Extends `resman import` with a `--from <source>` selector supporting W&B and
MLflow CSV exports. The new zero-dependency `src/csv.rs` RFC-4180 reader handles
quoted fields, embedded commas, embedded newlines, `""` escaped quotes, and both
`\n`/`\r\n` line endings. Default `--from tsv` behavior is byte-identical to v0.14.

### Added

- **`resman import --from wandb --metric <col> -t <tag>`** — imports a W&B
  "Export runs" CSV. Maps `State` to status (`finished`→keep, `crashed/failed/
  killed/preempted`→crash). Commit is the `ID` col, description from `Notes` or
  `Name`. Extra columns become `params.*`. Empty metric cells default to 0.0;
  unparseable cells emit a per-row warning and use 0.0 (import never aborts).
- **`resman import --from mlflow --metric <col> -t <tag>`** — imports an MLflow
  `search_runs` CSV. Accepts `--metric loss` or `--metric metrics.loss` (prefix
  resolved automatically). `run_id`→commit, `tags.mlflow.runName`→description,
  `params.*` prefix stripped into the params map, `FAILED/KILLED`→Crash.
- **`src/csv.rs`** — RFC-4180 char-by-char state machine; no new crate
  dependencies. Six unit tests cover all edge cases.
- **`examples/wandb-export.csv`** and **`examples/mlflow-export.csv`** — realistic
  4-row fixtures used in integration tests and the smoke test above.
- `ImportSource` value enum (`tsv`/`wandb`/`mlflow`) in `cli.rs`.
- `Error::Import(String)` variant in `error.rs`.

### Improved

- **Actionable empty states.** `stats`, `compare`, `export`, and `report` now
  tell a first-time user what to do when the store is empty (e.g. "no
  experiments to summarize yet — add or import some first (`resman add ...` or
  `resman import <file>`).") instead of a dead-end "no experiments found." —
  matching the guidance `list`/`tags`/`usage` already gave.

### Unchanged

- Default `--from tsv` path: code path is identical to v0.14; shell-script API
  (`resman best -f value`) preserved.
- No new MCP tool added (CSV import is a setup-time ingestion operation;
  `resman_add_experiment` remains the agent-facing write path).

### Tests

211 → **234 passing** (csv.rs edge cases + require_metric + parse_wandb +
parse_mlflow + end-to-end import fixtures). Clippy 0 warnings, fmt clean.

---

## [0.14.0] — Full MCP parity for query commands (2026-06-16)

Four new MCP tools give the query CLI commands a first-class agent-facing
surface. `TOOL_NAMES` is now the single source of truth for all 17 tools,
fixing the stale cold-tools list in `resman usage` (was 13, now 17).
A manifest↔dispatch drift-guard test catches future mismatches at CI time.

### Added — MCP tools

- **`resman_list`** — filtered, sorted experiment list (status/signal/grep/top/reverse/tag/sort_by). Richer than `resman_list_recent`; backed by the shared `filter_sort_truncate` helper.
- **`resman_compare`** — per-run best-experiment table; optional tag substring filter. Backed by `compare_summary`.
- **`resman_stats`** — aggregate counts (kept/discarded/crashed) + val_bpb best/worst/mean/stddev/improvement. Backed by `compute_stats`/`pct`.
- **`resman_usage`** — telemetry summary from `usage.jsonl`: totals, per-tag adoption funnel, and cold tools. Backed by `load_events`/`summary_json`.

### Fixed

- `TOOL_NAMES` const (single source of truth) now lists all 17 tools; cold-tool
  detection in `resman usage` and the new `resman_usage` MCP tool is accurate.

### Tests

199 → **~212 passing** (5 new handler tests + drift-guard). Clippy 0 warnings.

---

## [0.13.3] — Classifier hardening + test rigor (2026-06-16)

Tier 3 of the audit follow-up: systematic signal-classifier coverage, stronger
tests, and supply-chain hardening of the installer. No schema changes.

### Improved — signal-classifier coverage

Common real-world failure phrasings that previously fell through to `unknown`
are now classified, each with a dedicated test (plus a negative test asserting
normal log lines stay signal-free):

- **Oom** — PyTorch allocator line (`… GiB total capacity`, no "out of memory")
  and explicit NCCL OOM.
- **CudaError** — cuBLAS (`CUBLAS_STATUS_*`), cuDNN (`CUDNN_STATUS_*`),
  `device-side assert triggered`, and generic `ncclInternalError` (a generic
  NCCL failure is a CUDA-stack error, *not* an OOM).
- **NanLoss** — AMP `Gradient overflow detected` / `GradScaler … overflow`.
- **DivergedLoss** — `loss: -inf`, the HF-Trainer dict form `'train_loss': inf`,
  Megatron `lm_loss: inf`, and grad-norm inf.
- **SlowMfu** — HuggingFace `mfu=15.3%` / `MFU: 15.3` (plus existing
  `mfu_percent:`).
- **Timeout** — SLURM `DUE TO TIME LIMIT`, `subprocess.TimeoutExpired`,
  `SIGALRM`.

### Hardened — tests

- Replaced a tautological `term.rs` color test (it asserted on a hardcoded
  `if false`) with one that calls `paint()` and checks real output.
- The `store.rs` round-trip proptest now generates all `Status` variants and a
  full `val_bpb` range (0.0 … 1e6), not just `Status::Keep`.
- Added a `Status` wire-format guard (every variant → its exact lowercase
  string) and locked `best -f value` to the exact six-decimal byte format.

### Hardened — supply chain

- Release CI now publishes a `.sha256` per binary; `install.sh` verifies it when
  present (aborting on mismatch) and still installs older pre-checksum releases
  with a warning. Fully backward-compatible.

### Tests

176 → **199 passing** (0 failed). Clippy 0 warnings.

---

## [0.13.2] — Audit hardening (2026-06-16)

Fixes from a comprehensive 5-domain adversarial audit (MCP parity, correctness,
signal/distill robustness, test rigor, docs/supply-chain). No schema changes —
all prior stores load unchanged; the `best -f value` byte-stable API is untouched.

### Fixed — MCP (the primary agent interface)

- `resman_find_by_signal` now exposes all **8** signal kinds — it was missing
  `diverged_loss` and `slow_mfu`, so agents in schema-validating harnesses could
  not filter by them. The enum is now derived from `signals::ALL_KINDS` so it can
  never silently drift from the implementation again.
- `resman_add_experiment` returns **structured JSON**
  (`{recorded, tag, commit, val_bpb, status, lineage_warning}`) instead of a prose
  ack string — honoring the server's own "all tools return JSON" contract.
- A malformed `tools/call` (missing tool name) now returns a proper JSON-RPC
  `-32602` invalid-params error instead of an `isError` prose result.
- The MCP serve loop exits cleanly on a client-disconnect write error instead of
  spinning silently.

### Fixed — correctness & robustness

- **Reject non-finite `val_bpb`** (NaN / ±inf) at both CLI `add` and MCP add — a
  stored NaN poisoned `best` selection (`partial_cmp` treats NaN as Equal).
- `signals::classify` AssertFail location now reports the **deepest** traceback
  frame (the actual assert site), not the outermost.
- The `distill` reproducibility-gap suggestion no longer fires when a tag has only
  crashes (nothing is verifiable) — removes false-positive noise.
- `default_data_dir` no longer **silently** falls back to the cwd when no home env
  is set — it now warns. `verify` replaced two non-test `.unwrap()`s with
  propagated errors.

### Packaging & docs

- Removed the unused `anyhow` dependency; added `rust-version = "1.85"`.
- Fixed MCP tool-count drift (MCP.md 10→13, AGENT_QUICKSTART 12→13, README
  +`resman_tags`); refreshed the stale roadmap to v0.13.x; documented
  `best --composite -f value` semantics.

### Tests

169 → **176 passing** (0 failed). New coverage: MCP enum/JSON/-32602/non-finite,
assert-deepest-frame, distill all-crash suppression, NaN rejection,
verify-not-found, data-dir precedence. Clippy 0 warnings.

---

## [0.13.1] — NaN-loss classifier fix (2026-06-15)

Patch release fixing a signal-classification gap surfaced by dogfooding the
`distill` pipeline on a realistic store.

### Fixed

- **`nan_loss` signal classifier** now matches two extremely common real-world
  PyTorch NaN signatures that previously fell through to `unknown`:
  space-separated `loss nan` (no "is"/colon) and the autograd anomaly
  detector's `returned nan values in its Nth output`. Broadened the
  `signals.rs::classify` regex and regression-tested with the exact strings
  that slipped through. Improves `distill` failure-cluster accuracy and
  `resman list --signal nan_loss` / `find_by_signal` recall.

### Test counts

**169 tests** (count unchanged; `detects_nan_loss` gained three real-world
cases). Clippy 0 warnings.

---

## [0.13.0] — Feedback loop + crates.io rename (2026-06-15)

Closes the usage-data feedback loop that makes `distill` and the composite
scorer self-improving, and publishes the crate to crates.io for the first
time.

### Added — usage-aware distill

- **Reproducibility-gap suggestion**: `distill` reads `usage.jsonl` and emits
  an actionable suggestion when a tag shows many adds but zero verifies
  (`added >= 10 && verified == 0`): *"Tag has N logged experiments but none
  have been verified — consider re-running the best and calling
  `resman verify`."* CLI + MCP aggregate via a `tag_funnel` pass over the
  usage log. Graceful no-op when `usage.jsonl` is absent — output is
  byte-identical to v0.12.0 in that case.
- **CLI usage logging**: `add`, `verify`, `unverify`, `import`, and `distill`
  now write one JSONL line to `usage.jsonl` on every invocation (same
  `RESMAN_DISABLE_USAGE_LOG=1` opt-out as the MCP path; local-only; failures
  stderr-once, never block the command). The hot `best -f value` path is
  deliberately NOT logged so shell-script latency is unchanged.
  Previously only `resman mcp` calls were recorded; CLI loops are now first-
  class participants in the feedback dataset.

### Packaging

- Published to crates.io as **`resman-cli`** (binary remains `resman`).
  The crate name `resman` is taken by an unrelated project.
  `cargo install resman-cli` → installs the `resman` command.

### Test counts

146 → **169 tests** (160 unit + 9 CLI). Clippy 0 warnings.

---

## [0.12.0] — Per-tag snapshot probe (2026-05-16)

A targeted polish release: one new command + matching MCP tool that fills
a gap between "per-experiment list" and "aggregate stats".

### Added

- **`resman tags`** + MCP tool **`resman_tags`** — per-tag snapshot. One
  row per tag: experiment_count, best_commit, best_value, metric_name,
  direction, last_update, schema_version. Sorted by last_update desc
  (most-recently touched first). Three output formats; MCP returns a
  JSON array.

  Why: `resman_list_recent` is per-experiment; `resman stats` is
  cross-tag aggregate. Neither answers the higher-level "what tags do
  I have and what is each one's headline?" question that agents and
  humans both want as a follow-on to `resman_doctor`.

### Test counts

142 → **146 tests** (140 unit + 6 CLI). +4 covering empty store /
single-tag fields / sort-by-last_update / long-metric-name table render.
Clippy 0 warnings.

### Invariants preserved

- `resman_tags` exposes only fields already in RunLog — no schema change.
- The thirteen MCP tools (was twelve in v0.11) all return parseable JSON.

## [0.11.0] — Agent narrative + onboarding (2026-05-16)

v0.10 closed the verify ↔ unverify cycle. v0.11 makes the distill output
read like a narrative an agent can act on, brings the MCP entrypoint
prompt up to date with the full tool surface, and provides a single
agent-facing onboarding doc.

### Added — distill narrative

- **"Other branches" section** with verdict labels. Every non-best
  branch root gets one line: `<root> → … → <terminal> [status]
  depth=N verdict={converged|broke|abandoned}` plus a signal-kind note
  for broke verdicts. The agent reads which alternate directions were
  tried and how each ended without re-walking every experiment.
- **HTML parity** — `distill --html <out>` now renders the same
  Other-branches section using the existing badge palette
  (converged=green, broke=red, abandoned=gray).
- **Cross-tag continuation links** — when this tag's root experiment
  has a parent_commit that lives in another tag, the distill header
  surfaces `_continues from \`yesterday\` (commit \`bbb222d\`)._`. Sews
  together overnight sessions where each tag inherits from the prior.
  Pure detection via `find_continuation(run, all_runs)`; cmd_distill
  and MCP `resman_distill` both populate it.
- `DistillReport.branch_verdicts: Vec<BranchVerdict>` and
  `continues_from: Option<ContinuationLink>` are both `serde(default)`
  so v0.6–v0.10 JSON consumers ignore the new fields safely.

### Added — onboarding

- **`docs/AGENT_QUICKSTART.md`** — single-page agent-facing doc. Section 0
  is `resman_doctor` first call; sections 1-6 walk the session lifecycle
  with per-tool guidance and the lineage-warning + tolerance precision;
  section 7 states the one-sentence contract ("log first, reset last").
- **`initialize_result.instructions` (MCP)** rewritten to walk the full
  twelve-tool lifecycle and cross-reference `docs/AGENT_QUICKSTART.md`.
  This is the prompt the LLM sees before any tool call.

### Added — industrial reliability

- **`bench_load_all_runs_1000_experiments_50_tags`** — `#[ignore]`'d test
  measuring load time on 50 tags × 20 experiments. Validates the
  README's "loads in milliseconds" claim (~5ms on a recent NVMe).
  Invoke with `cargo test --release -- --ignored --nocapture`.

### Test counts

133 → **138 tests** (132 unit + 6 CLI). Plus the new ignored perf bench
(opt-in). Clippy 0 warnings.

### Invariants preserved

- `branch_verdicts` is additive — existing distill JSON consumers see a
  new optional array; nothing else changes.
- Tag-prefix convention `resman-v*` reaffirmed in tags. The existing
  `.github/workflows/resman.yml` triggers release artifact builds on
  these tags across linux/macos/windows.

### Explicitly deferred (still not in v0.11)

- **`val_bpb` / `memory_gb` rename** — SCHEMA.md decision unchanged.
- **Composite-weight tuning** — gated on real `usage.jsonl` corpus.
- **Cross-tag continuation links in distill** — wait for a dogfood
  session to confirm the pattern matters.
- **Stagnation suggestion in HTML** — currently markdown only; HTML
  has been a "look at this in the morning" surface, so the markdown
  read-on-MCP path covers the agent.

## [0.10.0] — Agent UX symmetry + reliability validation (2026-05-16)

v0.9 hardened the storage and protocol layer. v0.10 closes the symmetry
gaps in the agent surface (verify ↔ unverify), adds the highest-leverage
distill intelligence (stagnation, keep-but-reverted), validates the
concurrent-write invariant with tests instead of just docs, and makes the
CLI feel native via shell completions.

### Added — agent surface

- **`resman unverify <commit>`** + MCP tool **`resman_unverify`** —
  symmetric retraction of `resman verify`. Reverts a Verified experiment
  back to Keep when the verified result turns out to be a fluke (later
  re-runs disagree, criterion too lenient). The val_bpb stays at the
  verify-time value — retraction is about trust, not metric. Closes the
  verify ↔ unverify cycle: agents can promote reproductions to verified,
  and walk that label back when evidence changes.

### Added — distill intelligence

Two new heuristic suggestion patterns in `distill::suggest`:

- **Stagnation detector** — fires when a tag has ≥10 experiments and ≥8
  consecutive kept runs haven't advanced the rolling best. Reports the
  anchor commit + val_bpb so the agent can revisit a non-best lineage
  branch or pivot to a radically different direction. Direction-aware.

- **Keep-but-reverted detection** — when a `keep` experiment is the
  lineage ancestor of a strictly-better `verified` descendant on the
  same `parent_commit` chain, surface as an under-explored direction
  with "re-combine the kept idea with verified-tier insights" hint.
  Walks ancestors with a 50-hop cycle guard.

Both are pure pattern detection over existing fields — no schema changes,
no LLM dependency.

### Added — CLI polish

- **`resman completions <shell>`** (bash / zsh / fish / powershell /
  elvish) via `clap_complete`. Tab completion now covers every
  subcommand, every flag, and clap-known enums (status, format, signal
  types). Install: `source <(resman completions bash)`.

### Added — industrial reliability tests

- **Concurrent-write integrity tests** in `store::tests`:
  - 8 threads × 3 rounds writing 8 distinct tags → every tag's last
    write survives, no corruption, all schema_version=1.
  - 8 threads × 5 rounds writing the *same* tag → last-writer-wins
    semantics, but the atomic tmp+rename invariant guarantees the
    on-disk file is always parseable JSON.

  These elevate the README's "safe to run from 10 concurrent loops"
  claim from prose to a deterministic test.

### Test counts

129 → **133 tests** (127 unit + 6 CLI). +4 unverify tests + 4 from prior
distill work covered in the same window. Clippy 0 warnings. Builds with
the same dependency set (proptest dev-only, clap_complete is the only
production-side addition in v0.10).

### Invariants preserved

- `resman best -f value` still byte-identical to v0.7.
- All v0.1–v0.9 JSON-on-disk stores load unchanged (`schema_version`
  defaults to 1 via serde).
- `cargo install resman` still produces one static binary; `clap_complete`
  is the sole new transitive crate.

### Explicitly deferred (not in v0.10)

- **`val_bpb` / `memory_gb` rename** — SCHEMA.md decision still stands;
  single dedicated PR after first dogfood session.
- **Verified-anchored lineage rendering in distill** — visual polish that
  belongs with a distill HTML re-pass; queued for v0.11.
- **`.github/workflows/release.yml`** — release binary CI; queued for
  v0.11 once a real dogfood session exists to validate against.

## [0.9.0] — Industrial-grade agent memory layer (2026-05-16)

v0.7-v0.8 built the surface: signals, verified status, composite scoring,
terminal UX. v0.9 hardens it for production as **the** memory layer agents
rely on across overnight sessions — structured JSON everywhere, one-call
self-probe, schema-version lock, property-tested store, two more typed
crash signals, and full per-MCP-call telemetry.

### Added — agent surface

- **`resman doctor`** — six-check health probe (data_dir writable,
  `RESMAN_HOME` resolution, runs present, `usage.jsonl` activity,
  `.mcp.json` discoverable, store invariants). Returns ok/warn/fresh/fail
  + actionable hint per check, three output formats, exit 1 on any fail.
  Exposed as MCP tool `resman_doctor` so agents self-probe at session
  start — one call replaces a dozen exploratory probes.
- **`resman usage`** — analyse `usage.jsonl` telemetry. Four flavors
  (`--summary` / `--by-tool` / `--errors` / `--sequences`), three formats,
  `--tool` / `--since` / `--top` filters. Streaming line-by-line read,
  graceful on empty/missing file. `docs/MCP.md` jq stub block replaced
  with a 10-recipe library (R1–R10) covering latency/error/adoption/
  dissent/transition analyses.
- **`Signal::DivergedLoss { detail }`** + **`Signal::SlowMfu { mfu_percent }`** —
  the two variants deferred since v0.6. `classify(tail)` detects
  `loss=inf` (NanLoss still wins on `loss=nan`) and `mfu_percent < 20`.

### Changed — MCP tools all return structured JSON

Eight tools converted from prose to JSON-string output:
`resman_best` (plain + composite breakdown), `resman_search`, `resman_near`,
`resman_find_by_signal`, `resman_diff_tags`, `resman_lineage`, `resman_verify`,
and `resman_list_recent` (was the first, shipped earlier in the cycle).
`resman_distill` already supported `format=json`. `resman_add_experiment`
keeps its short ack — and now appends `warning: ... lineage chain broken`
when `parent_commit` is omitted on a tag with prior experiments, so agents
detect lineage breaks at write time instead of silently producing a
disconnected history.

CLI commands (`cmd_best`, `cmd_search`, …) unchanged — humans still get
tables and friendly text.

### Added — per-call MCP telemetry

- **`usage.jsonl`** is written by `resman mcp` on every `tools/call` —
  one JSONL line: `{ts, tool, args, ok, duration_ms, result_chars}`.
  Source of truth for "which agents call which tools, with what success
  rate" — the dataset that will tune composite weights and distill
  templates before v1.0 schema freeze. Opt out with
  `RESMAN_DISABLE_USAGE_LOG=1`. Failures stderr-once, never block a call.

### Added — schema durability

- **`RunLog.schema_version: u32`** (v0.8 stores backfill to 1 via serde
  default). Bump only on incompatible changes; readers should silently
  ignore higher values.
- **`docs/SCHEMA.md`** — field-level v1.0 freeze decisions. Composite
  weights frozen on hardcoded `0.5 / 0.2 / 0.2 / 0.1` (no data to tune
  yet). `val_bpb` → `primary_metric` and `memory_gb` → `peak_memory_gb`
  rename deferred to a dedicated post-dogfood PR (50+ refs; will ship
  with `#[serde(alias = …)]` for v0.1–v0.9 store compatibility). No
  `deny_unknown_fields` (forward-compat over typo guard). No
  `resman migrate` (serde alias covers the rename scope).
- **proptest save/load roundtrip** — random tags + 0–5 experiments
  through `save_run` / `load_run`, all fields preserved. Plus an explicit
  legacy-store test (hand-written v0.7-shape JSON loads with
  `schema_version=1`).

### Added — `auto_research_task/program.md` protocol upgrade

The autoresearch agent's protocol document, mandating resman MCP as the
canonical long-term memory:

- New "Memory layer (resman MCP) — read before you act" section with the
  10-tool usage table mapping situation → tool → purpose.
- Setup step 5 rewritten: `resman_list_recent` is the discovery probe;
  `total === 0` is the structured fresh-store signal (was a fragile
  substring match on English prose).
- Experiment loop: consult `resman_search` / `resman_best --composite`
  before coding; `resman_add_experiment` after every run; near-best
  triggers `resman_verify`; every 10 runs `resman_distill`.
- Verify tolerance semantics tightened: explicit "absolute, direction-
  sensitive, default 0.01" with worked val_bpb examples covering both
  sides of the boundary (was "within ~1% directionally" — misleading
  for any metric not already near val_bpb's ~1.0 magnitude).
- Step 9 makes "log to resman before `git reset`" explicit — reset
  destroys the commit from `git log`; resman is the only place the
  attempt is remembered.
- `.mcp.json` shipped at the autoresearch repo root so Claude Code /
  Cursor pick up the resman MCP server out of the box.

### Test counts

87 → **124 tests** (118 unit + 6 CLI). New: 9 list_recent/parent-warning
tests, 11 MCP structured JSON tests, 10 doctor tests, 6 usage tests, 1
proptest, 1 legacy-store check, 4 signal tests. Clippy 0 warnings.

### Invariants preserved

- `resman best -f value` still byte-identical to v0.7 (single float +
  newline, no ANSI even on TTY).
- All v0.1–v0.8 JSON-on-disk stores load unchanged.
- `cargo install resman` still produces one static binary, no runtime,
  no new dependencies in production deps (proptest is dev-only).

### Explicitly deferred (not in v0.9)

- **`val_bpb` / `memory_gb` rename** — single dedicated PR after first
  dogfood session; aliases preserve all prior stores.
- **Composite-weight tuning** — wait for usage.jsonl signal.
- **`resman_unverify`** — symmetric retraction. Add when a real
  reproduction-of-reproduction case appears.
- **Stagnation detector** — flag tags with N runs and no improvement
  in distill output. v1.0 nice-to-have.

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
