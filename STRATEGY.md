# Strategy

This repo hosts three pieces. Their relationship is deliberate: **two of them exist to generate training demand for the third, which is the commercial product.**

```
┌───────────────────────────┐        ┌───────────────────────────┐
│  base_autoresearch/        │        │  auto_research_task/       │
│  (Python · karpathy fork)  │        │  (Python + Rust)           │
│  reference training loop   │        │  loop + resman integration │
└────────────┬──────────────┘        └────────────┬──────────────┘
             │                                    │
             │     both produce:                  │
             │     · results.tsv                  │
             │     · run.log                      │
             ▼                                    ▼
        ┌────────────────────────────────────────────┐
        │        resman  — the commercial product      │
        │        (Rust CLI · local-first · OSS)        │
        │                                               │
        │   "experiment tracker for AI agents that run │
        │    100 experiments overnight"                 │
        └────────────────────────────────────────────┘
```

## Positioning

**Category:** Local-first experiment tracker for the AI-agent era.

**One-sentence pitch:** wandb is for humans who log experiments one at a time. resman is for agents that run 100 overnight and need to query "what's the best?" from a shell script in 50ms.

**Why a new category exists now:**
1. Coding agents (Claude Code, Codex, Cursor background agents) can now drive training loops autonomously. karpathy/autoresearch is the canonical public demo.
2. The workload pattern changed: overnight batch of 100+ runs → machine-readable decisions → git commits, not human-logged notebook sessions.
3. Existing trackers are cloud-SDK-first. They break if the agent loses network. They require an account. They can't be called from `bash` in 50ms. They can't round-trip through a self-contained HTML report you email to yourself.

**Why it will win (vs. "just use wandb"):**
- **Latency.** No cloud roundtrip. Agent decision loops reading "current best" are ~50ms vs 500-2000ms.
- **Offline.** Works on a train, on a plane, in a cluster with no egress.
- **Git-native.** Status, commits, and descriptions live in the schema. Agents already work in this mental model.
- **One binary.** `cargo install` or curl a release. No Python env, no Docker, no runtime.
- **Pipe-friendly.** Every command has `-o json|tsv|table` and a `value`-format shortcut for scripts.

## Validated by upstream community signal

Before committing to features, we audited the top-voted issues and PRs on
`karpathy/autoresearch` (52 open issues, 130 open PRs as of Apr 2026). The
three most-commented themes all map to something only resman can serve well:

| Upstream thread (comments) | What the community actually needs | How resman answers |
|---|---|---|
| PR #302 "Memory-in-the-Loop" (41) · Issue #47 "novelty" · PR #80 "diversity" | Agents repeat experiments they already tried. They need a queryable memory of prior work. | `resman search <regex>` and `resman_search` MCP tool — "has this been tried?" in one call. |
| PR #101 "pre-eval checkpoint" · bd75534 "traceback reading" | A `crash` status loses the traceback. Agents need the actual error to decide whether to retry. | `resman add --log run.log` siphons the last 50 lines into `crash_excerpt`. |
| PR #114 "Zero-dep Real-Time Dashboard" (11) | Visualisation without adding deps. | `resman report out.html` today; `resman serve` on the roadmap. |
| Issue #98 "MCP" (closed/merged upstream) | Agent harnesses (Claude Code, Cursor, Codex) now speak MCP natively. | `resman mcp` — five tools exposed as JSON-RPC over stdio. |
| PR #102 "dynamic MFU / GPU detection" | Experiments should know what hardware they ran on. | `resman add` auto-probes `nvidia-smi` for GPU name. |
| PR #472 "structured reasoning / knowledge graph" | Lineage between experiments. | `--parent <commit>` field; future `resman tree`. |

This is how we know we're building something the market actually wants: the
karpathy repo itself is the focus group, and the highest-comment threads are
all features a cloud-SDK tracker cannot deliver.

## Pain points solved (ranked by urgency)

1. **"How do I know if my agent's latest run is actually better?"** — `resman best -f value` as a shell-script primitive.
2. **"My overnight run crashed and I lost the TSV."** — atomic writes, append-only semantics, `resman watch` auto-mirrors the TSV.
3. **"How do I share results with my manager at 9am?"** — `resman report report.html` produces one file. Email it.
4. **"Which of my 10 branches performed best?"** — `resman compare -o json` piped to jq, or a table view.
5. **"I need to migrate off wandb because the bill."** — future: `resman import --from wandb`.

## Monetization

Standard OSS + managed-service ladder. Timelines assume 1 full-time maintainer.

| Tier | What | Price | When |
|---|---|---|---|
| **CLI (OSS)** | Everything in this repo — MIT, stays free forever | $0 | Now |
| **Team Cloud** | Optional `resman sync` — shared run namespace across a team, web dashboard, Slack/Discord hooks on new-best | $15/user/mo | Q2 after first 100 GitHub stars |
| **Self-hosted Enterprise** | SSO, audit log, on-prem sync server | $500–2000/mo/team | Only when inbound asks |

