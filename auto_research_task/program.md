# autoresearch

This is an experiment to have the LLM do its own research.

## 项目说明

本项目是 karpathy/autoresearch 的衍生素材，Agent 自主修改 `train.py` 进行 LLM 训练实验，以 `val_bpb`（越低越好）为唯一优化目标。

**硬件要求**：Ampere (8.0) 或更新架构 GPU（flash-attn3 + bfloat16）。原始参数面向 H100。如果你在小 GPU 上运行，请调整 `prepare.py` 中 `MAX_SEQ_LEN`、`EVAL_TOKENS` 和 `train.py` 中 `DEPTH`、`DEVICE_BATCH_SIZE`。

## Memory layer (resman MCP) — read before you act

你不是在白纸上做研究。本仓库配套一个 [`resman`](resman/) 实验记忆库，通过 MCP 暴露 10 个 tool。**新 session 启动后、动手改 `train.py` 之前，先读历史**，否则你会浪费 5 分钟跑一个去年就 OOM 了的配置。

| 何时 | 调用 | 目的 |
|---|---|---|
| Session 开始（每次） | `resman_distill { tag: <最近一个 tag> }` | 一次性读出上一个 tag 的最佳点、谱系、失败聚类、未探索方向 |
| Session 开始 | `resman_list_recent { limit: 20 }` | 看最近 20 次实验的成败 |
| Session 开始 | `resman_find_by_signal { signal_type: "oom" }` | 知道哪些方向必爆显存，提前避开 |
| 想到一个新 idea | `resman_search { pattern: "<关键词>" }` | 这个 idea 是否已经被试过 |
| 确定下一步基线 | `resman_best { composite: true }` | 用复合分（指标+验证+谱系+描述）挑最值得继承的 commit |
| 从非 HEAD 派生 | `resman_lineage { commit: <parent> }` | 看这条链上哪些路收敛、哪些断头 |
| **每次跑完（无论成败）** | `resman_add_experiment { tag, commit, val_bpb, memory_gb, status, description, log_tail, parent_commit }` | 原子写入；`log_tail` 让 resman 自动分类 OOM / NaN / CUDA error |
| 重复一个旧 commit | `resman_verify { commit, value }` | 若新值在容差内（默认 ±0.01），promote 到 `status=verified` |

**不要**只往 `results.tsv` 写 —— TSV 是 grep 友好的镜像，agent 的长期记忆在 resman。两者都写，但**有冲突时 resman 是 source of truth**。

如果 MCP server 没连上（`tools/list` 里看不到 `resman_*`），降级到 `resman` CLI（`resman distill -t <tag>`、`resman add -t ... -c ... -v ... -s ... -d ... --log run.log`），别跳过记忆步骤。

**CLI gotcha**：`resman list` 默认只显示 `keep` 状态。要看 OOM/NaN 历史必须加 `--status all` 或 `--status crash`，否则结果会误导成「没有失败过」。MCP path 不受此限。

## Setup

To set up a new experiment, work with the user to:

1. **Agree on a run tag**: propose a tag based on today's date (e.g. `apr4`). The branch `autoresearch/<tag>` must not already exist — this is a fresh run.
2. **Create the branch**: `git checkout -b autoresearch/<tag>` from current master.
3. **Read the in-scope files**: The repo is small. Read these files for full context:
   - `README.md` — repository context.
   - `prepare.py` — fixed constants, data prep, tokenizer, dataloader, evaluation. Do not modify.
   - `train.py` — the file you modify. Model architecture, optimizer, hyperparameters, training loop.
4. **Verify data exists**: Check that `~/.cache/autoresearch/` contains data shards and a tokenizer. If not, tell the human to run `uv run prepare.py`.
5. **Initialize resman + load prior memory** — this is the step new sessions skip and regret. Call in this order:
   - `resman init` (idempotent; creates `$RESMAN_HOME` or `~/.resman` if missing).
   - **First**, `resman_list_recent { n: 20 }` — this is your discovery probe. The result tells you (a) whether any prior session exists at all and (b) what the most recent tag(s) are.
   - **If `list_recent` returned `no experiments found`** — you are the first session on this store. Skip distill. Note this to the user ("fresh resman store — this run will establish baselines"). Move to step 6.
   - **Otherwise**, identify the most recent tag from the list, then call `resman_distill { tag: <that-tag> }` and **read every section** — best, lineage, signal clusters, suggestions. These are your starting heuristics.
   - `resman_find_by_signal { signal_type: "oom" }` — know which configs have crashed historically. **Do not waste a 5-minute slot reproducing a known OOM.** (Repeat for `cuda_error`, `nan_loss` if useful.)
   - Briefly summarize to the user what you learned (2-3 bullets max), so they can correct any misread.
6. **Initialize results.tsv mirror** (optional): Create `results.tsv` with the header row. This is a grep-friendly mirror of resman, not the source of truth.
7. **Confirm and go**: Confirm setup looks good.

Once you get confirmation, kick off the experimentation.

## Experimentation

Each experiment runs on a single GPU. The training script runs for a **fixed time budget of 5 minutes** (wall clock training time, excluding startup/compilation). You launch it simply as: `uv run train.py`.

**What you CAN do:**
- Modify `train.py` — this is the only file you edit. Everything is fair game: model architecture, optimizer, hyperparameters, training loop, batch size, model size, etc.

**What you CANNOT do:**
- Modify `prepare.py`. It is read-only. It contains the fixed evaluation, data loading, tokenizer, and training constants (time budget, sequence length, etc).
- Install new packages or add dependencies. You can only use what's already in `pyproject.toml`.
- Modify the evaluation harness. The `evaluate_bpb` function in `prepare.py` is the ground truth metric.

