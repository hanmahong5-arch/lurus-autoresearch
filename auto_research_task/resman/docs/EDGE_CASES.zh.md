# resman 边缘情况与可靠性手册

resman 经过 8 轮对抗式审计硬化（v0.17.2–v0.17.14），下面 8 个场景是这些硬化的**可复现证据**：每个都给出触发命令、**实跑捕获的真实输出**、以及它保护了什么。坏数据进来不会污染存储、不会 panic、不会静默给错结果。

## 复现前置

```bash
cargo build --release && alias resman=./target/release/resman   # v0.17.14
export RESMAN_HOME=/tmp/resman-edge
rm -rf "$RESMAN_HOME"
resman init >/dev/null
```
> `exit=N` 指命令的退出码（`echo "exit=$?"` 可查）。这对脚本/agent 很重要：**机器可据退出码分支**。

---

## E1 — 空存储：报错可执行，机器格式仍可解析

```bash
resman best ; echo "exit=$?"
```
```
error: no experiments found — run `resman import <results.tsv>` or `resman add ...` first (if you haven't created a store yet, run `resman init`)
exit=1
```

```bash
resman list
```
```
no experiments found. try `resman import <results.tsv>` first.
```

```bash
resman search nope -o json ; echo "exit=$?"
```
```
[]
exit=0
```

**保护**：报错指向修复动作（不是裸 `panic`）。`best` 空库**退出非零且绝不吐假浮点**（`$(resman best -f value)` 不会静默吞下垃圾）。`search` 无匹配在 `-o json` 下返回**合法空数组 `[]`**（不是人类散文）——agent 的 `jq` 管道在最常见的「没找到」路径上不会噎住。

---

## E2 — 拒绝非有限 `val_bpb`（inf / nan）

```bash
resman add -t t1 -c aaa1111 -v inf -s keep -d x ; echo "exit=$?"
resman add -t t1 -c aaa1112 -v nan -s keep -d x ; echo "exit=$?"
```
```
error: val_bpb must be finite; crashes use 0.0
exit=1
error: val_bpb must be finite; crashes use 0.0
exit=1
```

**保护（曾是 HIGH 级数据丢失）**：`inf`/`nan` 若落盘会被 `serde_json` 序列化成 JSON `null`，再加载时反序列化失败 → **整个 tag（含所有正常兄弟实验）变得不可读**。现在所有写盘口（`add`、MCP `add`、`import`、`verify`）在落盘前一律拒绝或归零。崩溃请用 `-v 0 -s crash`。

---

## E3 — `import` 含非有限值的行被拒（带行号定位）

```bash
printf 'commit\tval_bpb\tmemory_gb\tstatus\tdescription\nbbb2221\t0.95\t10\tkeep\tok\nbbb2222\tinf\t10\tkeep\tdiverged\n' > /tmp/bad.tsv
resman import /tmp/bad.tsv -t imported ; echo "exit=$?"
```
```
error: val_bpb must be finite; crashes use 0.0 (row 3, got `inf`)
exit=1
```

**保护**：报错**精确定位**到第 3 行、值 `inf`。整批导入中止（坏数据零容忍，和已有的「无法解析的值即报错」语义一致）——`imported` 这个 tag **不会被创建**，正常行 `bbb2221` 也不会半截写入。（注意 Rust 的 `f64::parse` 会接受字面量 `"inf"`/`"1e999"`，所以必须显式 finite 检查。）

---

## E4 — Maximize 指标：`0.0` 是合法值，不是哨兵

```bash
resman add -t acc -c ccc3331 -v 0.0  -s keep -d untrained --metric-name accuracy --metric-direction max --no-gpu-probe
resman add -t acc -c ccc3332 -v 0.92 -s keep -d trained  --parent ccc3331 --metric-name accuracy --metric-direction max --no-gpu-probe
resman best -t acc
```
```
=== resman best ===
  accuracy:    0.920000
  memory_gb:   0.0
  commit:      ccc3332
  status:      ✓ keep
  description: trained
```

```bash
resman list --tag acc
```
```
=== resman list (2 experiment(s)) ===
   #    accuracy   mem_gb    commit  status      description
--------------------------------------------------------------------------------
   1    0.920000      0.0   ccc3332  ✓ keep      trained
   2    0.000000      0.0   ccc3331  ✓ keep      untrained
```

