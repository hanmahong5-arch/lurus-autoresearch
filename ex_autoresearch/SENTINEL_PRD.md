# Sentinel PRD —「常驻研究员」产品定义

> 状态:**草案 / 待 review**。本文件是产品级决策的白纸,先 review 再动代码。
> 工作代号 **Sentinel**(常驻研究员)——名字本身是开放决策(见 §11)。
> 适用范围:`ex_autoresearch/`(Elixir 深度研究 web agent)。**不得**与 `resman` 交叉污染(CLAUDE.md 红线)。

---

## 0. TL;DR

把现在这个"一次性深度研究 demo",重组成一个 **hosted Deep Research 结构上做不出来的品类**:

> **定义一次研究课题 → 现在给你一份经核查、带逐句引用的报告 → 之后按周期自动给你「这周变了什么、为什么重要」的 delta 简报。私有部署、多租户、全程可审计。**

仓库已有 **70% 的管道**(Oban / Ash 多租户 / LiveView Mission Control / Template+Schedule 骨架)。缺的是两块新核心:**信任层(Trust Layer)** 和 **Delta 引擎**,外加把运行引擎从"全系统单并发 GenServer"升级为"job-per-run"。

---

## 1. 业务最痛的点(为什么这个产品有人付钱)

**痛点原话:**
> "我每周手动重跑同一批研究——盯竞品、盯监管、盯赛道。Perplexity 跑完就忘、不能定时、引用不敢信、数据不能留在自己机器上;Crayon/Klue 这类监控工具只告诉我'某页变了',不帮我做开放式综合研究。我要一个不睡觉、可审计、可私有部署的研究员。"

**付钱的人(都有预算、痛感急):**

| 买家 | 现在怎么受苦 | 预算来源 |
|---|---|---|
| 竞争情报 / 公司战略 | 每周手搓竞品 brief 塞 PPT | 战略/市场预算 |
| 合规 / 监管(医药·金融·法律) | 必须盯监管动态、**需可审计出处**、**数据不能出公司** | 合规预算(最舍得花) |
| VC/PE deal team | 持续盯标的与赛道,信息散落 | 投研预算 |

**为什么现在(市场窗口):** 2026 上半年 agent/deep-research 是最热品类,但:
- hosted Deep Research(Perplexity / Gemini / OpenAI)是 **SaaS 单发**——无自托管、无定时复发、无多租户、无审计轨迹、无源域管控。
- 监控类 SaaS(Crayon / Klue / Contify)只做 change-alert,**不做开放式综合研究**。
- 自托管开源(GPT-Researcher 等)**无多租户、无调度、无 versioned 存储、无实时 UI**。

→ `深度研究 × 定时复发 × 结构化 delta × 自托管 × 可审计` 这个交叉点 **无人占领**。

---

## 2. 产品定义

**一句话:** Sentinel 是一个常驻的、可私有部署的研究员——你定义一次 Brief(研究课题 + 周期 + 源策略),它现在产出一份经核查的引用报告,之后按周期自动复跑并推送"变了什么"的 delta 简报。

**它不是"更好的 Perplexity",是 Perplexity 进不来的那块地。**

**核心循环:**
```
定义 Brief → (Oban 定时触发) → 计划→搜索→分析→[核查]→综合
          → 结构化报告(逐句引用 + 置信度) → 存为 versioned 快照
          → 与上一版 diff → "变了什么 + 为什么重要" 简报 → 收件箱/Slack/邮件
```

---

## 3. 护城河(为什么是我们能做)

| 产品要素 | 需要的能力 | 仓库已有 |
|---|---|---|
| 定时不睡觉 | durable 后台 job、cron、survive 重启/重试 | **Oban + ash_oban**(已装,后者未用)✅ |
| 跨时间 delta + 审计 | versioned、多租户、可查询的结构化记录 | **Ash + AshSqlite,多租户已开**(`:attribute` on `:organization_id`)✅ |
| 信任 = 实时透明 | 边跑边看 plan/子查询/逐源打分 | **LiveView + Mission Control + Narrative 面板**(刚做完)✅ |
| 定义 brief + 排程 | 模板 + 调度界面 | **TemplateLive + ScheduleLive + TemplateScheduler**(骨架在)✅ |

**hosted 进不来:** 要做这个得暴露它们刻意抽象掉的基础设施(自托管/定时/审计)。**自托管开源进不来:** 得从零补调度+versioned 存储+多租户。**我们三层都在跑。**

