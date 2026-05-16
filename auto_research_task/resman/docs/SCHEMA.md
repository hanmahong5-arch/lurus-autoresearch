# resman schema — v0.8 status and v1.0 freeze plan

resman 升 v1.0 launch 前要冻结 JSON-on-disk schema。本文档记录字段级决策：哪些已稳、哪些 v1.0 前要做、哪些 v1.0 后再改。

## Field stability

### `Experiment` (src/model.rs)

| 字段 | 类型 | 引入 | 决策 |
|---|---|---|---|
| commit | String | v0.1 | stable, stay |
| val_bpb | f64 | v0.1 | **rename → `primary_metric` before v1.0** (pre-v1.0 单独 PR, 含 `#[serde(alias = "val_bpb")]` 兼容 v0.1-v0.8 stores) |
| memory_gb | f64 | v0.1 | **rename → `peak_memory_gb` before v1.0** (同上 PR, 含 alias) |
| status | Status | v0.2 | stable (enum, `#[serde(rename_all = "lowercase")]`) |
| description | String | v0.1 | stable |
| timestamp | String | v0.2 | stable (ISO 8601 / RFC 3339, 故意 String 不 chrono — jq 友好) |
| params | HashMap\<String,String\> | v0.2 | stable (String-keyed 是 agent-CLI 工效选择) |
| parent_commit | Option\<String\> | v0.3 | stable (Option 正确建模 v0.1/v0.2 无谱系) |
| crash_excerpt | Option\<String\> | v0.3 | stable (与 signals 并存，作为 Unknown signal 的取证) |
| metric_name | Option\<String\> | v0.5 | stable |
| metric_direction | Option\<Direction\> | v0.5 | stable |
| signals | Vec\<Signal\> | v0.6 | stable post-v0.8（本次 PR 加 DivergedLoss/SlowMfu 后枚举锁定） |

### `RunLog` (src/model.rs)

| 字段 | 类型 | 引入 | 决策 |
|---|---|---|---|
| experiments | Vec\<Experiment\> | v0.1 | stable |
| run_tag | String | v0.1 | stable |
| created_at | String | v0.1 | stable (同 timestamp 理由) |
| metric_name | Option\<String\> | v0.5 | stable |
| metric_direction | Option\<Direction\> | v0.5 | stable |

### `Signal` enum (src/signals.rs)

| 变体 | 引入 | 状态 |
|---|---|---|
| Oom | v0.6 | stable |
| CudaError { hint } | v0.6 | stable |
| NanLoss | v0.6 | stable |
| AssertFail { location } | v0.6 | stable |
| Timeout | v0.6 | stable |
| Unknown { pattern } | v0.6 | stable |
| **DivergedLoss { detail }** | **v0.8 (this PR)** | stable |
| **SlowMfu { mfu_percent }** | **v0.8 (this PR)** | stable |

## v1.0 freeze decisions

### Decision 1 — Composite weights frozen on hardcoded 0.5/0.2/0.2/0.1

`best.rs` 当前 hardcoded `0.5 × metric + 0.2 × verified + 0.2 × lineage + 0.1 × desc`. 不暴露为 schema 字段。

理由：调参数据未到（usage.jsonl 还在累积）；暴露字段会拖大 schema 但无据可调；v1.0 launch 后若需要按 schema-versioned migration 加字段（additive, 不破老 stores）。

### Decision 2 — `val_bpb` / `memory_gb` rename 推迟到 dogfood 后单独 PR

理由：rename 涉及 50+ 处代码 ref（commands/best.rs, distill, near, list, mcp.rs, report.rs 等），PR 风险大；dogfood 后 usage.jsonl 数据到来时一并升级；用 `#[serde(alias = "val_bpb")]` / `#[serde(alias = "memory_gb")]` 保 v0.1-v0.8 stores 兼容。

实施时机：第一次 dogfood 跑完、有 usage.jsonl 数据驱动权重调参 PR 时一并 ship。

### Decision 3 — 不加 `#[serde(deny_unknown_fields)]`

理由：保 forward-compat（v0.8 reader 能 load v1.0+ stores 中未知字段，silently drop）；typo guard 用外部 schema 验证工具。

### Decision 4 — 不实现 `resman migrate`

理由：`#[serde(alias = ...)]` 足够覆盖 rename 场景；resman 是 per-run JSON-on-disk 不是 DB，无 ALTER TABLE 需求。

## Backwards-compat audit

v0.8 加 DivergedLoss/SlowMfu 后，v0.6/v0.7 stores 仍可 load —— `Vec<Signal>` 的 `#[serde(default)]` 保证 pre-v0.6 records 反序列化为 empty Vec；v0.6+ records 反序列化未变化（未知 enum variant 不在老数据里）。

无需 migration。

## 拒绝过的设计（保留以避免反复提出）

见 `README.md` "What resman is NOT" 节：SQLite/DB 后端、网络账户、Python deps、`status=verified` 经 `resman add` 设置（仅 `resman verify` 可促进）。
