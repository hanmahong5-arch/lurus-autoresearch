# resman

**Local-first experiment tracker for autonomous AI training agents.**

Built for the era of coding agents that run 100 experiments overnight.
Zero config, no account, no cloud. One Rust binary. Git-native. Machine-readable.

---

## Why another tracker?

`wandb`, `mlflow`, `neptune` were designed when a human logged experiments one at a time. That world is ending. The new workload looks like this:

```
  an AI agent runs 12 experiments/hour, 100+ overnight,
  each ending with a machine-readable summary,
  deciding on its own whether to keep or discard.
```

What that workload actually needs:

| Need | Existing tools | resman |
|---|---|---|
| Start tracking in < 1s | Login, project, run init | `resman init` |
| Read "what's the current best?" from a shell script | SDK + network call | `resman best -f value` |
| Zero network, zero account | ❌ | ✅ |
| Git-commit-based identity | ❌ | ✅ |
| Append from CI / cron / agent with one CLI call | ❌ (needs SDK) | `resman add ...` |
| Self-contained HTML report (email, share, archive) | Mostly web UI only | `resman report out.html` |
| One static binary, no runtime | Python + deps | Rust, ~3 MB |

This is a *different product category* than cloud experiment trackers — not a replacement, a complement.

---

## Install

**Prebuilt binary** (recommended — Linux / macOS):

```bash
curl -fsSL https://raw.githubusercontent.com/hanmahong5-arch/lurus-autoresearch/master/auto_research_task/resman/install.sh | sh
```

Detects your OS+arch, pulls the latest release from GitHub, drops a ~3 MB binary into `~/.local/bin`. Customize with `RESMAN_INSTALL_DIR=/usr/local/bin` or `RESMAN_VERSION=v0.17.8`. Each release also ships a `.sha256` the installer verifies automatically; a failed or missing verification aborts the install (override with `RESMAN_ALLOW_UNVERIFIED=1`).

**From crates.io** _(once `resman-cli` is published — until then use the prebuilt binary above or build from source)_:

```bash
cargo install resman-cli   # installs a `resman` command
```

**From source**:

```bash
git clone https://github.com/hanmahong5-arch/lurus-autoresearch
cargo install --path lurus-autoresearch/auto_research_task/resman    # Rust 1.85+
```

Windows users: prebuilt binary on Releases, or build from source above.

## 30-second tour

```bash
resman init                                                    # ~/.resman/

# Option A — import an agent-written TSV
resman import results.tsv -t apr17

# Option B — append one experiment at a time (no TSV needed)
resman add -t apr17 -c $(git rev-parse --short HEAD) \
           -v 0.9921 -m 44.2 -s keep -d "increased LR to 0.04" \
           -p lr=0.04 -p optim=muon

# Query from scripts / agent loops
BEST=$(resman best --format value)       # → 0.992100

# Human views
resman list --top 10
resman compare -o table
resman stats
resman report report.html                # self-contained dark-mode HTML

# Live mode during overnight runs
resman watch results.tsv -t apr17 -i 2   # re-imports on every change
```

## Commands

| Command | Purpose |
|---|---|
| `init [path]` | Create data directory (`$RESMAN_HOME` / `$XDG_DATA_HOME/resman` / `~/.resman`). |
| `import <file>` | Bulk-import experiments. `-t <tag>` names the run; `-f` overwrites. Default reads a `results.tsv`; `--from wandb\|mlflow --metric <col>` ingests a wandb/mlflow CSV export. |
| `add -t <tag> -c <commit> -v <bpb> …` | Append one experiment. Auto-probes `nvidia-smi` for GPU; `--log run.log` captures crash context; `--parent <commit>` records lineage. |
| `search <regex>` | "Has this been tried?" — regex across every description, commit, and param. |
| `near <val_bpb>` | Show N experiments whose val_bpb is closest to a target — grounds a new result. |
| `parse-log '<glob>'` | Extract metrics from `run.log` files (val_bpb, MFU, steps, VRAM, …). |
| `list` | Show experiments. Filter by `--status`, `--tag`, regex `--grep`; sort; `-o json\|tsv`. |
| `best` | Print the single best experiment. `-f value\|json\|table`. |
| `compare [tag…]` | One row per run, best-of-run highlights. `-o json\|tsv\|table`. |
| `stats` | Mean, stddev, crash rate, bpb-drop-per-experiment. |
| `report out.html` | Self-contained HTML with SVG trend chart. No JS, no CDN. |
| `export out.json` | Dump the full store as JSON. |
| `watch <tsv>` | Poll a TSV; auto re-import on mtime change. |
| `mcp` | Run as an MCP server over stdio — agents call tools directly. See [docs/MCP.md](docs/MCP.md). |
| `doctor` | Six-check health probe (data dir, env, runs, usage telemetry, MCP wiring, store invariants). One call replaces a dozen exploratory probes. `-o table\|json\|tsv`. |
| `tags` | Per-tag snapshot — one row per tag (count, best, metric, last_update). Sorted most-recent first. The "what tags do I have?" answer. `-o table\|json\|tsv`. |
| `usage` | Analyse `usage.jsonl` MCP telemetry. `--summary` / `--by-tool` / `--errors` / `--sequences`; `--tool`/`--since`/`--top` filters; three formats. |
| `verify` / `unverify` | Promote a reproduced experiment to `verified` (within tolerance, direction-aware) — and retract it back to `keep` when evidence later disagrees. The trust label is symmetric. |
| `completions <shell>` | Generate tab-completion script for bash / zsh / fish / powershell / elvish. `source <(resman completions bash)`. |