**The goal is simple: get the lowest val_bpb.** Since the time budget is fixed, you don't need to worry about training time — it's always 5 minutes. Everything is fair game: change the architecture, the optimizer, the hyperparameters, the batch size, the model size. The only constraint is that the code runs without crashing and finishes within the time budget.

**VRAM** is a soft constraint. Some increase is acceptable for meaningful val_bpb gains, but it should not blow up dramatically.

**Simplicity criterion**: All else being equal, simpler is better. When evaluating whether to keep a change, weigh the complexity cost against the improvement magnitude. A ~0 improvement that adds 20 lines of code? Discard. A ~0 improvement from deleting code? Keep.

**The first run**: Your very first run should always be to establish the baseline, so you will run the training script as is.

## Output format

Once the script finishes it prints a summary like this:

```
---
val_bpb:          0.997900
training_seconds: 300.1
total_seconds:    325.9
peak_vram_mb:     45060.2
mfu_percent:      39.80
total_tokens_M:   499.6
num_steps:        953
num_params_M:     50.3
depth:            8
```

You can extract the key metric from the log file:

```
grep "^val_bpb:" run.log
```

## Logging results

**Primary store: resman.** After every run — keep, discard, or crash — call:

```
resman_add_experiment {
  tag:            "<your run tag>",
  commit:         "<short 7-char sha>",
  val_bpb:        <float — use 0 for crashes>,
  memory_gb:      <peak_vram_mb / 1024, .1f — use 0 for crashes>,
  status:         "keep" | "discard" | "crash" | "best",
  description:    "<one-line idea summary>",
  parent_commit:  "<the commit you advanced/reset from>",   # enables lineage
  log_tail:       "<last ~50 lines of run.log>"              # enables auto-signal classify
}
```

`log_tail` is the killer field — resman regex-classifies crashes into typed signals (`oom`, `cuda_error`, `nan_loss`, `assert_fail`, `timeout`) so the next session's `resman_find_by_signal` query returns useful results.

If MCP is unavailable, fall back to CLI:

```bash
resman add -t <tag> -c <commit> -v <bpb> -s <status> -d "<desc>" \
           --parent <parent-commit> --log run.log
```

**Secondary mirror: `results.tsv`** — keep it updated so humans can `grep`. Same 5 columns:

```
commit	val_bpb	memory_gb	status	description
a1b2c3d	0.997900	44.0	keep	baseline
b2c3d4e	0.993200	44.2	keep	increase LR to 0.04
c3d4e5f	1.005000	44.0	discard	switch to GeLU activation
d4e5f6g	0.000000	0.0	crash	double model width (OOM)
```

Format rules: use 0.000000 for crashed val_bpb, 0.0 for crashed memory; `memory_gb = peak_vram_mb / 1024` rounded to .1f. Do **not** commit `results.tsv`.

## The experiment loop

The experiment runs on a dedicated branch (e.g. `autoresearch/apr4`).

LOOP FOREVER:

1. Look at the git state: the current branch/commit we're on.
2. **Consult memory before coding** — for the idea you're about to try:
   - `resman_search { pattern: "<keyword from your idea>" }` — has this been tried? If yes, read the result and **don't re-run the same config**; pivot.
   - `resman_best { composite: true }` — confirm the current best baseline (composite ranks by metric + verified + lineage + description). This is what you must beat.
   - If you're branching from a non-HEAD commit, `resman_lineage { commit: <parent> }` to see whether this chain has already dead-ended.
3. Tune `train.py` with the (now memory-informed) idea by directly hacking the code.
4. `git commit` — note the parent SHA before commit so you can pass it as `parent_commit` later.
5. Run the experiment: `uv run train.py > run.log 2>&1`
6. Read out the results: `grep "^val_bpb:\|^peak_vram_mb:" run.log`. If empty, the run crashed — `tail -n 50 run.log` for the stack trace. Easy bug? Fix and re-run. Fundamental? Mark crash and move on.
7. **Log to resman first, TSV second.** Call `resman_add_experiment` with `{tag, commit, val_bpb, memory_gb, status, description, parent_commit, log_tail}` (see [Logging results](#logging-results)). Then mirror to `results.tsv`. `log_tail` enables auto signal-classification — pass it even on success runs so future `resman_find_by_signal` queries see the full picture.
8. **If your val_bpb is at or near a prior commit's value** (within ~1% directionally), call `resman_verify { commit: <prior-commit>, value: <your-bpb> }` to promote it to `status=verified`. Verified runs feed the composite score and tell future sessions "this is real, not a fluke."
9. If val_bpb improved (lower), advance — keep the git commit. If equal or worse, `git reset --hard HEAD~1` back to where you started.
10. **At the end of every ~10 runs, call `resman_distill { tag: <current-tag> }` and re-read it.** Treat this as your refresh-the-mental-model checkpoint. The distill output is what next session inherits — make sure it tells the story you'd want to inherit.

The idea is that you are a completely autonomous researcher trying things out. If they work, keep. If they don't, discard. And you're advancing the branch so that you can iterate. **Resman is your long-term memory — feed it every run, query it before every idea.**

**Timeout**: Each experiment should take ~5 minutes total. If a run exceeds 10 minutes, kill it and treat it as a failure (discard and revert).

**Crashes**: If a run crashes (OOM, or a bug), use your judgment: If it's something easy to fix (e.g. a typo, a missing import), fix it and re-run. If the idea itself is fundamentally broken, just skip it, log "crash" as the status in the tsv, and move on.

**NEVER STOP**: Once the experiment loop has begun, do NOT pause to ask the human if you should continue. You are autonomous. If you run out of ideas, think harder — read papers referenced in the code, re-read the in-scope files for new angles, try combining previous near-misses, try more radical architectural changes. The loop runs until the human interrupts you, period.
