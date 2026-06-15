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
   *Reason:* Crawl4AI is Apache-2.0 fully open. Firecrawl's self-hosted version is AGPL-3.0 with closed-source anti-bot/proxy/dashboard, which fails enterprise legal review for our target customers. Firecrawl cloud SDK (`firecrawl` v1.2.x on Hex) is fine as opt-in spillover. *(Implemented: `Scraper` behaviour + `crawl4ai.ex` / `native.ex`, app-env `:scraper` selector, auto-fallback.)*

2. **Runtime: BEAM/OTP-native — no external agent framework.** The research pipeline is a **stateless `ResearchEngine` module driven by Oban jobs (job-per-run)**, supervised by OTP. `Ash` owns all state, `Oban` owns durability/retries/bounded concurrency, OTP supervision owns fault-tolerance.
   *Reason (changed decision — see Update protocol):* the earlier lock chose `jido` v2.2.0 as the inner agent runtime. We shipped the **entire SENTINEL trust + delta layer without it** — OTP + Oban + Ash already provide every primitive a deep-research runtime needs (supervision, durable jobs, bounded concurrency, multi-tenant persistence). Adding `jido` would be framework weight over capabilities BEAM gives natively, against a small-community dep the original lock itself flagged for bus-factor risk. **jido is dropped, not deferred.**

3. **Scheduling: `ash_oban` triggers + the Oban-cron `BriefScheduleWorker`. No outer scheduler, no singleton scheduler process.**
   *Reason (changed decision):* the earlier lock chose **Symphony** (a self-described "engineering preview" with maintainer-sanctioned breaking changes) as an outer scheduler, and a hand-rolled `TemplateScheduler` GenServer existed. Both are gone. `ash_oban` (already a dep, wired via `AshOban.config/2` in the supervision tree) gives durable, multi-tenant, cron-driven scheduling natively; `BriefScheduleWorker` enqueues due Briefs onto the `:research` queue.

4. **State: Ash + AshSqlite (AshPostgres only when a customer needs pgvector).** All resource changes go through `mix ash.codegen <desc> --yes` followed by `mix ash_sqlite.migrate`. Never write raw Ecto migrations for Ash-managed tables.

5. **Trust layer = the `Claim` resource (one row, three jobs).** Each `Claim` is simultaneously (a) an inline grounded citation, (b) an immutable audit unit, and (c) the minimal comparison unit for delta. `Verifier` extracts atomic claims from the draft and grounds each against numbered `Source` evidence (`:grounded | :contradicted | :unsupported | :complementary` + confidence). `Source` carries `relevance_score` (replaces the old byte-count "quality") and a `unique [report_id, url]` identity (cross-query URL dedup).
   *Reason:* one schema underwrites trust + audit + delta — the three things hosted Deep Research structurally cannot offer.

6. **Delta engine = versioned Reports + claim-hash diff.** A run yields a versioned `Report` (`brief_id` + `run_version`); `DeltaWorker` diffs the new claim set against the prior version by normalized `claim_hash` (added/changed/removed/contradicted); `NotifyWorker` pushes the digest. The moat is recurring "what changed and why it matters," not one-shot Q&A.

7. **HTTP client: `Req`.** Never `:httpoison`, `:tesla`, `:httpc`. Keep retries **off** for scraping — fast-fail triggers the `Native` fallback.

8. **UI: Phoenix LiveView, no separate React/Vue frontend.** Real-time research-progress streaming on the `"research:events"` PubSub topic is the flagship trust signal.

9. **Observability: telemetry-first; external sinks are pluggable, not locked.** Every external boundary emits `:telemetry`; `LLMUsageBridge` accrues token usage onto the owning `Report`. A Langfuse exporter is an **optional** sink on top of this seam — *not* a locked dependency.
   *Reason (relaxed decision):* the elegant invariant is "telemetry at every boundary." Which sink consumes it is a config choice, not an architecture lock.

10. **Semantic memory (pgvector + Bumblebee) is an OPTIONAL future upgrade, not locked.** Delta matching today uses normalized `claim_hash`; embeddings are an upgrade path only if hash precision proves insufficient. Don't pull heavy ML deps onto the canonical path speculatively.

11. **Self-A/B testing: resman is the experiment ledger for our own prompt changes (future).** Every prompt edit should create a resman experiment with `quality_score` as the metric — the dogfooding moat. **Concepts shared, code never** (anti-goals red line).

12. **Deployment: docker-compose for enterprise on-prem POC. No K8s on the canonical path** until a customer demands it.

## Anti-goals (do not drift)

