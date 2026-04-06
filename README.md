# Lurus Autoresearch — Autonomous AI Research Workspace

> *One day, frontier AI research used to be done by meat computers in between eating, sleeping, and synchronizing once in a while using sound wave interconnect in the ritual of "group meeting". That era is long gone. Research is now entirely the domain of autonomous swarms of AI agents running across compute cluster megastructures in the skies. The agents claim that we are now in the 10,205th generation of the code base... This repo is the story of how it all began.* —@karpathy, March 2026

[English](#how-it-works) | [中文](#工作原理)

---

## Overview / 概述

This repository is a multi-project workspace for autonomous AI research experiments, inspired by [karpathy/autoresearch](https://github.com/karpathy/autoresearch). It provides three independent implementations that let AI agents modify, train, and iterate on small language models — with no human in the loop.

本仓库是受 [karpathy/autoresearch](https://github.com/karpathy/autoresearch) 启发的多项目工作空间，提供三种独立实现，让 AI 代理自主修改、训练并迭代小型语言模型，全程无需人工干预。

## Project Structure / 项目结构

```
.
├── base_autoresearch/     # Python baseline (original karpathy codebase)
├── ex_autoresearch/       # Elixir/Phoenix implementation with LiveView dashboard
├── auto_research_task/    # Python task runner + resman CLI management tool
└── README.md              # This file
```

| Directory | Language | Description |
|-----------|----------|-------------|
| `base_autoresearch/` | Python | Original single-GPU autoresearch. The agent edits `train.py`, trains for 5 minutes, measures `val_bpb`, keeps or discards. Repeat. |
| `ex_autoresearch/` | Elixir/Phoenix | Full-featured web app with Phoenix LiveView dashboard, multi-user auth, Oban job queue, and SQLite persistence. Supports Claude, Gemini, and GitHub Copilot backends. |
| `auto_research_task/` | Python + Rust | Derived from the original with a `resman` CLI tool for importing results, comparing runs, and generating HTML reports with trend charts. |

---

## How It Works / 工作原理

### Core Idea / 核心理念

1. **Agent modifies code** — An LLM agent edits `train.py` (or a Rust agent in `base_autoresearch`) to try architectural changes, optimizer tweaks, or hyperparameter adjustments.
2. **Training runs for a fixed budget** — Every run trains for exactly 5 minutes wall-clock time (excluding startup/compilation).
3. **Results are evaluated** — The metric is `val_bpb` (validation bits per byte, **lower is better**). It's vocab-size-independent, so different architectures can be fairly compared.
4. **Keep or discard** — If the experiment improved `val_bpb`, keep the change and commit. If it's equal or worse, revert. The agent runs this loop autonomously overnight.

1. **代理修改代码** — LLM 代理编辑 `train.py`（或 Rust 代理）以尝试架构变更、优化器调整或超参数修改。
2. **固定时间预算训练** — 每次运行训练 5 分钟（不含启动/编译时间）。
3. **结果评估** — 指标为 `val_bpb`（验证集 bits per byte，**越低越好**）。它与词表大小无关，因此不同架构可以公平比较。
4. **保留或丢弃** — 如果实验改进了 `val_bpb`，保留变更并提交。否则回退。代理会在夜间自主循环运行。

### Architecture Diagram / 架构图

```
┌─────────────────────────────────────────────────────────────┐
│                    LIVEVIEW DASHBOARD (Elixir)              │
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

---

## Quick Start / 快速开始

### Option 1: Base Autoresearch (Python) / 选项一：基础 Python 版本

**Prerequisites:** NVIDIA GPU, Python 3.10+, [uv](https://docs.astral.sh/uv/)

```bash
cd base_autoresearch

# Install dependencies
uv sync

# Download data and train tokenizer (one-time)
uv run prepare.py

# Run a single experiment manually (~5 min)
uv run train.py
```

### Option 2: Ex Autoresearch (Elixir/Phoenix) / 选项二：Elixir 版本

**Prerequisites:** Elixir >= 1.15, API keys (Serper + LLM)

```bash
cd ex_autoresearch

# Install dependencies
mix deps.get && mix setup

# Set your API keys
export SERPER_API_KEY=your_key_here

# Start the Phoenix server
mix phx.server

# Open the LiveView dashboard at http://localhost:4000
```

### Option 3: Auto Research Task (Python + resman CLI) / 选项三：任务运行器

**Prerequisites:** NVIDIA GPU, Python 3.10+, [uv](https://docs.astral.sh/uv/), Rust 1.85+ (for `resman`)

```bash
cd auto_research_task

# Setup Python env
uv sync

# Prepare data
uv run prepare.py

# Build resman CLI
cd resman && cargo install --path . && cd ..

# Kick off an autonomous agent session
# Point your AI agent at program.md and follow the instructions there
```

---

## Design Choices / 设计决策

| Choice | Reason |
|--------|--------|
| **Single file to modify** (`train.py`) | Keeps scope manageable and diffs reviewable. The agent only edits this one file. |
| **Fixed 5-minute time budget** | All experiments are directly comparable regardless of architecture/batch size. ~12 experiments/hour, ~100 overnight. |
| **val_bpb (bits per byte)** | Vocab-size-independent metric. Fair comparison across different architectures. |
| **Self-contained** | No distributed training, no complex configs. One GPU, one file, one metric. |
| **Autonomous loop** | The agent runs FOREVER without asking for permission. It's a deep research worker, not a co-pilot. |

| 决策 | 原因 |
|------|------|
| **单文件修改** (`train.py`) | 保持可控范围和可审查的差异。代理只修改这一个文件。 |
| **固定 5 分钟时间预算** | 无论架构/批量大小，所有实验可直接比较。约每小时 12 次实验，通宵约 100 次。 |
| **val_bpb（bits per byte）** | 不依赖词表大小的指标。不同架构之间公平比较。 |
| **自包含** | 无分布式训练、无复杂配置。一块 GPU、一个文件、一个指标。 |
| **自主循环** | 代理永不询问许可。它是一个深度研究工作器，不是副驾驶。 |

---

## Detailed Project Descriptions / 子项目详细说明

### base_autoresearch / 基础版本

The original karpathy/autoresearch codebase. A minimal GPT training setup on a single GPU. The agent reads `program.md`, modifies `train.py`, runs training, records results, and repeats.

原始 karpathy/autoresearch 代码。单 GPU 上的最小 GPT 训练设置。代理读取 `program.md`，修改 `train.py`，运行训练，记录结果，并循环。

**Files:**
- `prepare.py` — Data download, BPE tokenizer training, dataloader, evaluation. **Read-only.**
- `train.py` — GPT model (8-layer), Muon + AdamW optimizer, 5-minute training loop. **Agent edits this.**
- `program.md` — Agent instructions. **Human edits this.**

### ex_autoresearch / Elixir 版本

A complete rewrite in Elixir that transforms the concept from a CLI agent into a production-ready web application.

完全用 Elixir 重写的版本，将 CLI 代理转变为生产就绪的 Web 应用。

**Key Features:**
- **Phoenix LiveView dashboard** — Real-time monitoring of research progress
- **Multi-user support** — Role-based auth, organizations, memberships
- **Oban job queue** — Reliable background processing
- **Pluggable LLM backends** — Copilot, Claude, Gemini (switch at runtime)
- **Adaptive search strategy** — LLM decides when to search deeper or synthesize
- **Full persistence** — SQLite via Ash framework
- **Docker support** — `Dockerfile` + `docker-compose.yml` included

**Architecture:**
```
lib/ex_autoresearch/
├── deep_research/
│   ├── research_orchestrator.ex    # Main agent loop
│   ├── search_quality_monitor.ex   # Quality tracking + early stopping
│   └── tools/
│       ├── search.ex               # Web search via Serper/Brave
│       ├── research_runner.ex      # Execute search + fetch + score
│       └── html_extractor.ex       # HTML content extraction
├── accounts/                       # User auth & org mgmt
├── research/                       # Report & investigation resources
├── analysis/
│   └── report_exporter.ex          # Markdown report export
├── agent/llm/                      # LLM backends
├── workers/                        # Oban background workers
└── web/                            # Phoenix LiveView + controllers
```

### auto_research_task / 任务运行器 + 管理工具

A derived version with the `resman` CLI tool for managing and comparing experiments.

附带 `resman` CLI 工具的衍生版本，用于管理和比较实验。

**resman Features (Rust CLI):**
- **Import** — `resman import results.tsv` to store run data as JSON
- **Parse logs** — `resman parse-log "run_*.log"` to extract metrics from training logs
- **List & search** — `resman list --status keep --top 5 --grep "LR"`
- **Compare** — `resman compare` to find the best experiment across all runs
- **Report** — `resman report report.html` generates self-contained HTML with SVG charts
- **Stats** — `resman stats` shows improvement rate, crash rate, mean/std

**Workflow / 工作流:**
```
Agent runs experiments autonomously
         │
         ├──> results.tsv (TSV, untracked by git)
         │         │
         │         ├─► resman import
         │         │
         │         ├─► resman list / compare / stats
         │         │
         └──> resman report report.html  (self-contained with SVG charts)
```

---

## Tech Stack / 技术栈

| Layer | Component | Technology |
|-------|-----------|------------|
| **Python Base** | Training | PyTorch, Flash-Attention 3, Muon optimizer |
| **Python Base** | Data | tiktoken (BPE tokenizer), RustBPE, PyArrow |
| **Elixir** | Web | Phoenix 1.8, LiveView 1.1, Bandit |
| **Elixir** | Persistence | Ash Framework, AshSqlite, SQLite, Oban |
| **Elixir** | LLM | jido_ghcopilot, claude_agent_sdk, gemini_cli_sdk |
| **Elixir** | HTTP | Req |
| **CLI (Rust)** | Tools | Rust, serde, ratatui |
| **CLI (Python)** | Analysis | matplotlib, pandas |

---

## Results Tracking / 结果追踪

Experiments are logged to `results.tsv` (tab-separated, not committed to git):

```
commit	val_bpb	memory_gb	status	description
a1b2c3d	0.997900	44.0	keep	baseline
b2c3d4e	0.993200	44.2	keep	increase LR to 0.04
c3d4e5f	1.005000	44.0	discard	switch to GeLU activation
```

| Column | Description |
|--------|-------------|
| `commit` | Git short commit hash (7 chars) |
| `val_bpb` | Validation bits per byte — lower is better |
| `memory_gb` | Peak VRAM in GB, rounded to 0.1 |
| `status` | `keep` (improved), `discard` (no improvement), `crash` |
| `description` | What was tried |

---

## Platform Support & Tuning / 平台支持与调优

> This code requires a single NVIDIA GPU. For CPU/MPS/AMD support, see the [Notable Forks](#notable-forks--值得关注的分支) section.

**For smaller GPUs (MacBooks, lower-end GPUs):**

1. Use a simpler dataset like [TinyStories](https://huggingface.co/datasets/karpathy/tinystories-gpt4-clean)
2. Decrease `vocab_size` (8192 → 4096/2048/1024)
3. Lower `MAX_SEQ_LEN` and `EVAL_TOKENS` in `prepare.py`
4. Reduce `DEPTH` (layers) in `train.py` from 8 to 4
5. Use `WINDOW_PATTERN` of `"L"` instead of `"SSSL"`
6. Lower `TOTAL_BATCH_SIZE` to `2**14` or smaller

---

## Notable Forks / 值得关注的分支

| Fork | Platform | Link |
|------|----------|------|
| autoresearch-macos | macOS (Apple Silicon) | [miolini/autoresearch-macos](https://github.com/miolini/autoresearch-macos) |
| autoresearch-mlx | macOS MLX | [trevin-creator/autoresearch-mlx](https://github.com/trevin-creator/autoresearch-mlx) |
| autoresearch-win-rtx | Windows RTX | [jsegov/autoresearch-win-rtx](https://github.com/jsegov/autoresearch-win-rtx) |
| autoresearch | AMD ROCm | [andyluo7/autoresearch](https://github.com/andyluo7/autoresearch) |

---

## License / 许可证

MIT

---

## References / 参考资料

- [karpathy/autoresearch](https://github.com/karpathy/autoresearch) — Original project
- [karpathy/nanochat](https://github.com/karpathy/nanochat) — Simplified GPT implementation this is based on
- [karpathy on X (tweet 1)](https://x.com/karpathy/status/2029701092347630069)
- [karpathy on X (tweet 2)](https://x.com/karpathy/status/2031135152349524125)
- [Dummy's Guide to Neural Networks](https://x.com/hooeem/status/2030720614752039185)
