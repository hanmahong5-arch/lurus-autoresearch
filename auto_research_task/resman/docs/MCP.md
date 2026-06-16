# resman as an MCP server

**resman exposes its store to any agent harness that speaks [Model Context Protocol](https://modelcontextprotocol.io/).** The agent gets thirteen tools — `resman_best`, `resman_search`, `resman_near`, `resman_list_recent`, `resman_add_experiment`, `resman_diff_tags`, `resman_lineage`, `resman_find_by_signal`, `resman_distill`, `resman_verify`, `resman_unverify`, `resman_doctor`, `resman_tags` — without ever seeing resman's CLI in its context window.

## Why this matters

Without MCP, an agent that wants to check "has this idea been tried?" has to:
1. Remember to run `bash -c "resman search 'GeLU'"`
2. Parse the stdout
3. Decide

With MCP, the agent just calls `resman_search({pattern: "GeLU"})` as a structured tool. The harness handles the plumbing. Result: fewer tokens, fewer bash-escaping accidents, and the tool is discoverable via `tools/list` — the agent knows it exists.

This is the primary integration surface going forward. Every new resman feature should expose a matching MCP tool.

## Wiring it up

### Claude Code

Add to `.claude/mcp.json` (or the global equivalent):

```json
{
  "mcpServers": {
    "resman": {
      "command": "resman",
      "args": ["mcp"],
      "env": {
        "RESMAN_HOME": "/abs/path/to/.resman"
      }
    }
  }
}
```

Restart Claude Code; tools appear as `mcp__resman__resman_best`, etc.

### Cursor

`~/.cursor/mcp.json`:

```json
{
  "mcpServers": {
    "resman": { "command": "resman", "args": ["mcp"] }
  }
}
```

### Any harness

The protocol is JSON-RPC 2.0, one message per line, on stdio. Launch:

```bash
resman mcp
```

Expected client messages: `initialize` → `notifications/initialized` (no reply) → `tools/list` → `tools/call` (repeat).

## Tool surface

| Tool | When the agent should call it | Output |
|---|---|---|
| `resman_best` | Before starting a new experiment, to know the current baseline to beat. | (JSON) |
| `resman_search` | Before trying an idea, to check if it's been attempted. Avoids duplicate work. | (JSON) |
| `resman_near` | After getting a new result, to ground it ("what else landed near 0.985?"). | (JSON) |
| `resman_list_recent` | At session start, to remember what was tried last. | (JSON) |
| `resman_add_experiment` | After every training run — keep, discard, or crash. | (JSON) |
| `resman_diff_tags` *(v0.4)* | When branches diverge — "why did branch A beat B?" | (JSON) |
| `resman_lineage` *(v0.4)* | When planning a new experiment — walks the `parent_commit` graph so the agent knows which chains converged vs. dead-ended. | (JSON) |
| `resman_find_by_signal` *(v0.6)* | When triaging failures — "how many OOMs overnight?" Filters by typed crash signal (`oom`, `cuda_error`, `nan_loss`, `assert_fail`, `timeout`, `unknown`). | (JSON) |
| `resman_distill` *(v0.6)* | End of session — "what did we learn last night?" Returns a structured Markdown (or JSON) summary: best, lineage, failure clusters, unexplored neighbors, heuristic suggestions. The preferred long-term-memory artifact. | (JSON via format=json) |
| `resman_verify` *(v0.7)* | After re-running an experiment — pass `{commit, value, tolerance?}` to promote it to `status=verified` if the new measurement is within tolerance of the original (directional by metric direction). | (JSON) |
| `resman_unverify` | When later evidence disagrees with a verified result — retracts the trust label back to `keep`; val_bpb is retained. Symmetric counterpart to `resman_verify`. | (JSON) |
| `resman_doctor` | Session start health probe — runs six checks (data dir, env, runs present, usage telemetry, MCP wiring, store invariants). Read `summary.fail`; fix any failing check's `hint` before proceeding. | (JSON) |
| `resman_tags` | Per-tag snapshot: one row per tag (count, best, metric, last_update). Use for "what tags do I have?" — complements `resman_list_recent` which is per-experiment. | (JSON) |

`resman_best` also accepts `composite: true` *(v0.7)* to rank by a multi-dim score (metric + verification + lineage + description) rather than raw metric. Preferred when the agent asks "which experiment should I resume from?".

The `instructions` field in the `initialize` response tells the LLM exactly this, so well-behaved agents call the right tools at the right times without bespoke prompt engineering.

## Sanity-check the server

Pipe a hand-rolled handshake:

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' \
  '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"resman_best","arguments":{}}}' \
  | resman mcp | jq .
```

You should see three JSON-RPC responses.

## Usage telemetry (`usage.jsonl`)

Every `tools/call` the server handles appends one JSONL event to `<data_dir>/usage.jsonl`. This is the source of truth for "which agents call which tools, with what args, with what success rate" — the dataset used to tune composite weights and distill templates before v1.0 schema freeze.

**Schema** (one line per call):

```json
{"ts":"2026-05-16T15:30:00.123Z","tool":"resman_distill","args":{"tag":"may16"},"ok":true,"duration_ms":12,"result_chars":482}
```

| Field | Type | Notes |
|---|---|---|
| `ts` | ISO 8601 UTC, ms precision | When the call returned |
| `tool` | string | e.g. `resman_best`, `resman_add_experiment` |
| `args` | object | Full `arguments` payload (small) — useful for "which params are agents passing?" |
| `ok` | bool | `false` iff the tool returned `isError: true` |
| `duration_ms` | int | Wall-clock dispatch time |
| `result_chars` | int | Length of the response text (proxy for payload size) |

Quick analysis with `jq` (10 recipes):

R1 — p50/p95 latency per tool, useful as a regression alarm after deploys:

```bash
# R1. p50/p95 latency per tool — regression alarm
jq -s 'group_by(.tool)|map({tool:.[0].tool, n:length,
  p50:(map(.duration_ms)|sort|.[length/2|floor]),
  p95:(map(.duration_ms)|sort|.[(length*0.95)|floor])})' usage.jsonl
