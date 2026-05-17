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

## Delivered (as of 2026-05-16)

The 8-week MVP is ahead of its Week 1-2 plan. What's in `master`:

**Scraper layer (Week 1-2 + extras):**
- `Scraper` behaviour with `Native` + `Crawl4ai` impls; configured default via `:scraper` app-env.
- `Scraper.fetch/2` wrapper auto-falls-back to Native on primary failure (telemetry-instrumented). `{:both_failed, primary: ..., native: ...}` when both die.
- `docker-compose.yml` ships an `unclecode/crawl4ai:latest` service with `--shm-size=1g`.
- Telemetry events on `[:ex_autoresearch, :scraper, :fetch, :start|:stop]` carry `report_id`, `investigation_id`, `outcome`, `primary_error`.
- **Not done:** the "3 representative URLs" smoke test from the original acceptance — needs a live Crawl4AI container.

**Real-time UI (Week 4 pulled in early):**
- `TelemetryBridge` re-emits scraper stop events as `{:scraper_progress, ...}` on the `"research:events"` PubSub topic.
- DashboardLive renders per-URL progress with a daisyUI alert that turns amber when fallback was used and surfaces the underlying reason ("Crawl4AI failed (HTTP 503), fell back to native scraper").
- `MissionControlLive` at `/mission` — Three.js 3D control room subscribing to the same `"research:events"` topic. Orchestrator sphere recolors on agent status, token reactor cylinder fills by usage, scraper outcomes spawn orbiting satellites (green/amber/red by outcome), `quality_alert` events surface as a speech bubble. Clicking a satellite opens a right-side drawer with the persisted `Investigation` (query, reasoning, quality_score, fetched_at, findings excerpt).
- JS hook (`assets/js/hooks/mission_control.js`) lazy-loads three.js + addons from `esm.sh` on first mount — keeps the app.js bundle slim. Local `.glb` models in `priv/static/3d/` (`3d/` added to `static_paths/0`).

**Citation-grade provenance (the legal/compliance differentiator):**
- Every `Investigation` persists `fetched_at`, `content_hash` (SHA-256 hex), `scraper_source`, `fallback_used`.
- Final report markdown gets an automatic `## Sources` block with numbered clickable links, fetched-at timestamps, hash prefix, and ⚠ marker if Crawl4AI fell back.

**Append-only audit log:**
- `ExAutoresearch.Audit` domain + `Event` resource, multi-tenant by `organization_id`, declares only `:read` and `:record` actions so immutability is enforced by the domain itself.
- Hooks at: DashboardLive `start_research` / `export_report`; ResearchOrchestrator `update_report_complete` / `fail_report`.
- `/audit` LiveView with filter chips, real-time PubSub append, designed empty state, tenant-isolated.

**LLM token telemetry (local-visible half of Week 5):**
- `LLMClient.complete/2` wrapped in `:telemetry.span` on `[:ex_autoresearch, :llm, :complete]`. Public return signature unchanged — usage flows via telemetry metadata, not return value.
- Each provider parses its own `usage` shape (OpenAI `prompt_tokens`/`completion_tokens`; Anthropic `input_tokens`/`output_tokens`).
- `Observability.LLMUsageBridge` attaches at app boot, accumulates onto the owning `Report` via the new `:track_llm_usage` action, tenant-scoped, `authorize?: false`. Skips on nil `report_id` or `:error` outcome.
- `Report` gains `total_input_tokens`, `total_output_tokens`, `llm_calls_count`. DashboardLive `report_detail` shows a 3-column daisyUI `stats` row when `llm_calls_count > 0`.
- **Not done in this scope:** actually shipping a trace to a self-hosted Langfuse — that becomes a separate commit when the Langfuse docker-compose stack lands.

**Health probes:**
- `GET /healthz` (always 200 with version) + `GET /readyz` (Repo `SELECT 1`; Crawl4AI status reported informationally — never flips readiness because the auto-fallback means we still serve traffic).

**Acceptance status:** `mix precommit` 60 tests, 0 failures, 0 skipped. Format clean. deps.unlock --unused clean. Compile clean.

## Current sprint: pick one of

1. **Langfuse upload — close the loop on Week 5.** LLM telemetry already emits everything we need (span events on `[:ex_autoresearch, :llm, :complete]` with provider, model, tokens, outcome, report_id). What's missing: a `LangfuseExporter` that POSTs traces to a self-hosted Langfuse, plus the docker-compose entry. Smallest credible scope.

2. **Week 6 — pgvector + Bumblebee local embeddings.** Switch SQLite → Postgres for the data layer (schema migration via Ash), add `Embedding` resource per Investigation, semantic-search past research before kicking off a new run. **Blocking story:** "duplicate queries don't re-burn tokens; partner can semantically search the corpus."

3. **End-to-end demo.** Boot the docker-compose stack, run a real research query against Crawl4AI, screenshot the LiveView + the Sources block + the audit log + the LLM stats. Surfaces real bugs (Playwright OOM, Serper quotas, provider rate limits) before customers do.

4. **Cost USD calculation.** Add a model→price table (or a `Settings` resource for self-hosted customers to enter their own rates) so the `Report` shows cost in USD, not just tokens. Adds two columns + a tiny UI widget.

5. **Week 3 — jido-ization** (deferred from original plan because the manual GenServer state machine works fine; jido pays off when we add Symphony in Week 8).

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