前沿三条逆向洞察验证方向:**"引用准确度 > 来源数量"、"人在环 > 全自主"、"长 context 单 agent > 复杂编排"**——都指向"比可信/可持续,不比炫/自主"。

---

## 4. 现状硬伤(必须在产品化前修掉)

来自代码实测(deep_research 管线):

| 现状 | 问题 | 产品影响 |
|---|---|---|
| `quality_score = min(平均字节数/2000, 1.0)` | 纯字节量,与相关性无关 | delta/信任的基础分是假的 |
| 正文引用 vs 末尾来源列表脱节 | 合成 prompt 收不到编号引用表 → **inline 引用是 LLM 幻觉** | 报告不可信 = 产品无价值 |
| 跨 query 不去重 URL | 同页重复计为多源 | 源统计/delta 失真 |
| `SearchQualityMonitor` 信号孤儿 | 广播了但 orchestrator 不订阅 | 质量决策无效 |
| 内容硬截断 2000 字符 | 深页(论文/法规)被砍 | 综合质量受限 |
| 跨源一致性检测 | 完全没有 | 无法做矛盾标红 |
| 单 `ResearchOrchestrator` GenServer | **全系统只能跑一个研究** | 多租户/定时彻底跑不起来 |

---

## 5. 架构设计(SOTA + 复用现有 + 修硬伤)

### 5.1 运行引擎:job-per-run 取代全局 GenServer

**现状:** `ResearchOrchestrator` 是单例 GenServer,全系统单并发 + 重启即丢。`ResearchWorker`(`use Oban.Worker, queue: :default, max_attempts: 3`)已存在但 orchestrator 逻辑没搬进去。

**目标:** 每次研究运行 = 一个 Oban job。orchestrator 的 phase 状态机逻辑移入 `ResearchWorker`(或由 worker 起一个 per-run 进程驱动)。

- 并发:N 个租户的 N 个 Brief 同时跑,互不阻塞。
- durable:deploy/崩溃后 Oban 重试(`max_attempts`)。
- 专用队列:`queue: :research`(与默认队列隔离,可限流)。
- **定时:用 `ash_oban`(已装未用)在 `Brief` 上挂 scheduled trigger,取代手搓的 `TemplateScheduler` GenServer。**

### 5.2 Phase 机器升级

现有:`planning → searching → analyzing → (deepening) → writing → completed`。
**新增 `verifying` 阶段**(在 `writing` 之后、`completed` 之前):

```
planning → searching → analyzing → (deepening) → writing → verifying → completed
                                                              │
                                          抽取 atomic claims → 逐 claim 比对引用源
                                          → grounding 分类 + 置信度 → 矛盾标红
```

运行结束后入队 `DeltaWorker`:与上一版报告 diff → 生成 Delta + 推送。

### 5.3 信任层(产品命门,前沿对齐)

1. **相关性分取代字节量** `relevance_score`:MVP 用 LLM relevance judge(query↔内容,0–1);后续可换 embedding 相似度。
2. **Verifier-Critic pass**:独立一遍,把报告拆成 atomic claims,每条 claim 拿其引用 `Source` 内容做 grounding 判定 → `grounded / contradicted / unsupported / complementary`(参 GSAR / typed claim grounding)。矛盾/无据 claim 在报告里标红或脚注。
3. **逐 claim 结构化引用**:`Claim → Source`(哪个源支撑)+ `origin_subquery`(哪个子查询找到它)+ `confidence`。干掉幻觉引用。

> **一物三用(关键设计):** `Claim` 资源**同时**是逐句引用、审计轨迹、**和 delta 的最小比较单元**。一套 schema 撑起信任 + 审计 + delta 三个卖点。

### 5.4 Delta 引擎(独占品类)

- 每次 Brief 运行产出一个 versioned `Report`(加 `brief_id` + `run_version`)。
- 跨两版按 `Claim.claim_hash` 匹配:`added / removed / changed / contradicted`。
  - MVP:`claim_hash` = 归一化文本 hash + 模糊匹配;后续上 embedding 桶。
- LLM 从结构化 diff 生成"为什么重要"的 `Delta.markdown_digest`。
- 推送:LiveView 收件箱(未读高亮)+ Slack/邮件/webhook。

### 5.5 合规解锁(企业付费点,建在信任层之上,近乎免费)

- 每租户**源域 allow/block**:`Brief.allow_domains` / `block_domains`,在 scrape 阶段强制(数据不出公司、屏蔽竞品源)。
- **claim 级审计导出**:`Claim` 表直接导出 CSV/JSON(法律/医药/金融刚需)。

