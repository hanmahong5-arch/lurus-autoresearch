# Mission

> **NOTE TO FUTURE CLAUDE / AI AGENTS LANDING IN THIS DIRECTORY:**
> `AGENTS.md` and `CLAUDE.md` at this level are **gitignored** and regenerate per session. **This file (`MISSION.md`) is the durable, committed source of truth** for what this codebase is and what's locked. Read this *before* you make any architectural suggestion. If a decision here conflicts with what you'd otherwise propose, *the decision here wins* unless the user explicitly overrides — they were locked deliberately.

## What this is

**The open-source, self-hosted, auditable, intranet-data-source-capable alternative to Perplexity Pro Deep Research.**

Active commercial focus as of 2026-05-01. Status was previously "deprioritized optionality side-project" — that framing is **outdated and overridden by this document**.

Repo positioning is fully described in `../STRATEGY.md` § *"Second product line: ex_autoresearch"*. This file is the engineering-level decision lock.

## Target customers (named)

- Law firms (matter research, due diligence, regulatory tracking)
- Management consulting firms (industry deep-dives where confidentiality forbids cloud egress)
- Large-company R&D departments (competitive intelligence on private corpora)
- University / government research offices (compliance-bound research)

**Not targeted:** consumers, hobbyists, small SaaS startups, anyone who would happily use Perplexity cloud.

## Architectural decisions LOCKED (do not relitigate)

These were chosen after explicit research and trade-off analysis. Each has a reason. Don't re-open without showing the reason has changed.

1. **Web scraping: Crawl4AI is primary, native `Req` is fallback, Firecrawl Hex SDK is cloud-spillover only.**
   *Reason:* Crawl4AI is Apache-2.0 fully open. Firecrawl's self-hosted version is AGPL-3.0 with closed-source anti-bot/proxy/dashboard, which fails enterprise legal review for our target customers. Firecrawl cloud SDK (`firecrawl` v1.2.x on Hex) is fine as opt-in spillover.

2. **Agent runtime: `jido` v2.2.0** (Action / Agent / Sensor / Signal / AgentServer). Vendor into `deps/` once we ship — small community, bus-factor risk.

3. **Outer scheduler: Symphony pinned to a specific commit SHA, not `main`.** It's an "engineering preview" per its own README. Budget one re-port within 12 months.

4. **State: Ash + AshSqlite (later AshPostgres for pgvector).** All resource changes go through `mix ash.codegen <desc> --yes` followed by `mix ash_sqlite.migrate`. Never write raw Ecto migrations for Ash-managed tables.

5. **HTTP client: `Req`.** Never `:httpoison`, `:tesla`, `:httpc`. (Phoenix-stack default; preserved.)

6. **UI: Phoenix LiveView, no separate React/Vue frontend.** Real-time research-progress streaming is a flagship demo.

7. **Memory: pgvector + Bumblebee for local embeddings (Week 6+).** No OpenAI embeddings on the canonical path — data sovereignty.

8. **Observability: Langfuse self-hosted (Week 5+).** Per-research-run cost + latency + LLM call tree are required for enterprise audit.

9. **Self-A/B testing: resman is the experiment ledger for our own prompt changes (Week 7+).** Every prompt edit creates a resman experiment with `quality_score` as the metric. This is the dogfooding moat — we ship the same product we used to track our own work.

10. **Deployment: docker-compose for enterprise on-prem POC (Week 8). No K8s on the canonical path** until a customer demands it.

## Anti-goals (do not drift)

- **No consumer pricing tier.** Enterprise-only revenue model.
- **No general-purpose Perplexity clone.** Vertical-first (law/consulting/R&D).
- **No cross-pollination with the resman Rust codebase.** Different binaries, different repos eventually. Concepts can be shared; code cannot.
- **No second agent framework alongside `jido`.** One inner runtime.
- **No raw Ecto for Ash-managed tables.** `mix ash.codegen` is non-negotiable.
- **No closed-source dependencies on the canonical path.** AGPL is a flag-yellow; non-OSI is a flag-red.
- **No "let's add LangChain too."** We have `jido`. That is sufficient.

## Current sprint: Week 1–2 — Crawl4AI integration

**Goal:** Replace the inline `Req.get` + regex HTML extraction at `lib/ex_autoresearch/deep_research/tools/research_runner.ex:76-103` with a swappable `Scraper` behaviour, with Crawl4AI as the default implementation.

**Concrete changes:**
1. Define `ExAutoresearch.DeepResearch.Scraper` behaviour with `@callback fetch(url :: String.t(), opts :: keyword()) :: {:ok, %{markdown: String.t(), metadata: map()}} | {:error, term()}`.
2. Implement `ExAutoresearch.DeepResearch.Scraper.Native` — extract current logic from `ResearchRunner` unchanged. Used as fallback / when Crawl4AI is unavailable.
3. Implement `ExAutoresearch.DeepResearch.Scraper.Crawl4ai` — `POST /crawl` + poll `GET /task/{id}` against a self-hosted Crawl4AI instance. Configure base URL via `:ex_autoresearch, :crawl4ai_base_url` (default `http://localhost:11235`).
4. (Optional, Week 2) Implement `ExAutoresearch.DeepResearch.Scraper.Firecrawl` using the `firecrawl` Hex SDK — for cloud-spillover.
5. Configure default via `config :ex_autoresearch, :scraper, ExAutoresearch.DeepResearch.Scraper.Crawl4ai` (with `Native` as the fallback when env var `EX_AUTORESEARCH_SCRAPER=native` is set).
6. Modify `ResearchRunner.do_run/3` line 38 to call the configured scraper instead of inline `fetch_page_content`. Keep `fetch_page_content` private only for the `Scraper.Native` path.
7. Add unit tests for each `Scraper` implementation (mock Crawl4AI HTTP via Bypass / Req's testing API).
8. Add `docker-compose.yml` snippet committed to repo for `unclecode/crawl4ai:latest`.

**Acceptance:**
- `mix precommit` clean.
- `Scraper.Crawl4ai` returns clean markdown for at least 3 representative URLs (one static, one JS-heavy, one with paywall — paywall expected to fail gracefully).
- `Scraper.Native` still passes tests (regression guard).
- No change to `ResearchRunner.run/2,3` public API.

## Pitfalls (things that have already burned us or obvious traps)

- Don't put Crawl4AI URLs / API keys in compiled config. Use `runtime.exs` + env vars.
- `Req` retries are off by default — keep them off for scraping (we want fast-fail to fall back to `Native`).
- Don't forget to set `--shm-size=1g` on the Crawl4AI Docker container or Playwright will OOM.
- After ANY Ash resource change, `mix ash.codegen <desc> --yes && mix ash_sqlite.migrate` — see Phoenix/Ash boilerplate in `AGENTS.md`.

## Where to look (file map)

- Pipeline orchestrator: `lib/ex_autoresearch/deep_research/research_orchestrator.ex`
- Search (Serper): `lib/ex_autoresearch/deep_research/tools/search.ex`
- Web fetch (the seam we're replacing): `lib/ex_autoresearch/deep_research/tools/research_runner.ex:76-103`
- Ash domain root: `lib/ex_autoresearch/research/research.ex`
- Ash resources: `lib/ex_autoresearch/research/{report,investigation,template}_resource.ex`
- Oban worker: `lib/ex_autoresearch/workers/research_worker.ex`
- LiveView entry: `lib/ex_autoresearch_web/live/dashboard_live.ex`

## Update protocol

When a locked decision changes (e.g., we rip out `jido` for a different runtime), update **this file in the same commit** as the code change. The git history of this file is the audit log of architectural decisions.