- **No consumer pricing tier.** Enterprise-only revenue model.
- **No general-purpose Perplexity clone.** Vertical-first (law/consulting/R&D).
- **No cross-pollination with the resman Rust codebase.** Different binaries, different repos eventually. Concepts can be shared; code cannot.
- **No external agent framework.** BEAM/OTP + Oban + Ash *is* the runtime — not jido, not LangChain, not Symphony. (Reverses the earlier jido lock; see Architectural decisions #2–3.)
- **No raw Ecto for Ash-managed tables.** `mix ash.codegen` is non-negotiable.
- **No closed-source dependencies on the canonical path.** AGPL is a flag-yellow; non-OSI is a flag-red.
- **No speculative heavy deps on the canonical path.** No pgvector / Bumblebee / Langfuse until a concrete need (and a customer) pulls them in. Telemetry + claim-hash cover today's needs.

## Delivered

The headline is the **SENTINEL trust + delta layer**; the Week 1–2 scraper/UI/audit infrastructure underneath it still holds. What's in `master`:

**SENTINEL trust + delta layer (the product moat):**
- **Trust layer:** `Verifier` extracts atomic claims from each draft and grounds them against numbered `Source` evidence, persisting `Claim` rows (`grounding` + `confidence` + `origin_subquery`) and appending a `## ⚠ Verification Notes` footer. Never blocks completion — always returns `{:ok, body}`.
- **`Source` resource:** `relevance_score` replaces the old byte-count quality; `unique [report_id, url]` identity enforces cross-query URL dedup; `scraper_source` recorded.
- **Delta engine:** versioned `Report` (`brief_id` + `run_version`); `DeltaWorker` diffs claim sets by normalized `claim_hash` (added/changed/removed/contradicted); `NotifyWorker` + `Notifications.{Notifier,Webhook}` push the digest; `InboxLive` (`/inbox`) surfaces unread deltas.
- **`Brief` resource:** recurring research subscription (question + cadence + source policy + notify channels), multi-tenant by `organization_id`.
- **Job-per-run engine:** `ResearchEngine` is a stateless pipeline (plan→search→analyze→deepen→write→**verify**) driven by Oban jobs on the `:research` queue. The singleton `ResearchOrchestrator` GenServer and hand-rolled `TemplateScheduler` are **gone**.
- **`ash_oban` scheduling:** `BriefScheduleWorker` (Oban cron) enqueues due Briefs; `ash_oban` wired via `AshOban.config/2` in the supervision tree.
- **Search seam:** Serper + DuckDuckGo + SearXNG behind a `Search` behaviour (multi-backend, on-prem-friendly).
- **Claim audit export:** `Analysis.ClaimExporter` (CSV/JSON) for legal/compliance.

The Week 1–2 foundation below is unchanged and still in force:

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

**Acceptance status:** `mix test` **199 tests, 0 failures, 0 skipped, 0 warnings** (verified 2026-06-13). Format clean, compile clean.

## Current sprint: pick one of

1. **Langfuse upload — close the loop on Week 5.** LLM telemetry already emits everything we need (span events on `[:ex_autoresearch, :llm, :complete]` with provider, model, tokens, outcome, report_id). What's missing: a `LangfuseExporter` that POSTs traces to a self-hosted Langfuse, plus the docker-compose entry. Smallest credible scope.

2. **Week 6 — pgvector + Bumblebee local embeddings.** Switch SQLite → Postgres for the data layer (schema migration via Ash), add `Embedding` resource per Investigation, semantic-search past research before kicking off a new run. **Blocking story:** "duplicate queries don't re-burn tokens; partner can semantically search the corpus."

3. **End-to-end demo.** Boot the docker-compose stack, run a real research query against Crawl4AI, screenshot the LiveView + the Sources block + the audit log + the LLM stats. Surfaces real bugs (Playwright OOM, Serper quotas, provider rate limits) before customers do.

4. **Cost USD calculation.** Add a model→price table (or a `Settings` resource for self-hosted customers to enter their own rates) so the `Report` shows cost in USD, not just tokens. Adds two columns + a tiny UI widget.

5. **P3 enterprise unlock — per-tenant source-domain allow/block enforcement at scrape time + Slack/email/webhook notify channels.** `Brief` already carries `allow_domains` / `block_domains` / `notify_channels`; wire them through the scraper seam and `NotifyWorker`. This is the named enterprise paywall (data-sovereignty + audit), built on the trust layer at near-zero marginal cost.

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

- Research engine (job-per-run pipeline): `lib/ex_autoresearch/deep_research/research_engine.ex`
- Trust layer (claim grounding): `lib/ex_autoresearch/deep_research/verifier.ex`
- Scraper seam: `lib/ex_autoresearch/deep_research/scraper.ex` (+ `scraper/{crawl4ai,native}.ex`)
- Search seam: `lib/ex_autoresearch/deep_research/search.ex` (+ `search/{serper,duck_duck_go,sear_x_n_g}.ex`)
- Per-thread executor: `lib/ex_autoresearch/deep_research/tools/research_runner.ex`
- Ash domain root: `lib/ex_autoresearch/research/research.ex`
- Ash resources: `lib/ex_autoresearch/research/{brief,report,investigation,source,claim,delta,template}_resource.ex`
- Workers: `lib/ex_autoresearch/workers/{research,brief_schedule,delta,notify}_worker.ex`
- LiveView: `lib/ex_autoresearch_web/live/{dashboard,inbox,mission_control,report_detail,audit}_live.ex`
- Claim / audit export: `lib/ex_autoresearch/analysis/claim_exporter.ex`

## Update protocol

When a locked decision changes (e.g., we rip out `jido` for a different runtime), update **this file in the same commit** as the code change. The git history of this file is the audit log of architectural decisions.

**2026-06-13 reconciliation:** decisions #2 (jido) and #3 (Symphony) were de-facto reversed in code — the SENTINEL trust + delta layer shipped on pure OTP / Oban / Ash — but this file was not updated in step, violating the protocol above. This revision realigns the lock with `master` (199 tests green). Future decision changes must update this file in the same commit as the code.