Global flags: `-D, --data-dir <path>` overrides the data dir for any command.

## Migrating from wandb / mlflow

resman is a complement, not a rip-and-replace — bring your existing history
with you. Both importers read a **CSV export** (no API key, no network, no
Python): in wandb, *Export to CSV* from a runs table; in mlflow,
`mlflow.search_runs().to_csv("runs.csv")`.

```bash
# wandb: pick which metric column is your primary objective
resman import wandb-export.csv --from wandb --metric 'eval/loss' -t apr17

# mlflow: the metrics. / params. prefixes are handled for you
resman import runs.csv --from mlflow --metric loss -t apr17
```

Run `State`/`status` maps to keep/crash, config columns become searchable
`params`, and the rest of resman (`best`, `distill`, `search`, MCP) works
immediately. `--metric` is required because these tools log many metrics and
resman won't guess your objective. The CSV reader is hand-rolled and zero-dep —
quoted fields, embedded commas, and newlines all parse correctly.

## The agent loop it was built for

```bash
BASELINE=$(resman best --tag $TAG -f value 2>/dev/null || echo "999")
# agent edits train.py, runs training...
NEW_BPB=$(grep "^val_bpb:" run.log | awk '{print $2}')
COMMIT=$(git rev-parse --short HEAD)

if (( $(echo "$NEW_BPB < $BASELINE" | bc -l) )); then
  resman add -t $TAG -c $COMMIT -v $NEW_BPB -m 44.0 -s keep -d "$IDEA"
  git commit --allow-empty -m "autoresearch: $IDEA → $NEW_BPB"
else
  resman add -t $TAG -c $COMMIT -v $NEW_BPB -m 44.0 -s discard -d "$IDEA"
  git reset --hard HEAD~1
fi
```

All IO is idempotent, atomic-write, and safe to run from 10 concurrent loops (different `--tag`s).

## Data layout

```
$RESMAN_HOME/ (default: ~/.resman/)
  runs/
    apr17.json          # one JSON per run, atomic-written
    apr18.json
```

The JSON schema is stable. Fields are explicitly versioned in `model.rs::RunLog`. `serde` defaults mean adding fields is non-breaking for older stores.

## Design choices

- **Local files, not a DB.** A million experiments fits in a few MB of JSON and loads in milliseconds. SQLite was considered and rejected — git-diffable JSON is more debuggable and enables trivial sync strategies.
- **Atomic writes (tmp + rename).** An interrupted `resman add` during an overnight crash cannot corrupt the store.
- **Status is a typed enum**, not a string. Typos fail at the CLI boundary, not silently at analysis time.
- **Three output formats everywhere** (`table`/`json`/`tsv`). Tables for humans, JSON for agents, TSV for spreadsheets.
- **No locks.** Per-run files mean concurrent agents writing different tags never contend.

## Agent integration via MCP

Add to `.claude/mcp.json` (Claude Code) or `~/.cursor/mcp.json`:

```json
{ "mcpServers": { "resman": { "command": "resman", "args": ["mcp"] } } }
```

Now the agent gets **seventeen structured tools** out of the box, all returning
parseable JSON (no substring match on English prose):

- `resman_doctor` — one-shot health probe, call this first.
- `resman_list_recent` — discovery probe; `total === 0` ⇒ fresh store.
- `resman_distill` — long-term-memory artifact (best, lineage, signals,
  unexplored neighbors, suggestions). Call at end of session.
- `resman_best` — current baseline. `composite: true` for the multi-dim
  resume-from-here score. `resman best --composite -f value` prints the composite score (0–1); plain `best -f value` prints val_bpb.
- `resman_search` — "has this idea been tried?" before wasting compute.
- `resman_near` — neighbors of a target val_bpb for grounding.
- `resman_find_by_signal` — failure triage by typed crash kind (oom,
  cuda_error, nan_loss, assert_fail, timeout, diverged_loss, slow_mfu).