```

R2 — error rate per tool, surface which tools are flaky:

```bash
# R2. error rate per tool
jq -s 'group_by(.tool)|map({tool:.[0].tool, n:length,
  errs:(map(select(.ok==false))|length)})|map(.+{rate:(.errs/.n)})' usage.jsonl
```

R3 — distill adoption: fraction of per-tag sessions that ended with a distill call:

```bash
# R3. distill adoption — fraction of sessions that ended with distill
jq -s 'group_by(.args.tag//"_")|map({tag:.[0].args.tag,
  added:(map(select(.tool=="resman_add_experiment"))|length),
  distilled:(map(select(.tool=="resman_distill"))|length)})' usage.jsonl
```

R4 — verify-after-keep ratio: how often agents follow up Keep experiments with a verify call:

```bash
# R4. verify-after-keep ratio
jq -s '{verifies:(map(select(.tool=="resman_verify"))|length),
        kept_adds:(map(select(.tool=="resman_add_experiment" and .args.status=="keep"))|length)}
       |.+{ratio:(.verifies/(.kept_adds|if .==0 then 1 else . end))}' usage.jsonl
```

R5 — repeated `search` patterns: detect agents re-querying the same pattern (forgetting prior negative results):

```bash
# R5. repeated `search` patterns — agent forgetting prior negative results
jq -r 'select(.tool=="resman_search")|.args.pattern' usage.jsonl | sort | uniq -c | sort -rn
```

R6 — composite-best dissent: count how often `best{composite=true}` is immediately followed by a search/near (agent disagreeing with composite ranking):

```bash
# R6. `best{composite=true}` dissent — composite-best immediately followed by search/near
jq -s '[range(0;length-1) as $i | {a:.[$i], b:.[$i+1]}
  | select(.a.tool=="resman_best" and (.a.args.composite==true)
           and (.b.tool=="resman_search" or .b.tool=="resman_near"))] | length' usage.jsonl
```

R7 — top 8 tool transition pairs, the full Markov transition matrix condensed:

```bash
# R7. tool transition matrix — top 8 pairs
jq -s '[range(0;length-1) as $i | "\(.[$i].tool)→\(.[$i+1].tool)"] | reduce .[] as $p ({}; .[$p]+=1)
       | to_entries | sort_by(-.value) | .[0:8]' usage.jsonl
```

R8 — time from first add to first distill per tag (seconds), measures how quickly agents close the loop:

```bash
# R8. time-from-add to first distill per tag (seconds)
jq -s 'group_by(.args.tag//"_")|map({tag:.[0].args.tag,
  first_add:(map(select(.tool=="resman_add_experiment"))|.[0].ts),
  first_distill:(map(select(.tool=="resman_distill"))|.[0].ts)})
  |map(select(.first_add and .first_distill))' usage.jsonl
```

R9 — failing-call bigrams: which prior tool call most often precedes a failure:

```bash
# R9. failing-call bigrams — which prior calls predict failure
jq -s '[range(1;length) as $i | select(.[$i].ok==false) | "\(.[$i-1].tool)→\(.[$i].tool)"]
       | reduce .[] as $p ({}; .[$p]+=1)' usage.jsonl
```

R10 — tool co-occurrence within 5 s: discover which tools agents invoke in rapid succession:

```bash
# R10. tool co-occurrence within 5s
jq -s '[range(0;length-1) as $i | {a:.[$i].tool, b:.[$i+1].tool,
  dt:((.[$i+1].ts|fromdateiso8601)-(.[$i].ts|fromdateiso8601))}
  | select(.dt<5 and .a!=.b) | "\(.a)+\(.b)"] | reduce .[] as $p ({}; .[$p]+=1)' usage.jsonl
```

> 若想避免维护这些 jq, 用 `resman usage` 子命令 (built-in)。

**Opt out** with `RESMAN_DISABLE_USAGE_LOG=1`. Failures to write are logged once to stderr and silently swallowed — telemetry must never break a tool call.

**Not transmitted anywhere.** Local file only. Inspect it, delete it, ship it to your own analytics — your call.

## Design notes

- **Transport is line-delimited JSON**, not the full LSP-style `Content-Length` framing. Claude Code / Cursor / the reference Python SDK all accept either; line-delimited is simpler and sufficient for stdio.
- **Notifications are silently accepted.** Anything without an `id` field gets no reply, per spec.
- **Tool errors are `isError: true` inside a successful JSON-RPC response**, not JSON-RPC `error` objects. This matches MCP conventions — transport-level failures are JSON-RPC errors; tool-level failures are content errors the LLM can read and retry.
- **The protocol version** is `2024-11-05` (the stable MCP version as of this writing). Bumping it is a deliberate decision, not automatic.
