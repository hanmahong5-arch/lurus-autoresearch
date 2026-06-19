# resman 实操教程（一次完整的过夜训练循环）

本文用**一个连贯的真实场景**走完 resman 的核心环路：基线 → 改进 → 走死的分支（discard）→ 一次 OOM 崩溃 → 恢复并刷新最优 → 复现验证。每条命令的输出都是 **v0.17.14 实跑捕获**的，照抄即可复现。

---

## 0. 如何复现本文

```bash
# 装：发布后用 cargo，或在仓库内编译
cargo install resman-cli            # 方式 A（crates.io 发布后）
# cargo build --release && alias resman=./target/release/resman   # 方式 B（仓库内）

# 所有示例共享一个干净的数据目录
export RESMAN_HOME=/tmp/resman-demo
rm -rf "$RESMAN_HOME"
```

> **环境相关字段（不影响复现核心数据）**：输出里的 `timestamp`、自动探测的 `params.gpu`（来自 `nvidia-smi`）、以及解析后的绝对路径（下文统一记为 `<DATA_DIR>`）会因你的机器/时间而异。其余全部确定。想要完全确定的输出，给 `add` 加 `--no-gpu-probe`。Windows 的 Git Bash 里 `/tmp` 会映射到 `%LOCALAPPDATA%\Temp`，路径串不同但数据一致。

---

## 1. 初始化

```bash
resman init
```
```
initialized resman data directory: <DATA_DIR>
  runs/    — per-run experiment logs (one JSON each)

next: `resman import results.tsv` or `resman add --tag <t> ...`
```

resman 没有数据库、没有服务、没有账号——一个目录、每个 run 一个 JSON 文件（原子写：先写 `<tag>.json.tmp` 再 rename，agent 半路崩溃也不会损坏存储）。

---

## 2. 环路的核心：每次训练后 `resman add`

一次实验一行 `add`。`tag` 首次使用即创建，之后追加。**从第二条起务必带 `--parent`**（指向你 advance/reset 的那个 commit），否则后续 `distill`/`tree` 无法重建血缘。

```bash
# ① 基线（根节点，无 parent）
resman add -t overnight -c a1b2c3d -v 0.995 -m 14.2 -s keep -d "baseline: 6-layer, lr=3e-4"
# ② 旋转位置编码（更优）
resman add -t overnight -c b2c3d4e -v 0.982 -m 14.4 -s keep -d "rotary embeddings" --parent a1b2c3d
# ③ 换 gelu（更差 → discard）
resman add -t overnight -c c3d4e5f -v 0.989 -m 14.3 -s discard -d "swap relu->gelu" --parent b2c3d4e
# ④ 加大 batch → OOM 崩溃（把日志尾巴喂进去）
printf 'step 142 loss 3.21\nRuntimeError: CUDA out of memory. Tried to allocate 4.00 GiB\n' > /tmp/oom.log
resman add -t overnight -c d4e5f6a -v 0 -s crash -d "batch 64 -> 128" --parent b2c3d4e --log /tmp/oom.log
# ⑤ 梯度检查点 + 大 batch（恢复，刷新最优）
resman add -t overnight -c e5f6a7b -v 0.974 -m 12.1 -s keep -d "grad checkpoint + batch 128" --parent b2c3d4e
```

每条都会回显「第几条 / 当前最优 / 落盘路径」。第 ④ 条因为带了 `--log`，自动把崩溃日志分类成了 **`oom`** 信号：

```
added experiment #4 to `overnight` (crash)
  signals: oom
  current best: val_bpb=0.982000 (b2c3d4e)
  saved: <DATA_DIR>/runs/overnight.json
added experiment #5 to `overnight` (keep)
  current best: val_bpb=0.974000 (e5f6a7b)
  saved: <DATA_DIR>/runs/overnight.json
```

> **崩溃也要记。** `-v 0`（崩溃哨兵值）+ `-s crash` + `--log`，resman 用正则把日志分类成 `oom`/`cuda_error`/`nan_loss`/`assert_fail`/`timeout`。这样过夜后你能直接问「昨晚 OOM 了几次、在哪些方向」。

---

## 3. 「现在最优是多少」——`best`

三种输出，对应人 / shell 脚本 / agent：