- `resman_diff_tags`, `resman_lineage` — branch- and chain-level analysis.
- `resman_add_experiment` — log every run (keep, discard, crash).
  Returns `lineage chain broken` warning when `parent_commit` is missed
  on a non-fresh tag.
- `resman_tags` — per-tag snapshot: count, best, metric, last_update per tag.
- `resman_verify` / `resman_unverify` — promote a reproduced experiment
  to `status=verified`, and walk that label back when evidence disagrees.
- `resman_list` — filtered, sorted experiment list (status, signal, grep,
  top, sort) — richer than `resman_list_recent`.
- `resman_compare` — best-of-each across runs, side by side.
- `resman_stats` — aggregate counts + val_bpb best/worst/mean/stddev.
- `resman_usage` — telemetry summary: totals, per-tag adoption funnel, and
  cold (never-called) tools.

Every `tools/call` is logged to `$RESMAN_HOME/usage.jsonl` for `resman usage`
analysis. Opt out with `RESMAN_DISABLE_USAGE_LOG=1`.

- Agent-first onboarding: [docs/AGENT_QUICKSTART.md](docs/AGENT_QUICKSTART.md)
- MCP wiring guide: [docs/MCP.md](docs/MCP.md)
- Field-level schema decisions: [docs/SCHEMA.md](docs/SCHEMA.md)

## The feedback flywheel

resman doesn't just store what an agent did — it reads its own usage back and
turns it into advice. **The more an agent uses resman, the sharper
`resman distill` gets.**

Every MCP and loop CLI call lands in `$RESMAN_HOME/usage.jsonl`. `resman distill`
mines that telemetry for behavioral gaps and surfaces them as *evidence-grounded*
suggestions. For example, an agent that logged ten experiments under a tag but
never verified one:

```bash
$ resman distill -t apr17
...
## Suggestions
1. ...
2. Agent usage shows 10 experiments added for this tag but zero verify calls —
   a reproducibility gap. Re-run your top candidates through `resman verify`
   before trusting them.
```

That line fires *only because the telemetry shows the behavior* — not from a
static rule. Remove `usage.jsonl` and it vanishes: distill degrades gracefully
to its baseline output, so there is **zero noise before an agent has any
history**, and the advice compounds as real usage accumulates. Local-only; opt
out any time with `RESMAN_DISABLE_USAGE_LOG=1`.

## Design system

The HTML artifacts (`resman report`, `resman distill --html`) are **dual-theme** —
light, dark, and follow-system (`prefers-color-scheme`) — driven by CSS design
tokens defined in one place (`src/html.rs`), with `[data-theme]` override hooks.
The trend chart themes with the page and has hover tooltips; reports and distills
share the same components (stat cards, badges, tables, the best-card).

A versioned component library lives in [`design/`](design/) — 14 self-contained
previews (token swatches, the six status badges, stat cards, the trend chart,
data tables, lineage, signal clusters, and the full report + distill pages),
each tagged for sync to a claude.ai/design project via `/design-sync`.

The **terminal** shares one vocabulary too: every command opens with
`=== resman <cmd> (<context>) ===`, a uniform 80-col rule, consistent
glyph+label status cells, and one empty-state tone — light/dark/system in HTML,
`NO_COLOR`-aware ANSI in the terminal.

## Roadmap

The project is at **v0.17.8**. Through v0.9–v0.17 the following shipped:
`doctor`, `usage`, structured MCP JSON across all tools, typed signals
(`diverged_loss`, `slow_mfu`), schema_version, property tests (v0.9–v0.11);
usage-aware distill, composite best, verify/unverify, `resman_tags`,
`resman_unverify`, `resman_doctor` MCP tools (v0.12–v0.13); full MCP parity
for the query commands (v0.14); wandb/mlflow CSV import (v0.15); a **dual-theme
HTML design system** with a synced `design/` component library (v0.16); and
**unified terminal presentation** across every command (v0.17).
See [CHANGELOG.md](CHANGELOG.md) and field-level decisions in
[docs/SCHEMA.md](docs/SCHEMA.md).

Next:
- **v1.0 schema freeze** — `val_bpb` → `primary_metric`, `memory_gb` →
  `peak_memory_gb` (single PR, `serde(alias)` keeps all prior stores).
- **Composite-weight tuning** — data-driven, gated on first batch of
  real `usage.jsonl` from agent sessions.
- **`resman serve`** — zero-dep HTTP dashboard (upstream request).
- **`resman sync`** — opt-in cloud sync for teams (paid tier; OSS CLI
  stays free).

## License

MIT
