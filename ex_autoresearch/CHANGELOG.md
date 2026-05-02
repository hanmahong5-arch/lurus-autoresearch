# Changelog

All notable changes to **ExAutoresearch** are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Each version block must start with a level-2 heading of the exact form
`## [x.y.z] — YYYY-MM-DD`, followed by `### Added` / `### Changed` / `### Fixed`
/ `### Removed` sub-sections. The `lib/ex_autoresearch/changelog.ex` parser
relies on this shape — keep it.

---

## [0.2.0] — 2026-05-02

### Added

- **In-app version badge & "What's new" drawer.** Floating badge anchored
  bottom-right on every page shows the running version. First visit after a
  version bump highlights the badge with a red dot and auto-opens the changelog
  drawer once; dismissed state is remembered in `localStorage`, so it never
  nags twice. Settings page also carries a permanent "Version & Changelog"
  section for retrospective browsing.
- **Per-URL scraper progress in LiveView.** Dashboard now streams individual
  scrape attempts on the `research:events` PubSub topic so users see
  `fetching url 3 of 5 …` instead of a frozen spinner.
- **Scraper telemetry + automatic fallback.** Every scrape goes through
  `:telemetry.span([:ex_autoresearch, :scraper, :fetch], …)`. When the primary
  scraper (Crawl4AI by default) fails, requests automatically fall back to the
  native `Req`-based scraper, with the original error preserved in metadata
  for the UI.
- **`Scraper` behaviour with Crawl4AI + Native baseline.** Web scraping is now
  a swappable behaviour selected via `Application.fetch_env!(:ex_autoresearch,
  :scraper)`. `Scraper.Crawl4ai` is the default (self-hosted, Apache-2.0);
  `Scraper.Native` is the always-available fallback.

### Changed

- Robustness and UI/UX acceptance bars are now spelled out in `MISSION.md` and
  enforced for every new feature: bounded timeouts, telemetry on every external
  boundary, PubSub progress events, designed empty states, errors-as-values.

---

## [0.1.0] — 2026-04-15

### Added

- Initial deep-research agent: orchestrator GenServer, Serper search,
  inline `Req` HTML extraction, Ash + SQLite persistence (`Report` /
  `Investigation` resources), Oban worker, and the LiveView dashboard with
  scheduling and templates.
- Pluggable LLM backends: GitHub Copilot, Claude (Anthropic), Gemini.
- Markdown report export.