```bash
resman best            # 人看
```
```
=== resman best ===
  val_bpb:     0.974000
  memory_gb:   12.1
  commit:      e5f6a7b
  status:      ✓ keep
  description: grad checkpoint + batch 128
```

```bash
resman best -f value   # shell API：只吐一个浮点，无前缀无颜色
```
```
0.974000
```
> 这是公开契约。`THRESHOLD=$(resman best -f value)` 可直接进脚本——永远只有一个 6 位小数浮点。

```bash
resman best -f json    # agent / jq
```
```json
{"commit":"e5f6a7b","val_bpb":0.974,"memory_gb":12.1,"status":"keep","description":"grad checkpoint + batch 128","timestamp":"…","params":{"gpu":"…"},"parent_commit":"b2c3d4e"}
```

---

## 4. 列表与三种格式——`list`

默认只显示 `keep`：

```bash
resman list
```
```
=== resman list (3 experiment(s)) ===
   #     val_bpb   mem_gb    commit  status      description
--------------------------------------------------------------------------------
   1    0.974000     12.1   e5f6a7b  ✓ keep      grad checkpoint + batch 128
   2    0.982000     14.4   b2c3d4e  ✓ keep      rotary embeddings
   3    0.995000     14.2   a1b2c3d  ✓ keep      baseline: 6-layer, lr=3e-4
```

> **常见坑**：`list` 默认只看 `keep`。要看崩溃/丢弃历史，加 `--status all`（或 `--status crash`）。

```bash
resman list --status all
```
```
=== resman list (5 experiment(s)) ===
   #     val_bpb   mem_gb    commit  status      description
--------------------------------------------------------------------------------
   1    0.000000      0.0   d4e5f6a  ✗ crash     batch 64 -> 128
   2    0.974000     12.1   e5f6a7b  ✓ keep      grad checkpoint + batch 128
   3    0.982000     14.4   b2c3d4e  ✓ keep      rotary embeddings
   4    0.989000     14.3   c3d4e5f  · discard   swap relu->gelu
   5    0.995000     14.2   a1b2c3d  ✓ keep      baseline: 6-layer, lr=3e-4
```

按信号筛 + JSON 输出（给 agent/jq）：

```bash
resman list --status all --signal oom -o json
```
```json
[
  {
    "commit": "d4e5f6a", "val_bpb": 0.0, "memory_gb": 0.0,
    "status": "crash", "description": "batch 64 -> 128",
    "timestamp": "…", "params": { "gpu": "…" },
    "parent_commit": "b2c3d4e",
    "crash_excerpt": "step 142 loss 3.21\nRuntimeError: CUDA out of memory. Tried to allocate 4.00 GiB",
    "signals": [ { "type": "oom" } ]
  }
]
```

---

## 5. 「试过没 / 这个值附近还有啥」——`search` 与 `near`

```bash
resman search rotary
```
```
run                val_bpb    commit    status  description
----------------------------------------------------------------------------------------
overnight         0.982000   b2c3d4e  keep  rotary embeddings

1 match(es). → idea already explored; consider a variation.
```

```bash
resman near 0.98 -n 3
```
```
neighbors of val_bpb=0.980000 (closest first):

run                val_bpb           Δ    status  description
----------------------------------------------------------------------------------------
overnight         0.982000   +0.002000  keep  rotary embeddings
overnight         0.974000   -0.006000  keep  grad checkpoint + batch 128
overnight         0.989000   +0.009000  discard  swap relu->gelu
```
> 注意崩溃的 `0.000000` **不**出现在 `near` 里——对 minimize 指标，`0.0` 是哨兵值，被正确排除。

---

## 6. 血缘树——`tree`

`--parent` 串起来的 DAG。`★` 标记「最优血缘链」上的节点，`(best)` 标记当前最优：

```bash
resman tree -t overnight
```
```
overnight: 5 experiment(s), 1 root(s)

a1b2c3d   0.9950   keep      ★ baseline: 6-layer, lr=3e-4
└── b2c3d4e   0.9820   keep      ★ rotary embeddings
    ├── c3d4e5f   0.9890   discard     swap relu->gelu
    ├── d4e5f6a   0.0000   crash       batch 64 -> 128
    └── e5f6a7b   0.9740   keep      ★ grad checkpoint + batch 128    (best)
```
一眼看清：基线→rotary 是主干，gelu 走死、大 batch OOM 都从 rotary 分叉，最后梯度检查点从 rotary 恢复成最优。