---

## 6. Schema 草案(贴 house style:`AshSqlite` + 多租户 + `uuid_v7`)

> 约定参照现有 `Report` / `Investigation` / `Template`:`use Ash.Resource, domain: ExAutoresearch.Research, data_layer: AshSqlite.DataLayer`;`sqlite do table .. repo ExAutoresearch.Repo end`;`multitenancy do strategy :attribute; attribute :organization_id end`;`uuid_v7_primary_key :id`;命名 action(`create :start` 等)。

### 6.1 `Brief`(新)— 常驻研究订阅(产品 primitive)

> 与现有 `Template` 重叠(都有 query/cron/enabled)。**决策点(§11):** 是扩展 `Template` 还是新建 `Brief`。本草案按新建写,因其生命周期(cadence/last_run/next_run/订阅者/源策略/delta 设置)显著不同于"一键启动模板"。

```elixir
use Ash.Resource, domain: ExAutoresearch.Research, data_layer: AshSqlite.DataLayer

sqlite do table "briefs"; repo ExAutoresearch.Repo end
multitenancy do strategy :attribute; attribute :organization_id end

attributes do
  uuid_v7_primary_key :id
  attribute :name,        :string, allow_nil?: false
  attribute :question,    :string, allow_nil?: false          # 研究课题
  attribute :category,    :atom,   constraints: [one_of: [:competitor, :market, :policy, :trend, :custom]], default: :custom
  attribute :cadence,     :string                              # cron 表达式;nil = 仅手动
  attribute :enabled,     :boolean, default: false
  attribute :organization_id, :uuid_v7, allow_nil?: false
  attribute :model,       :string, default: "claude-sonnet-4"
  attribute :max_depth,   :integer, default: 3
  attribute :max_sources, :integer, default: 25
  attribute :allow_domains, {:array, :string}, default: []     # 源策略(空=不限)
  attribute :block_domains, {:array, :string}, default: []
  attribute :notify_channels, {:array, :string}, default: []   # ["inbox","slack:...","email:..."]
  attribute :last_run_at, :utc_datetime_usec
  attribute :next_run_at, :utc_datetime_usec
  timestamps()
end

actions do
  defaults [:read]
  create :create do accept [:name, :question, :category, :cadence, :enabled, :organization_id,
                            :model, :max_depth, :max_sources, :allow_domains, :block_domains, :notify_channels]; primary? true end
  update :update do accept [:name, :question, :category, :cadence, :model, :max_depth, :max_sources,
                            :allow_domains, :block_domains, :notify_channels] end
  update :toggle do accept [:enabled] end
  update :mark_ran do accept [:last_run_at, :next_run_at] end
end

relationships do
  has_many :reports, ExAutoresearch.Research.Report
  has_many :deltas,  ExAutoresearch.Research.Delta
  belongs_to :organization, ExAutoresearch.Accounts.Organization
end
```

`ash_oban` scheduled trigger(取代 TemplateScheduler):
```elixir
# 伪代码:on Brief, 每分钟扫一次到期的 enabled brief,入队 ResearchWorker
oban do
  triggers do
    trigger :run_due do
      action :read
      scheduler_cron "* * * * *"
      where expr(enabled == true and next_run_at <= now())
      worker_module_name ExAutoresearch.Workers.ResearchWorker
    end
  end
end
```

### 6.2 `Report`(扩展现有)— 一次运行 = 一个 versioned 快照

**只加两个字段,复用全部现有机器(status/markdown_body/token 追踪/investigations):**
```elixir
attribute :brief_id,    :uuid_v7              # nil = 一次性研究(向后兼容)
attribute :run_version, :integer, default: 1  # 同一 Brief 下递增
# relationships: belongs_to :brief, ExAutoresearch.Research.Brief
# has_many :claims, ExAutoresearch.Research.Claim
# has_many :sources, ExAutoresearch.Research.Source
```

### 6.3 `Source`(新)— 结构化引用(支撑 URL 去重 + 域策略 + 域统计)

```elixir
sqlite do table "sources"; repo ExAutoresearch.Repo end
# 无独立 multitenancy:经 report_id → organization_id 链隔离(同 Investigation)
attributes do
  uuid_v7_primary_key :id
  attribute :report_id,       :uuid_v7, allow_nil?: false
  attribute :investigation_id,:uuid_v7                       # 可空:来自哪次 investigation
  attribute :url,             :string,  allow_nil?: false
  attribute :domain,          :string                        # 去重 + 域统计 + 策略
  attribute :title,           :string
  attribute :fetched_at,      :utc_datetime_usec
  attribute :content_hash,    :string
  attribute :relevance_score, :float                         # 取代字节量分
  attribute :scraper_source,  :atom, constraints: [one_of: [:crawl4ai, :native, :unknown]], default: :unknown
  timestamps()
end
# identity: unique [:report_id, :url] —— 强制跨 query 去重
```

