# ExAutoresearch — Self-Hosted Verified Deep Research

A self-hostable, multi-tenant deep-research agent built in Elixir. Give it a question and it searches the web, analyzes findings, digs deeper into promising leads, and generates a **cited, verified** research report. Save the question as a **Brief** and it re-runs on a schedule, delivering a **delta digest** of what changed since last time.

Built on the BEAM for concurrent, fault-tolerant, on-premise research automation — an open, self-hosted alternative to cloud Deep Research for teams who can't send their queries to a third-party LLM.

## How It Works

```
┌──────────────────────────────────────────────────────────────┐
│                     LIVEVIEW SURFACE                          │
│  Dashboard (ad-hoc query)  ·  Briefs (scheduled)  ·  Inbox    │
│  Report detail (cited)     ·  Mission Control (live progress) │
└───────────────┬───────────────────────────────┬──────────────┘
                │                               │ PubSub "research:events"
                ▼                               │
┌──────────────────────────────────────────────┴──────────────┐
│           RESEARCH ENGINE  (stateless pipeline)               │
│   plan → search → analyze → (deepen)? → write → verify        │
│   · state persists in SQLite via Ash — crashes resume         │
│   · runs inside an Oban job (queue :research)                 │
└───┬───────────────────────┬──────────────────────┬───────────┘
    │                       │                      │
    ▼                       ▼                      ▼
┌────────────────┐   ┌────────────────┐   ┌────────────────────┐
│ RESEARCH RUNNER│   │   VERIFIER     │   │   DELTA ENGINE     │
│ · Serper/Brave │   │ extract claims │   │ diff this run vs   │
│ · Scraper.fetch│   │ → ground each  │   │ the Brief's last   │
│   (crawl4ai →  │   │   vs sources   │   │ report → added /   │
│    native f/b) │   │ → typed verdict│   │ changed / removed/ │
│ · score+persist│   │ → cited footer │   │ contradicted       │
│   Source/Invest│   │   (Claim rows) │   │ → Delta + notify   │
└────────────────┘   └────────────────┘   └────────────────────┘
```

A **Brief** is the recurring unit: question + cadence + domain allow/block + notify channels. `brief_schedule_worker` enqueues due Briefs; each run produces a `Report`; `delta_worker` diffs it against the previous run and writes a `Delta`; `notify_worker` pushes the digest.

## Key Features

- **Scheduled recurring research** — save a question as a Brief; it re-runs on a cron cadence, hands-off
- **Structured deltas** — every re-run produces a "what changed" digest (added / changed / removed / contradicted), not a wall of fresh text
- **Verified, cited claims** — the Verifier extracts each claim and grounds it against sources with a typed verdict: `grounded` / `contradicted` / `unsupported` / `complementary`; reports carry inline `[n]` citations
- **Source-level audit trail** — every cited URL is a deduped `Source` row; every `Claim` links back to its source — full provenance for legal/compliance review
- **Multi-tenant** — organization-scoped data isolation, built for on-prem team deployments
- **Quality-driven depth control** — stops digging when results diminish, deepens when findings are rich
- **Pluggable LLM backends** — Anthropic (Claude) or OpenRouter (switch at runtime via env var)
- **Pluggable scraper** — Crawl4AI (default, self-hosted) with automatic fallback to a native Req-based scraper
- **Live progress** — watch plan → search → analyze → verify stream in real time (Dashboard + 3D Mission Control)
- **Markdown / CSV-JSON export** — export completed reports and extracted claims

## Prerequisites

- **Elixir** ≥ 1.15 with OTP
- **LLM access** — at least one of:
  - Anthropic API key (`ANTHROPIC_API_KEY`) for Claude
  - OpenRouter API key (`OPENROUTER_API_KEY`) for other models
- **Search API** — Serper (`SERPER_API_KEY`) or Brave (`BRAVE_API_KEY`)
- **Scraper** (optional) — a Crawl4AI endpoint (`CRAWL4AI_BASE_URL`, default `http://localhost:11235`); falls back to the native scraper if unavailable

## Quick Start

```bash
# Setup (deps + ash.setup + assets)
mix setup

# Set your keys
export SERPER_API_KEY=your_key_here
export ANTHROPIC_API_KEY=your_key_here

# Start the app
mix phx.server

# Open → http://localhost:4000
```

After **any** Ash resource change, regenerate migrations:

```bash
mix ash.codegen <short_desc> --yes && mix ash_sqlite.migrate
```

## Architecture

```
lib/ex_autoresearch/
├── deep_research/
│   ├── research_engine.ex          # Stateless pipeline: plan→search→analyze→deepen→write→verify
│   ├── verifier.ex                 # Claim extraction + grounding + citation footer
│   ├── sources_block.ex            # Numbered-source block for the synthesis prompt
│   ├── scraper.ex                  # Scraper behaviour (fetch/2 seam)
│   ├── scraper/{crawl4ai,native}.ex# Default + fallback scraper impls
│   ├── telemetry_bridge.ex         # :telemetry → PubSub "research:events"
│   └── tools/
│       ├── search.ex               # Web search via Serper / Brave
│       ├── research_runner.ex      # search + Scraper.fetch + score + persist
│       └── html_extractor.ex       # Query-driven extraction
├── research/                       # Ash domain: ExAutoresearch.Research
│   ├── report_resource.ex          # Report  (+ brief_id, run_version)
│   ├── investigation_resource.ex   # Investigation step
│   ├── brief_resource.ex           # Brief    — recurring subscription (cadence, domains, notify)
│   ├── source_resource.ex          # Source   — deduped citation (unique [report_id, url])
│   ├── claim_resource.ex           # Claim    — typed grounding + citation + delta key
│   ├── delta_resource.ex           # Delta    — per-run diff digest
│   ├── template_resource.ex        # Template — saved research preset
│   └── research.ex                 # Ash domain registration
├── workers/                        # Oban
│   ├── research_worker.ex          # Runs one research pipeline (queue :research)
│   ├── brief_schedule_worker.ex    # Enqueues due Briefs
│   ├── delta_worker.ex             # Diffs a new run vs the prior report
│   └── notify_worker.ex            # Pushes delta digests
├── analysis/
│   ├── report_exporter.ex          # Markdown export
│   └── claim_exporter.ex           # CSV / JSON claim export
└── agent/
    └── llm_client.ex               # LLM dispatcher: Anthropic + OpenRouter via Req (runtime-selected)
```

## Tech Stack

| Layer | Technology |
|-------|------------|
| Language | Elixir 1.15+ / OTP |
| Web | Phoenix 1.8, LiveView 1.1, Bandit |
| Persistence | Ash Framework, AshSqlite, SQLite (Postgres + pgvector optional, not yet adopted) |
| Job queue | Oban, ash_oban (scheduled Briefs) |
| Multi-tenancy | Ash attribute strategy (organization-scoped) |
| LLM clients | `Agent.LLMClient` — direct Anthropic + OpenRouter via `Req`, runtime-selected by env var |
| Scraping | Crawl4AI (self-hosted) + native Req fallback |
| HTTP | Req |
| Frontend | Tailwind CSS v4, esbuild, Three.js (Mission Control) |