---

## 7. 聚合统计——`stats`

```bash
resman stats -t overnight
```
```
=== resman stats (overnight) ===

total:       5
kept:        3  (60.0%)
discarded:   1  (20.0%)
crashed:     1  (20.0%)

val_bpb:
  best:        0.974000
  worst:       0.995000
  mean:        0.983667
  stddev:      0.008654
  improvement: 0.021000  (2.11%)
  bpb-drop per experiment: 0.007000
```

---

## 8. 长期记忆——`distill`（「昨晚学到了啥」）

这是 agent 跨会话记忆的核心产物：最优 + 血缘 + 失败信号聚类 + 未探索邻居 + 启发式建议，**无 LLM、纯规则**。

```bash
resman distill -t overnight
```
```markdown
# Distill: overnight

_Generated from 5 experiments (3 keep, 0 verified, 1 discard, 1 crash, 0 best). Metric: val_bpb (minimize)._

## Best result
- **val_bpb**: `0.974000`
- **commit**: `e5f6a7b`
- **description**: grad checkpoint + batch 128
- **GPU**: unspecified

## Lineage to best
  `a1b2c3d` ✓ val_bpb=0.995000  baseline: 6-layer, lr=3e-4
  `b2c3d4e` ✓ val_bpb=0.982000  rotary embeddings
  `e5f6a7b` ✓ val_bpb=0.974000  grad checkpoint + batch 128

## Failure signals

### oom (1)
- `d4e5f6a` — batch 64 -> 128

## Unexplored neighbors
- `b2c3d4e` val_bpb=0.982000 (Δ=-0.0080) — rotary embeddings
- `c3d4e5f` val_bpb=0.989000 (Δ=-0.0150) — swap relu->gelu
- `a1b2c3d` val_bpb=0.995000 (Δ=-0.0210) — baseline: 6-layer, lr=3e-4

## Suggestions
1. Best experiment is unverified — re-run and call `resman verify e5f6a7b --value <new>` to promote to verified status before you rely on it.

---
_resman distill v0.17.14 — …_
```
注意末尾的**建议**：最优还没验证，distill 主动提醒去复现验证。下一步就照做。

---

## 9. 复现验证——`verify`

你**自己**重跑那个实验，把新测得的值传进来；在容差内（默认绝对 `0.01`、按指标方向）就晋升为 `verified`，并更新 `val_bpb`：

```bash
resman verify e5f6a7b -v 0.975 --tag overnight
```
```
verified e5f6a7b on tag overnight
  metric (val_bpb, minimize)
    original:  0.974000
    new:       0.975000
    delta:     +0.001000
    tolerance: 0.010000
  status: keep → verified ✔
```

再看 `best`——状态已是 `✔ verified`，值更新为复现值 `0.975`：

```bash
resman best
```
```
=== resman best ===
  val_bpb:     0.975000
  memory_gb:   12.1
  commit:      e5f6a7b
  status:      ✔ verified
  description: grad checkpoint + batch 128
```
> 容差是**单向**的：minimize 指标下，`new <= original + tol` 才算通过——只在「更差」一侧设界。若复现明显偏离则**拒绝晋升**（见《边缘情况手册》E7）。后悔了可 `resman unverify <commit>` 退回 `keep`（值保留）。

---

## 10. 自包含 HTML 报告——`report`

```bash
resman report /tmp/overnight.html
```
```
html report written to: <DATA_DIR-or-path>/overnight.html
```
单文件、无外链、含 SVG 趋势图（双主题，跟随系统亮/暗）。可直接邮件/归档/分享，实测约 10 KB。

---

## 一句话环路契约

> **先记 resman，再写 TSV，最后 git commit。只有 resman 记下失败后，才 `git reset --hard HEAD~1`——一旦 reset，resman 就是「试过什么」的唯一记忆。**

下一步：把这套接进 agent（见 [`MCP.zh.md`](MCP.zh.md)），异常/边界行为见 [`EDGE_CASES.zh.md`](EDGE_CASES.zh.md)。
