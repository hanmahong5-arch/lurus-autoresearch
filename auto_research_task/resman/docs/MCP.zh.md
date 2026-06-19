# resman 接入 agent（MCP）实操

`resman mcp` 是一个 **stdio 上的 JSON-RPC 2.0 服务**，把存储暴露成 **17 个结构化工具**，agent 直接调用——不用在上下文里塞 CLI、不用解析 stdout、不用处理 bash 转义。这是 resman 面向 agent 的**主接口**（CLI 是人类后备）。

本文命令均为 v0.17.14 实跑验证。

---

## 1. 装

```bash
cargo install resman-cli            # 发布后
# 或仓库内：cargo build --release（二进制在 target/release/resman）
```

## 2. 接（`.mcp.json`）

**Claude Code** — 写进 `.claude/mcp.json`（或全局等价物）：
```json
{
  "mcpServers": {
    "resman": {
      "command": "resman",
      "args": ["mcp"],
      "env": { "RESMAN_HOME": "/abs/path/to/.resman" }
    }
  }
}
```
重启后工具以 `mcp__resman__resman_best` 等形式出现。

**Cursor** — `~/.cursor/mcp.json`：
```json
{ "mcpServers": { "resman": { "command": "resman", "args": ["mcp"] } } }
```

**任意 harness** — 协议是「一行一条 JSON-RPC」走 stdio，直接 `resman mcp` 启动即可。

---

## 3. 验证服务（手搓握手，实测）

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' \
  '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"resman_best","arguments":{}}}' \
  | resman mcp
```

启动时 resman 向 **stderr** 打印一行（不污染 stdout 的协议流）：
```
resman-mcp v0.17.14: listening on stdio (data_dir=<DATA_DIR>)
```

stdout 收到**恰好 3 条** JSON-RPC 响应（用上一篇教程的 `overnight` 存储跑的）：

- **① initialize** → `protocolVersion: "2024-11-05"`，`serverInfo: {"name":"resman","version":"0.17.14"}`，并带一个 `instructions` 字段（告诉 LLM 何时调哪个工具，省去定制 prompt）。
- **② tools/list** → **17** 个工具，前几个：`resman_best`、`resman_search`、`resman_near`、`resman_list_recent`、`resman_add_experiment`…
- **③ tools/call `resman_best`** → `content[0].text` 为（`isError: false`）：

```json
{"commit":"e5f6a7b","composite":null,"description":"grad checkpoint + batch 128","memory_gb":12.1,"metric":"val_bpb","status":"verified","tag":"overnight","value":0.975}
```

> 没有 `jq` 也行——管道接 `python -c "import sys,json; [print(json.loads(l)['id']) for l in sys.stdin]"` 即可逐条解析。工具级错误是成功 JSON-RPC 响应里的 `isError: true`（LLM 可读可重试），传输级失败才是 JSON-RPC `error`。

---

## 4. 环路只需三个调用

```jsonc
// ── 会话开始：昨晚学到了啥？
resman_distill { "tag": "overnight" }
//   → best + 血缘 + 失败信号聚类 + 未探索邻居 + 建议（你的长期记忆）

// ── 试想法前：是不是已经试过？
resman_search { "pattern": "rotary embeddings" }
//   → 避免重复劳动

// ── 每次训练后（keep / discard / 甚至 crash 都要记）：
resman_add_experiment {
  "tag": "overnight", "commit": "a1b2c3d", "val_bpb": 0.981,
  "status": "keep", "parent_commit": "f0e9d8c", "log_tail": "<run.log 末 50 行>"
}
```
其余（`resman_verify` 复现验证、`resman_diff_tags` 比分支、`resman_find_by_signal` 按信号 triage、`resman_best{composite:true}` 多维打分）按需取用。

> `log_tail` 是杀手字段：成功也传——resman 会把它正则分类成 `oom`/`cuda_error`/`nan_loss`/`assert_fail`/`timeout`，喂养后续 `resman_find_by_signal`。

---

## 5. 17 个工具速查（按生命周期）

| 时机 | 工具 |
|---|---|
| 会话开始（健康检查） | `resman_doctor` |
| 会话开始（发现历史） | `resman_list_recent` / `resman_tags` |
| 会话开始（读记忆） | `resman_distill` |
| 试想法前 | `resman_search`（试过没）/ `resman_best`（基线）|
| 分叉规划 | `resman_lineage` |
| 每次训练后 | `resman_add_experiment` |
| 复现后 | `resman_verify` / `resman_unverify` |
| 中途 triage | `resman_list` / `resman_find_by_signal` / `resman_near` |
| 比较 | `resman_diff_tags` / `resman_compare` / `resman_stats` |
| 自审 | `resman_usage` |

完整参数与语义见英文版 [`MCP.md`](MCP.md) 与给 LLM 的 [`AGENT_QUICKSTART.md`](AGENT_QUICKSTART.md)。

---

## 6. CLI JSON 与 MCP JSON 是两套接口（重要）

同一个「最优」，两条接口的**键名/形状不同**——这是有意为之，各自向各自的消费者冻结：

| | CLI `best -f json` | MCP `resman_best` |
|---|---|---|
| 指标 | `val_bpb` | `metric` + `value` |
| run 标识 | （无顶层 tag） | `tag` |
| 复合分 | （无） | `composite`（`null` 或对象）|
| 其余 | `commit/memory_gb/status/description/parent_commit/timestamp/params` | `commit/memory_gb/status/description` |

实测对照（同一最优实验）：
```jsonc
// CLI: resman best -f json
{"commit":"e5f6a7b","val_bpb":0.975,"memory_gb":12.1,"status":"verified", ...}
// MCP: resman_best
{"commit":"e5f6a7b","metric":"val_bpb","value":0.975,"tag":"overnight","composite":null,"status":"verified", ...}
```

> **别用同一个 parser 同时吃两边。** agent 走 MCP、人/脚本走 CLI `-o json`。两套形状的统一是 **v1.0 schema 冻结**的事（现在改任一面都会破坏该面已有的消费者）。

---

## 7. 排错

- MCP 工具列表里看不到 `resman_*` → 先 `resman doctor` 看 `mcp_wiring` 检查项的 `hint`；临时退回 CLI（`resman list --top 20 -o json` / `resman distill -t <tag>`）。
- `RESMAN_DISABLE_USAGE_LOG=1` 关闭遥测；遥测写失败只在 stderr 提示一次、绝不影响工具调用。
- 每次 `tools/call` 追加一行到 `<DATA_DIR>/usage.jsonl`（本地文件，不外传）——这是 v1.0 前调权重/模板的数据源，也可用内置 `resman usage` 直接看。