### 6.4 `Claim`(新)— 信任 + 审计 + delta 的最小单元(命门)

```elixir
sqlite do table "claims"; repo ExAutoresearch.Repo end
attributes do
  uuid_v7_primary_key :id
  attribute :report_id,  :uuid_v7, allow_nil?: false
  attribute :source_id,  :uuid_v7                             # 支撑此 claim 的源(可空=无据)
  attribute :text,       :string,  allow_nil?: false          # atomic 断言
  attribute :grounding,  :atom, constraints: [one_of: [:grounded, :contradicted, :unsupported, :complementary]], default: :unsupported
  attribute :confidence, :float                               # 0–1
  attribute :origin_subquery, :string                         # 哪个子查询找到它
  attribute :claim_hash, :string                              # delta 匹配键(归一化)
  attribute :order_index,:integer                             # 在报告中的位置
  timestamps()
end
# relationships: belongs_to :report; belongs_to :source
```

### 6.5 `Delta`(新)— 两版之间的"变了什么"简报

```elixir
sqlite do table "deltas"; repo ExAutoresearch.Repo end
multitenancy do strategy :attribute; attribute :organization_id end
attributes do
  uuid_v7_primary_key :id
  attribute :brief_id,        :uuid_v7, allow_nil?: false
  attribute :organization_id, :uuid_v7, allow_nil?: false
  attribute :from_report_id,  :uuid_v7                        # 可空:首次运行无 from
  attribute :to_report_id,    :uuid_v7, allow_nil?: false
  attribute :markdown_digest, :string                         # "变了什么 + 为什么重要"
  attribute :added_count,       :integer, default: 0
  attribute :changed_count,     :integer, default: 0
  attribute :removed_count,     :integer, default: 0
  attribute :contradicted_count,:integer, default: 0
  attribute :read_at,         :utc_datetime_usec              # 收件箱未读高亮
  attribute :generated_at,    :utc_datetime_usec
  timestamps()
end
```
> 可选(drill-down):`DeltaItem` 子表(change_kind + claim_id + prev_claim_id),MVP 不需要——明细可由两版 `Claim` 集合按 `claim_hash` 现算。

### 6.6 Domain 注册(`research.ex` 追加)
```elixir
resource ExAutoresearch.Research.Brief
resource ExAutoresearch.Research.Source
resource ExAutoresearch.Research.Claim
resource ExAutoresearch.Research.Delta
```

> ⚠️ **codegen 铁律(CLAUDE.md):** 任何 Ash 资源改动后必须 `mix ash.codegen <desc> --yes` 然后 `mix ash_sqlite.migrate`,否则运行时 `PendingCodegen` / `PendingMigrationError`。

---

## 7. Workers / Oban 设计

| Worker | 队列 | 触发 | 职责 |
|---|---|---|---|
| `ResearchWorker`(已存在,搬入 orchestrator 逻辑) | `:research` | `ash_oban` `Brief.run_due` trigger / 手动 | 跑完整 phase 机器,产出 versioned Report + Source + Claim |
| `DeltaWorker`(新) | `:default` | `ResearchWorker` 完成后入队 | diff 上一版 → 生成 `Delta` → 推送通知 |
| `NotifyWorker`(新,可后置) | `:default` | `DeltaWorker` 产出后 | 按 `Brief.notify_channels` 发 Slack/邮件/webhook |

- `Oban.Plugins.Cron crontab: []` 保持空——调度由 `ash_oban` trigger 动态驱动(比手搓 `TemplateScheduler` 更 idiomatic;**迁移后退役 `TemplateScheduler`**)。

---

## 8. MVP(证明品类的最小闭环)

**单租户,一条 Brief,跑通端到端:**
1. 定义 Brief(question + 周频 cadence)
2. `ash_oban` trigger 到期 → 入队 `ResearchWorker`
3. 信任层产出带核查引用的 Report(`Claim` + `Source` 落库,relevance 取代字节量)
4. `DeltaWorker` diff 上一版 → 生成 `Delta`
5. "变了什么"简报落进 LiveView 收件箱(未读高亮)