The OSS CLI is a genuine funnel, not a loss leader:
- Solo developers adopt it because it's strictly better for their use case than a cloud SDK.
- When they join a team, they pull the CLI in, and **the team-sync upgrade is a single env var** (`RESMAN_SYNC_URL`). Low-friction expansion.
- This is the Tailscale / Linear / Supabase pattern, not the "open-source loss leader" pattern.

## Anti-goals

Things we will *not* do even if users ask:

- **A full web UI bundled into the binary.** Report is HTML export + file. A dashboard is a separate optional service.
- **Hyperparameter search / scheduling.** That's a different product (Optuna, Ray Tune). We stay narrow: *track and query*, don't orchestrate.
- **Per-step metrics / TensorBoard-style curves.** Our unit is one experiment = one row. Users who want curves already have TensorBoard.
- **Python SDK as primary interface.** The CLI *is* the SDK. Agents speak shell better than they speak bindings.

## The other two sub-projects

### `base_autoresearch/`
karpathy's original, unmodified. Kept as the canonical reference and marketing artifact ("this is the loop resman was built for"). No engineering investment. Upstream-tracking only.

### `auto_research_task/` (training portion)
The demo integration: karpathy-style loop + `resman add` calls in `program.md`. Serves three purposes:
1. Proof that resman integrates with a real agent loop in < 10 lines.
2. Documentation by example for new users.
3. Test fixture — keeps us honest that real-world TSVs parse cleanly.

No independent roadmap. Grows only when it exposes a resman gap.

### `ex_autoresearch/` (Elixir deep-research agent)
**Status: separate product, deprioritized.** It solves a different problem (web research, not ML training) in a crowded space (Perplexity, Genspark, You.com). Kept for optionality but not the commercial focus. If it ever ships, the likely path is a vertical pivot (compliance research, legal research, clinical research) where BEAM's fault tolerance is a real selling point — not a general-purpose "deep research" tool.

## Near-term execution

Completed in v0.2 / v0.3 / v0.4 / v0.5 / v0.6:
1. ✅ Rewrite resman with proper error types, atomic writes, new `add` / `best` / `watch` subcommands (v0.2).
2. ✅ Positioning README distinct from the training-loop README (v0.2).
3. ✅ MCP server + `search` + `near` + `crash_excerpt` + `parent_commit` (v0.3).
4. ✅ Prebuilt-binary CI workflow (`.github/workflows/resman.yml`) — matrix build + tag-triggered multi-platform release.
5. ✅ `resman diff` and `resman tree` (v0.4) — mirrored as `resman_diff_tags` / `resman_lineage` MCP tools.
6. ✅ One-line install script (`install.sh`) + rewritten README install section (v0.4).
7. ✅ **v0.5 — schema generalization**: `Direction` enum + optional `metric_name` / `metric_direction` on Experiment+RunLog, effective-name cascade. Purely additive. Opens TAM beyond karpathy nanoGPT.
8. ✅ **v0.6 — typed signals + distill MVP**: `Signal` enum (Oom, CudaError, NanLoss, AssertFail, Timeout, Unknown) + regex `classify(tail)`; `add --log` classifies regardless of status; `list --signal <kind>` filters; `resman distill -t <tag>` emits structured Markdown/JSON summary (best + lineage + failure clusters + unexplored neighbors + heuristic suggestions, no LLM). MCP mirrors: `resman_find_by_signal`, `resman_distill`, `log_tail` on `resman_add_experiment`.

Up next (v1.0 roadmap):
9. Tag v0.6.0 → CI publishes Linux/macOS/Windows binaries → publish to crates.io.
10. **v0.7 — composite `best` + `Status::Verified`**: multi-dim scoring rubric (GDI-inspired); `resman verify <commit>` reproduces and promotes. Add deferred signal variants (`DivergedLoss`, `SlowMfu`) once sufficient workload data informs thresholds.
11. **v0.8 — distill GA**: richer templates, cross-run clustering, full exploitation of signals + verified + lineage. `resman_distill` becomes the canonical agent long-term memory interface.
12. **v1.0**: schema freeze, reposition as "memory layer for agent training loops"; long-form launch blog post.
13. Only after v1.0: the team-sync backend as a separate repo.

## What would make us wrong

- Existing trackers (wandb specifically) ship a first-class local-only mode, <100ms CLI, and git-commit-as-run-identity. They have the brand, so we'd be squeezed. Probability: low — their revenue model punishes offline use.
- Agent coding assistants converge on an in-memory tracking protocol (MCP, etc.) and skip the filesystem. Probability: medium. Mitigation: resman's JSON schema *is* a protocol; offer a native MCP adapter.
- karpathy/autoresearch fades as a meme and the overnight-agent-training pattern doesn't generalize. Probability: medium. Mitigation: resman's value doesn't depend on karpathy specifically — any LLM-training loop has the same needs.
