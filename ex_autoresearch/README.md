# ExAutoresearch — AI Deep Research Tool

An autonomous deep research agent built in Elixir. Give it a question, and it will automatically search the web, analyze findings, dig deeper into promising leads, and generate a comprehensive research report.

Inspired by Perplexity Pro Deep Research and Genspark — built on BEAM for concurrent, fault-tolerant research automation.

## How It Works

```
┌─────────────────────────────────────────────────────────────┐
│                    LIVEVIEW DASHBOARD                        │
│   Research query input · Report list · Real-time progress    │
└────────────────────────┬────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────────┐
│              RESEARCH ORCHESTRATOR (GenServer)               │
│  1. LLM generates initial search plan (multi-query)         │
│  2. Spawns parallel investigation threads per query         │
│  3. SearchQualityMonitor tracks result quality              │
│  4. LLM decides: go deeper or synthesize                    │
│  5. Repeat until depth budget exhausted or findings converge│
│  6. LLM writes final markdown report                        │
└────┬───────────────────────────────────────────────────┬────┘
     │                                                   │
     ▼                                                   ▼
┌──────────────────────┐         ┌────────────────────────┐
│  RESEARCH RUNNER     │ PubSub  │  SEARCH QUALITY MONITOR │
│  · Serper/Brave search│◄──────►│  · Track quality scores  │
│  · HTML fetch+extract │       │  · Detect diminishing    │
│  · Score relevance   │       │    returns               │
│  · Persist to SQLite │       │  · Signal pivot/stop     │
└──────────────────────┘         └────────────────────────┘
```

## Key Features

- **Autonomous research loop** — One question → full report, no human in the loop
- **Parallel investigation threads** — Multiple searches run concurrently (configurable)
- **Quality-driven depth control** — Stops digging when results diminish, goes deeper when findings are rich
- **Adaptive search strategy** — LLM generates follow-up queries based on findings
- **Pluggable LLM backends** — Copilot, Claude, or Gemini (switch at runtime)
- **LiveView real-time dashboard** — Watch research progress live: search → analyze → write
- **Full persistence** — Every report, investigation step, and source URL saved to SQLite via Ash
- **Markdown report export** — Auto-export completed reports with metadata

## Prerequisites

- **Elixir** ≥ 1.15 with OTP
- **LLM access** — At least one of:
  - GitHub Copilot (via `gh copilot` CLI)
  - Anthropic API key (for Claude)
  - Google API key (for Gemini)
- **Search API** — Serper API key (`SERPER_API_KEY`) or Brave (`BRAVE_API_KEY`)

## Quick Start

```bash
# Setup
mix setup

# Set your API keys
export SERPER_API_KEY=your_key_here

# Start the app
mix phx.server

# Open dashboard → http://localhost:4000
```

## Architecture

```
lib/ex_autoresearch/
├── deep_research/
│   ├── research_orchestrator.ex    # Main agent loop
│   ├── search_quality_monitor.ex   # Quality tracking + early stopping
│   └── tools/
│       ├── search.ex               # Web search via Serper
│       ├── research_runner.ex      # Execute search + fetch + score
│       └── html_extractor.ex       # Query analyzer
├── research/
│   ├── report_resource.ex          # Ash resource: Report
│   ├── investigation_resource.ex   # Ash resource: Investigation step
│   └── research.ex                 # Ash domain
├── analysis/
│   └── report_exporter.ex          # Markdown export
└── agent/llm/
    ├── copilot_backend.ex          # GitHub Copilot LLM
    ├── claude_backend.ex           # Claude LLM
    └── gemini_backend.ex           # Gemini LLM
```

## Tech Stack

| Layer | Technology |
|-------|------------|
| Language | Elixir 1.15+ / OTP |
| Web | Phoenix 1.8, LiveView 1.1, Bandit |
| Persistence | Ash Framework, AshSqlite, SQLite |
| Job queue | Oban |
| LLM clients | jido_ghcopilot, claude_agent_sdk, gemini_cli_sdk |
| HTTP | Req |
| Frontend | Tailwind CSS v4, esbuild |
