# resman as an MCP server

**resman exposes its store to any agent harness that speaks [Model Context Protocol](https://modelcontextprotocol.io/).** The agent gets ten tools — `resman_best`, `resman_search`, `resman_near`, `resman_list_recent`, `resman_add_experiment`, `resman_diff_tags`, `resman_lineage`, `resman_find_by_signal`, `resman_distill`, `resman_verify` — without ever seeing resman's CLI in its context window.

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

| Tool | When the agent should call it |
|---|---|
| `resman_best` | Before starting a new experiment, to know the current baseline to beat. |
| `resman_search` | Before trying an idea, to check if it's been attempted. Avoids duplicate work. |
| `resman_near` | After getting a new result, to ground it ("what else landed near 0.985?"). |
| `resman_list_recent` | At session start, to remember what was tried last. |
| `resman_add_experiment` | After every training run — keep, discard, or crash. |
| `resman_diff_tags` *(v0.4)* | When branches diverge — "why did branch A beat B?" Returns a compact text summary. |
| `resman_lineage` *(v0.4)* | When planning a new experiment — walks the `parent_commit` graph so the agent knows which chains converged vs. dead-ended. |
| `resman_find_by_signal` *(v0.6)* | When triaging failures — "how many OOMs overnight?" Filters by typed crash signal (`oom`, `cuda_error`, `nan_loss`, `assert_fail`, `timeout`, `unknown`). |
| `resman_distill` *(v0.6)* | End of session — "what did we learn last night?" Returns a structured Markdown (or JSON) summary: best, lineage, failure clusters, unexplored neighbors, heuristic suggestions. The preferred long-term-memory artifact. |
| `resman_verify` *(v0.7)* | After re-running an experiment — pass `{commit, value, tolerance?}` to promote it to `status=verified` if the new measurement is within tolerance of the original (directional by metric direction). |

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

## Design notes

- **Transport is line-delimited JSON**, not the full LSP-style `Content-Length` framing. Claude Code / Cursor / the reference Python SDK all accept either; line-delimited is simpler and sufficient for stdio.
- **Notifications are silently accepted.** Anything without an `id` field gets no reply, per spec.
- **Tool errors are `isError: true` inside a successful JSON-RPC response**, not JSON-RPC `error` objects. This matches MCP conventions — transport-level failures are JSON-RPC errors; tool-level failures are content errors the LLM can read and retry.
- **The protocol version** is `2024-11-05` (the stable MCP version as of this writing). Bumping it is a deliberate decision, not automatic.
