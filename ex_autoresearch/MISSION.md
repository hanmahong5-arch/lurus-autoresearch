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

## Robustness standards (every feature must hit these)

These are the **acceptance criteria for "production ready"** in this codebase. Skipping any of them is shipping a demo, not a product. Apply to every new module and every bug fix.

1. **Graceful degradation, never hard failure.** If Crawl4AI is down, fall back to `Scraper.Native` automatically — log it, don't crash. If Serper is down, fall back to a different search provider or a degraded mode. Customers running this on-prem don't have your on-call rotation.

2. **Bounded everything.** Every external call has an explicit `:receive_timeout` (HTTP), `Task.async_stream/3` has explicit `:timeout` AND `:max_concurrency`, every queue has a depth limit. The BEAM VM should never OOM because of one runaway research run.

3. **Telemetry events at every boundary.** Every external call emits `:telemetry.execute([:ex_autoresearch, <stage>, :start | :stop | :exception], measurements, metadata)`. This is what feeds Langfuse in Week 5. Don't bolt it on later — emit from day one.

4. **PubSub progress events on `"research:events"` topic.** Long-running operations broadcast `{:research_progress, report_id, %{stage: atom, status: atom, detail: term}}`. The LiveView consumes these for real-time UX. If a scraper takes 8 seconds, the user must see "fetching url 3 of 5..." not a frozen spinner.

5. **Idempotency where possible.** Re-fetching the same URL within N seconds returns the cached result. Re-running a research with the same `report_id` does not duplicate work. Crashed Oban jobs retry without poisoning the report.

6. **Errors are values, not exceptions.** Public functions return `{:ok, _}` | `{:error, reason}`. Pattern match on specific error reasons. Use `try/rescue` only at process boundaries (Oban worker `perform`, LiveView `handle_event`).

7. **Dependency failures are observable.** If LLM token quota is exhausted, the user sees "OpenRouter quota exhausted, retry in 30s," not a generic 500. Surface the specific reason in the UI.

8. **One-line config flips for every external service.** Customer can swap Crawl4AI → Native, OpenRouter → Anthropic, SQLite → Postgres with a single env var. No code change.

## UI/UX standards (every LiveView must hit these)

This product is sold to law firms and consulting partners. **The UI is the trust signal.** Sloppy UI = lost deal regardless of how good the backend is.

1. **Real-time, never frozen.** No operation > 1 second goes without a progress indicator showing what is happening (which URL is being scraped, which LLM call is in flight). Use the `research:events` PubSub topic.

2. **Loading states are first-class.** Every async UI op has explicit `:idle | :loading | :success | :error` states with distinct visual treatment. No "did I click that?" ambiguity.

3. **Error states surface the reason.** "Crawl4AI returned 503 — falling back to native scraper" is better UX than a red box saying "error". The user must understand *what* failed and *what's next*.

4. **Clean typography, calm spacing.** Tailwind v4 utilities only (no inline styles, no `@apply`). Use `<.icon>` from `core_components.ex` for icons — never raw SVG, never Heroicons modules.

5. **Premium feel, not enterprise drab.** Subtle micro-interactions on buttons (hover, active, focus rings), smooth transitions on state changes, considered empty states. Compare against Linear / Notion / Stripe — that bar.

6. **Streams for collections.** Any list with > 10 items uses `stream/3` (LiveView streams). Never `assign(:items, list)` for unbounded lists — leads to memory ballooning and we will get burned.

7. **DOM ids on every interactive element.** Every form, button, list item has an explicit `id="..."` so LiveView tests can target them with `element/2`.

8. **Responsive by default.** Test at narrow widths. Sidebar collapses, tables scroll, modals fit. Tailwind responsive prefixes (`sm:`, `md:`, `lg:`) liberally.

9. **No raw `<script>` in HEEx.** Use `:type={Phoenix.LiveView.ColocatedHook}` for inline JS, or external `phx-hook="MyHook"` referenced from `assets/js/`.

10. **Empty states are designed, not default.** Every collection has an explicit empty state with a CTA to populate it. "No reports yet" with a subtle illustration + "Start your first research" button — not a blank page.

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
