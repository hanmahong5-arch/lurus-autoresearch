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
curl -fsSL https://raw.githubusercontent.com/kaizen-38/autoresearch/master/auto_research_task/resman/install.sh | sh
```

Detects your OS+arch, pulls the latest release from GitHub, drops a ~3 MB binary into `~/.local/bin`. Customize with `RESMAN_INSTALL_DIR=/usr/local/bin` or `RESMAN_VERSION=v0.3.0`.

**From crates.io** (any OS with a Rust toolchain):

```bash
cargo install resman
```

**From source**:

```bash
git clone https://github.com/kaizen-38/autoresearch
cargo install --path autoresearch/auto_research_task/resman    # Rust 1.85+
```

Windows users: prebuilt binary on Releases, or `cargo install` path above.

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
| `import <tsv>` | Bulk-import a `results.tsv`. `-t <tag>` names the run; `-f` overwrites. |
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

Global flags: `-D, --data-dir <path>` overrides the data dir for any command.

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

Now the agent can call `resman_search`, `resman_best`, `resman_near`,
`resman_list_recent`, `resman_add_experiment` as native tools — no bash
escaping, no stdout parsing, fewer tokens. Full wiring guide: [docs/MCP.md](docs/MCP.md).

## Roadmap

- `resman diff <tagA> <tagB>` — config-level diff between the best of two runs
- `resman tree` — draw a lineage tree from `parent_commit` links
- `resman serve` — zero-dep HTTP dashboard (requested in upstream PR #114)
- `resman sync` — opt-in cloud sync for teams (paid tier, OSS CLI stays free)
- `resman import --from wandb` / `--from mlflow` — migration helpers

## License

MIT
