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
**Status (as of 2026-05-01): elevated to active commercial focus.** Repositioned from "optionality side-project" to a second product line — the open-source self-hosted enterprise alternative to Perplexity Pro Deep Research, targeting law / consulting / regulated R&D where data sovereignty is non-negotiable. Full positioning, locked architectural decisions, MVP roadmap, and anti-goals: see **§ "Second product line: ex_autoresearch"** at the end of this doc, and `ex_autoresearch/MISSION.md` (committed) for engineering-level decision lock.

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

9. ✅ Tagged v0.6.1 → CI publishes Linux/macOS/Windows binaries (v0.6.0 tag hit fmt-check; v0.6.1 is the effective release).
10. ✅ **v0.7 — `Status::Verified` + `resman verify` + opt-in composite `best`**: reproducibility promotion via tolerance-based comparison (no orchestration — caller provides the new value); `best --composite` blends metric + verified + lineage + desc. Default `best` unchanged — shell-script API preserved. Mirrored as `resman_verify` MCP tool + `composite` param on `resman_best`.

Up next (v1.0 roadmap):
11. Tag v0.7.0 → CI publishes binaries → update crates.io.
12. **v0.8 — distill GA**: richer templates, cross-run clustering, full exploitation of signals + verified + lineage. Tune composite weights from v0.7 usage data. Add deferred signal variants (`DivergedLoss`, `SlowMfu`) once sufficient workload data informs thresholds.
13. **v1.0**: schema freeze, reposition as "memory layer for agent training loops"; long-form launch blog post.
14. Only after v1.0: the team-sync backend as a separate repo.

## What would make us wrong

resman:
- Existing trackers (wandb specifically) ship a first-class local-only mode, <100ms CLI, and git-commit-as-run-identity. They have the brand, so we'd be squeezed. Probability: low — their revenue model punishes offline use.
- Agent coding assistants converge on an in-memory tracking protocol (MCP, etc.) and skip the filesystem. Probability: medium. Mitigation: resman's JSON schema *is* a protocol; offer a native MCP adapter.
- karpathy/autoresearch fades as a meme and the overnight-agent-training pattern doesn't generalize. Probability: medium. Mitigation: resman's value doesn't depend on karpathy specifically — any LLM-training loop has the same needs.