> 多租户规模化、源策略强制、审计导出、Slack/邮件、MCP 触发口——都是往上叠的层,**不阻塞 MVP**。

---

## 9. 路线(每段一个可演示里程碑)

| 阶段 | 交付 | 性质 | 验收 |
|---|---|---|---|
| **P0 止血** | 接孤儿质量信号 / URL 去重(`Source` identity) / 编号引用进 prompt / 解 2000 截断 | 几乎零成本 | 单报告引用不再幻觉;`mix precommit` 绿 |
| **P1 信任层** | `relevance_score` + Verifier-Critic + `Claim`/`Source` 资源 + `verifying` phase + Mission Control 展示真信号 | 命门 | 报告每条 claim 有 grounding+源+置信度;矛盾标红 |
| **P2 Delta 引擎** | `ResearchWorker` job-per-run + `Report.brief_id/run_version` + `ash_oban` trigger + `Delta` + `DeltaWorker` + 收件箱 | **护城河** | 同一 Brief 跑两次,收件箱出现正确 delta 简报 |
| **P3 企业解锁** | 多租户源策略强制 + claim 审计导出 + Slack/邮件/webhook + (可选) MCP 触发口 | 付费点 | 跨租户隔离;导出 CSV;Slack 收到简报 |

---

## 10. 验收 / 度量

**自动化:**
- 每个 Ash 资源改动后 `mix ash.codegen` + `mix ash_sqlite.migrate` 干净
- `mix precommit` 全绿(warnings-as-errors + format + test)
- 新增 LiveView/资源测试:Brief CRUD、Claim grounding 落库、Delta diff 正确性、租户隔离

**手动 golden path:**
1. 登录 → 建一条 Brief("track EU AI 监管动态",周频)
2. 手动触发首跑 → Mission Control 实时看 plan/子查询/逐源 relevance/verifying
3. 报告每条 claim 带 grounding 徽章 + 可点开源 + 置信度
4. 改一点条件再跑第二次 → 收件箱出现"变了什么"简报,added/changed/contradicted 计数正确
5. 导出该 Brief 的 claim 审计 CSV

**主观验收线:** 把 delta 简报发给一个非工程同事,问"你敢拿这个去汇报吗?为什么?"——若回答里有具体信任理由("每条都标了出处和置信度"、"矛盾的它给我标红了"),算过。

---

## 11. 开放决策(review 时定)

1. **产品名:** Sentinel?/ 常驻研究员 / 其它。
2. **`Brief` vs 扩展 `Template`:** 新建独立资源(本草案),还是给 `Template` 加 cadence/source-policy/notify 字段并升格?(影响现有 TemplateLive/ScheduleLive 改动量)
3. **relevance / claim-match 是否上 embedding:** MVP 纯 LLM judge + 文本 hash;还是早引入向量(`sqlite-vec`?)——加依赖 vs 精度。
4. **通知渠道优先级:** 先 LiveView 收件箱即可,还是 P2 就要 Slack?
5. **MCP 触发口:** 是否在 P3 暴露"运行 Brief"为 MCP tool(让外部 agent 触发)——注意**与 resman 的 MCP 严格分离**。
6. **定价层映射:** 自托管 OSS core + 企业版(多租户/审计/源策略)?还是?

---

## 12. 明确不做(拒绝再讨论)
- 追源数量(前沿:引用准确度 > 来源数量)
- 第二个 3D 场景 / VR
- 真 SSE 流式 LLM(已有 fake 流式够看)
- 可编辑协作 plan(留 v3)
- 与 `resman` 任何形式的交叉污染(CLAUDE.md 红线)
- 全自主黑盒(前沿:人在环 > 全自主 for trust;保留可打断/可steer)

---

## 附:现有资产复用 vs 新增 一览

| 复用(已在仓库) | 新增 |
|---|---|
| Oban + ResearchWorker + 队列 | `DeltaWorker` / `NotifyWorker` |
| ash_oban(已装未用)→ Brief trigger | `Brief` 资源 + trigger 配置 |
| Ash 多租户 / AshSqlite / domain | `Source` / `Claim` / `Delta` 资源 |
| Report(加 2 字段) | `verifying` phase + 信任层逻辑 |
| LiveView Mission Control + Narrative | 收件箱(Delta inbox)视图 |
| TemplateLive / ScheduleLive 骨架 | Brief 编辑/订阅视图(或升格 Template) |
| 现有 search/scrape 多后端 | 域策略过滤 + URL 去重 |