**保护**：方向贯穿全链路（`--metric-direction max`）。列名变成你的 `accuracy`；`best` 取**最高**；`0.0`（未训练基线）**被保留**并按「高者优先」排在 0.92 之后——对 minimize 它才是被排除的哨兵。早期 v0.5 的「越低越好」假设残留曾让 maximize 下的 `0.0` 被误删，现已在 best/stats/near/list/distill/report 全部修正。

---

## E5 — TSV 注入被中和

```bash
resman add -t inj -c ddd4441 -v 0.9 -s keep -d "$(printf 'evil\tcol\nnewline')" --no-gpu-probe
resman list --tag inj -o tsv
```
```
commit	val_bpb	memory_gb	status	description
ddd4441	0.900000	0.0	keep	evil col newline
```

**保护**：描述里塞进的 **Tab 和换行**被 `store::tsv_field` 替换成空格 → 输出仍是**严格一行、恰好 5 列**。否则一个 Tab 就能注入额外列、一个换行就能注入额外行，破坏下游表格解析。所有带自由文本字段的 TSV 发射点（list/near/compare/tree/search/diff）都过这道清洗。

---

## E6 — 多字节 commit 不会 panic（按字符截断）

```bash
resman add -t uni -c 实验一二三四五六七八 -v 0.9 -s keep -d "cjk commit" --no-gpu-probe
resman tree -t uni
```
```
uni: 1 experiment(s), 1 root(s)

实验一二三四五   0.9000   keep      ★ cjk commit    (best)
```

**保护**：10 字符的 CJK commit 被截成**前 7 个字符** `实验一二三四五`。早期按**字节**切片（`commit[..7]`）会在第 7 字节落在多字节码点中间时 panic（`byte index 7 is not a char boundary`）——这类来自 wandb/mlflow 导入的任意 id 是真实输入。tree/tags/usage/distill 现已全部按字符截断。

---

## E7 — 超出容差的 `verify` 被拒（不晋升）

```bash
resman add -t ver -c eee5551 -v 0.90 -s keep -d base --no-gpu-probe
resman verify eee5551 -v 0.95 --tag ver ; echo "exit=$?"
```
```
not verified: eee5551 on tag ver
  metric (val_bpb, minimize)
    original:  0.900000
    new:       0.950000
    delta:     +0.050000
    tolerance: 0.010000  (exceeded by 0.040000)
  status: keep (unchanged)
exit=0
```

**保护**：复现值 `0.95` 比原值 `0.90` 差了 `0.05`，超出默认容差 `0.01` → **拒绝晋升**，状态保持 `keep` 不变，并明确告知「超出 0.040000」。注意 `exit=0`——「未通过验证」是一个**正常结果**而非错误（容差可用 `--tolerance` 调）。绝不让一次侥幸的复现被标成 `verified`。

---

## E8 — 跨 run 混合方向：给出警告（stderr），不静默

接 E4–E7，存储里现在既有 maximize（`acc`）又有 minimize（`inj`/`uni`/`ver`）。跨 run 求 `best`：

```bash
resman best          # stdout=赢家；stderr=警告
```
```
warning: comparing runs with different directions (maximize vs minimize); using first run's direction
warning: comparing runs with different directions (maximize vs minimize); using first run's direction
warning: comparing runs with different directions (maximize vs minimize); using first run's direction
=== resman best ===
  accuracy:    0.920000
  memory_gb:   0.0
  commit:      ccc3332
  status:      ✓ keep
  description: trained
```

**保护**：把不同方向的指标放一起比本就无意义（这里按第一个 run 的方向 maximize 比较，于是 0.92 胜出）。resman **照常给出一个结果，但向 stderr 明确警告**——`best`/`stats`/`compare`/`list` 一致地这么做。要消除歧义就用 `--tag` 限定到单个 run。（警告走 stderr，不污染 stdout 的机器格式。）

---

## 这些保证一句话

> **坏数据进不了存储**（非有限值在所有写盘口被拒）、**任意输入不 panic**（全部按字符截断、无未受保护的切片/`unwrap`）、**机器格式永远可解析**（`best -f value` 单浮点、`-o json/tsv` 字节稳定且清洗）、**方向歧义不静默**（跨 run 混合方向必警告）。

对应的 CLI↔MCP 一致性、完整不变量与红线，见仓库 `CHANGELOG.md` v0.17.10–v0.17.14。
