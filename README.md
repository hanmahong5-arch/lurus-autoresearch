# autoresearch

**A home for [`resman`](auto_research_task/resman) — the local-first experiment tracker for autonomous AI training agents — and the agent-driven training loops it was built for.**

> The era when humans logged ML experiments one at a time through a Python SDK is ending. Coding agents now run 100 experiments overnight. They need a tracker that returns *"what's the best so far?"* to a shell script in 50ms, works offline, and survives a crashed laptop. — That's what `resman` is.

[English](#en) | [中文](#zh)

---

<a id="en"></a>

## What's in here

| Path | What it is | Status |
|---|---|---|
| **[`auto_research_task/resman/`](auto_research_task/resman)** | **The product.** Rust CLI — local-first experiment tracker for AI-agent training loops. | **Active** |
| [`base_autoresearch/`](base_autoresearch) | karpathy's original single-GPU autoresearch loop. Reference only. | Upstream-tracking |
| [`auto_research_task/`](auto_research_task) | A training loop wired up with `resman add` / `resman best`. Reference integration. | Active |
| [`ex_autoresearch/`](ex_autoresearch) | Elixir/Phoenix deep-research web agent — a separate product, deprioritized. | Maintenance |

See **[STRATEGY.md](STRATEGY.md)** for positioning, monetization plan, and anti-goals.

## Install resman

```bash
# one-liner (Linux / macOS)
curl -fsSL https://raw.githubusercontent.com/kaizen-38/autoresearch/master/auto_research_task/resman/install.sh | sh

# or from crates.io
cargo install resman

# or from source
cargo install --path auto_research_task/resman
```

## resman — 30-second tour

```bash
resman init

# Option A — import an agent-written TSV
resman import results.tsv -t apr17

# Option B — append one experiment at a time (no TSV needed)
resman add -t apr17 -c $(git rev-parse --short HEAD) \
           -v 0.9921 -m 44.2 -s keep -d "increased LR to 0.04"

# Query from scripts / agent loops (the killer feature)
BEST=$(resman best --format value)       # → 0.992100
resman search "GeLU"                     # has the agent tried GeLU? shows prior matches
resman near 0.985                        # what else landed near this score?

# Human views
resman list --top 10
resman compare
resman stats
resman report report.html                # self-contained dark-mode HTML

# Live mode during overnight runs
resman watch results.tsv -t apr17 -i 2

# Expose as native tools to your agent harness (Claude Code / Cursor / Codex)
resman mcp   # run as an MCP server over stdio — see docs/MCP.md
```

Full docs: **[`auto_research_task/resman/README.md`](auto_research_task/resman/README.md)**

## Why resman exists

`wandb`, `mlflow`, and cousins were designed when a human logged experiments one at a time via SDK. That workload is being replaced by AI agents running overnight batches that read their own prior results and decide the next move. The requirements are different:

|   | Cloud SDK trackers | resman |
|---|---|---|
| Cold-start | account → project → run | `resman init` |
| "What's the best?" from a script | SDK + network call, ~1s | `resman best -f value`, ~50ms |
| Offline | ❌ | ✅ |
| One static binary | ❌ (Python + deps) | ✅ (Rust, ~3 MB) |
| Git-commit-as-run-identity | ❌ | ✅ (built in) |
| Self-contained HTML report (email, archive) | ❌ (web UI only) | `resman report out.html` |
| Concurrent agents writing different runs | Needs coordination | Per-run file, zero locking |
| Native tool for MCP-speaking agents | ❌ | `resman mcp` → five tools via JSON-RPC |
| "Has this idea been tried?" in one call | ❌ (manual search) | `resman search <regex>` or MCP tool |

This is a different product category, not a replacement.

## Agent loop example

The pattern resman was built to serve:

```bash
BASELINE=$(resman best --tag "$TAG" -f value 2>/dev/null || echo "999")

# agent edits train.py, runs training...
NEW_BPB=$(grep "^val_bpb:" run.log | awk '{print $2}')
COMMIT=$(git rev-parse --short HEAD)

if (( $(echo "$NEW_BPB < $BASELINE" | bc -l) )); then
  resman add -t "$TAG" -c "$COMMIT" -v "$NEW_BPB" -m 44.0 -s keep -d "$IDEA"
  git commit --allow-empty -m "autoresearch: $IDEA → $NEW_BPB"
else
  resman add -t "$TAG" -c "$COMMIT" -v "$NEW_BPB" -m 44.0 -s discard -d "$IDEA"
  git reset --hard HEAD~1
fi
```

All `resman` IO is atomic (tmp-write + rename) and safe to run from 10 concurrent agent loops as long as each uses a different `--tag`.

## Running the training loop

If you also want the karpathy-style agent + training loop that generates these TSVs (not required to use resman):

```bash
cd auto_research_task                         # or base_autoresearch
uv sync
uv run prepare.py                             # one-time data prep, ~2 min
uv run train.py > run.log 2>&1                # single 5-minute training run
```

Requires a single NVIDIA GPU. See each sub-directory's `program.md` for the autonomous loop protocol.

## License

MIT

---

<a id="zh"></a>

## 中文说明

这个仓库的核心是 **[`resman`](auto_research_task/resman)** —— 面向 AI agent 自主训练场景的本地优先实验追踪工具。

**问题**：wandb / mlflow 这类工具是为"人类每次记录一次实验"设计的。但 AI coding agent（Claude Code、Codex 等）现在一夜跑 100 次实验，需要的是：

- 从 shell 脚本里 50ms 拿到"当前最好成绩"
- 完全离线可用，不需要账号
- 单个静态二进制，没有 Python 依赖
- 以 git commit 为身份
- 一个 HTML 文件就能发邮件分享

**resman 的定位**：不是替代 wandb，是一个不同的产品品类。

### 安装

```bash
# 一行安装（Linux / macOS）
curl -fsSL https://raw.githubusercontent.com/kaizen-38/autoresearch/master/auto_research_task/resman/install.sh | sh

# 或从 crates.io
cargo install resman

# 或源码编译
cargo install --path auto_research_task/resman
```

### 快速开始

```bash
resman init
resman import results.tsv -t apr17
resman best --format value                    # agent 读"当前最好"
resman report report.html                     # 人读完整报告
```

详细文档：[`auto_research_task/resman/README.md`](auto_research_task/resman/README.md)。
商业与路线图：[STRATEGY.md](STRATEGY.md)。

### 仓库结构

| 目录 | 内容 | 状态 |
|---|---|---|
| **`auto_research_task/resman/`** | **主产品**。Rust CLI 实验追踪器 | **活跃开发** |
| `base_autoresearch/` | karpathy 原版训练循环 | 仅跟随上游 |
| `auto_research_task/` | 训练循环 + resman 集成示例 | 活跃 |
| `ex_autoresearch/` | Elixir 深度研究 Web agent（独立产品，优先级低） | 维护 |

### 训练循环（可选）

如果要跑 karpathy 风格的自主训练循环（单 NVIDIA GPU，5 分钟一次实验）：

```bash
cd auto_research_task
uv sync && uv run prepare.py && uv run train.py > run.log 2>&1
```

## 致谢

核心训练循环基于 [karpathy/autoresearch](https://github.com/karpathy/autoresearch) 与 [karpathy/nanochat](https://github.com/karpathy/nanochat)。resman 是在使用这些循环过程中发现真实痛点后独立设计的工具。