ex_autoresearch:
- Microsoft / Google ship a self-hostable Copilot Research with M365/Workspace data-sovereignty guarantees. Probability: medium-high (12–18 months out). Mitigation: ship before they do, lock in 5+ enterprise reference customers, build switching cost via custom corpus integrations.
- Symphony pivots, deprecates, or breaks the workspace API we depend on. Probability: medium (it's an "engineering preview" per the README). Mitigation: pin to a commit SHA, not main; budget one re-port within 12 months; the architecture is replaceable with raw Oban + DynamicSupervisor if Symphony dies.
- Open-source competitors (Morphic, OpenDeepResearch, Open WebUI) add audit-log / on-prem features and erase our moat. Probability: medium. Mitigation: the dogfooding moat (resman A/B testing the agent's own prompts) produces quality data they can't match; double-down on vertical-specific corpus integrations.

---

## Second product line: ex_autoresearch (active 2026-05+)

**One-sentence pitch:** Perplexity Pro Deep Research, but you can run it on your own infra, audit every data flow, and point it at your private corpus.

### Why this category exists now
1. Hosted deep-research products (Perplexity, You.com, Genspark) cannot legally serve clients in regulated verticals — confidential matter info cannot egress to OpenAI/Anthropic via a third party.
2. Generic OSS Perplexity clones (Morphic, OpenDeepResearch) skip enterprise-mandatory features: audit logs, source provenance, role-based access, intranet ingestion.
3. BEAM's fault-tolerance is uniquely suited to long-running multi-stage research where per-source failures are routine.

### Target customers
Law firms, consulting firms, large company R&D departments, university / government research offices. *Not* consumers, *not* small SaaS startups.

### Three-layer architecture

```
        ┌──────────────────────────────────┐
        │   Symphony (outer scheduler)     │  ← OpenAI · Apache-2.0 · pinned commit SHA
        │   workspaces · max_turns ·       │
        │   max_concurrent_agents          │
        └─────────────┬────────────────────┘
                      │ schedules N concurrent
                      ▼
        ┌──────────────────────────────────┐
        │   ex_autoresearch (research)     │  ← Phoenix LiveView UI · Oban durable jobs
        │   Crawl4AI for scraping          │     pgvector + Bumblebee for memory
        │   resman for prompt A/B testing  │     Langfuse for cost / observability
        └─────────────┬────────────────────┘
                      │ uses
                      ▼
        ┌──────────────────────────────────┐
        │   jido (agent runtime)           │  ← MIT · v2.2.0
        │   Action / Agent / Sensor /      │
        │   Signal / AgentServer           │
        └──────────────────────────────────┘
```

### Locked architectural decisions (do not relitigate without strong reason — see `ex_autoresearch/MISSION.md` for full rationale)

1. **Crawl4AI is primary scraper.** Apache-2.0, fully open, Playwright-based. Firecrawl's self-hosted version is AGPL-3.0 with closed-source proxy/anti-bot/dashboard — disqualifies for enterprise on-prem legal review.
2. **Firecrawl Hex SDK as cloud-spillover fallback only.** Customer-opt-in for bursty workloads; never the default path.
3. **jido v2.2.0 as inner agent runtime.** Vendor into `deps/` long-term once we ship — bus-factor risk on a small project.
4. **Symphony as outer scheduler.** Pin commit SHA, not `main` — engineering preview, breaking changes allowed by the maintainer.
5. **resman is dogfooded internally.** Every research-agent prompt change creates a resman experiment with `quality_score` as the metric. We sell the same product we used to track our own work — quantifiable quality moat competitors can't match.
6. **Phoenix LiveView for UI.** No separate React/Vue frontend. Real-time research-progress streaming is the killer demo.
7. **Ash for all state, not raw Ecto.** `mix ash.codegen` is non-negotiable.

### Anti-goals (do not drift back into these)

- **Consumer pricing tier.** This is enterprise-only. No "$9/mo Hobby plan."
- **General-purpose deep research clone.** Vertical-first (law/consulting/R&D) or die.
- **SaaS-first deployment.** Self-host is canonical; cloud is optional.
- **Cross-pollinating with the resman codebase.** They share *concepts* (experiments, lineage), not *binaries*.
- **Adding more agent frameworks alongside jido.** Pick one. jido is the choice.

### MVP roadmap (8 weeks, started 2026-05-01)

| Week | Deliverable | Status |
|---|---|---|
| 1–2 | Scraper behaviour + Crawl4AI implementation replacing `ResearchRunner.fetch_page_content/2` | in progress |
| 3 | Jido-ize ex_autoresearch — convert ResearchRunner stages into `jido` Actions | |
| 4 | Phoenix LiveView real-time research-progress UI |  |
| 5 | Langfuse integration for cost / observability |  |
| 6 | pgvector + Bumblebee local-embedding memory layer |  |
| 7 | resman integration — every prompt change becomes a versioned resman experiment |  |
| 8 | Symphony outer-shell + docker-compose enterprise-POC bundle |  |

### Monetization (added to existing resman tiers)

| Tier | What | Price | When |
|---|---|---|---|
| **OSS Self-host** | Full source, MIT/Apache stack, customer runs on own infra | $0 | Week 8 (POC) |
| **Enterprise Support** | SLA, security review docs, deployment assistance, custom corpus integrations | $2K–10K/mo per customer | Q3 2026 after first POC closes |
| **Hosted (small firms)** | Multi-tenant cloud option, customer's own LLM API key | $99/seat/mo | Q1 2027 — only if inbound demand pulls it forward |

The OSS self-host is the funnel; enterprise support is where the revenue lives. Same Tailscale/Linear/Supabase pattern as resman.
