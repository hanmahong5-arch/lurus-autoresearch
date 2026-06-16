# Agent quickstart — five minutes from cold start to useful memory

You are an LLM agent that just opened this repository. **resman** is the
local-first experiment tracker you will use as long-term memory across
training sessions. This doc tells you exactly what to call, when, and what
to do if a call fails. It is written for *you*, not for a human reviewing
your work later.

## 0. Confirm the wiring works (one call)

Before you trust anything in this layer, call `resman_doctor`:

```
resman_doctor {}
```

Read the returned JSON's `summary` field. Branch on the result:

- `summary.fail == 0` → continue.
- `summary.fail >= 1` → **stop and read the `checks` array**. Each failing
  check has a `hint` field describing how to fix it. If you cannot fix it
  yourself (e.g., a missing `.mcp.json`), surface the hint to the user and
  ask for help. Do not proceed to call other tools until doctor is clean.

`status` values: `ok` (works), `warn` (works but suboptimal), `fresh`
(expected for a new store — keep going), `fail` (must fix).

## 1. Discover prior state (session start)

```
resman_list_recent { n: 20 }
```

This is your **structured fresh-store probe**. Parse the returned JSON:

- `total === 0` → this is the first session. Tell the user "fresh resman
  store — this run will establish baselines." Skip the rest of section 1.
- `total > 0` → identify the most-recent tag from `tags[0]` (the `tags`
  array is ordered by most recent experiment first). Then call:

```
resman_distill { tag: <tags[0]> }
```

Read **every section** of the distill output: best, lineage, signal
clusters, unexplored neighbors, suggestions. These are your starting
heuristics — do not skip them.

If you might fire experiments that historically OOM, also call:

```
resman_find_by_signal { signal_type: "oom" }
```

to know which directions to avoid wasting a 5-minute slot on.

Summarize what you learned to the user in 2-3 bullets before coding.

## 2. The seventeen MCP tools, by lifecycle moment

| Moment | Tool | Why |
|---|---|---|
| Session start (always) | `resman_doctor` | Confirm wiring. One call replaces a dozen probes. |
| Session start (optional) | `resman_tags` | Per-tag snapshot: one row per tag (count, best, last_update). Easier than scanning list_recent for tag boundaries. |
| Session start (always) | `resman_list_recent` | Fresh-store probe (`total === 0`) + most-recent-tag lookup. |
| Session start (if not fresh) | `resman_distill` | Long-term memory artifact. Read every section. |
| Session start (if you'll run risky configs) | `resman_find_by_signal` | "Has this OOM'd before?" / "what NaN's the loss?" |
| Before trying an idea | `resman_search` | "Has this idea been tried?" Avoid duplicate work. |
| Before starting an experiment | `resman_best` | Current baseline to beat. Add `composite: true` for the multi-dim "resume-from-here" score. |
| Branching from non-HEAD | `resman_lineage` | Which chains converged vs dead-ended. |
| After every training run | `resman_add_experiment` | Atomic write. Include `parent_commit` and `log_tail`. Watch the response for `"lineage chain broken"` warning. |
| After a reproduction run near a prior baseline | `resman_verify` | Promote to verified if within tolerance (default 0.01, absolute, direction-sensitive). |
| When later evidence disagrees with a verified result | `resman_unverify` | Retract the trust label back to `keep`. val_bpb retained. |
| Comparing two branches | `resman_diff_tags` | Why did branch A beat branch B? |
| Triaging mid-session | `resman_list` | Filtered, sorted view (by `status`, `signal`, `grep`, `top`) — deeper than `resman_list_recent`. |
| Comparing many runs at once | `resman_compare` | Best-of-each across runs, side by side. |
| Sanity-checking progress | `resman_stats` | Counts + val_bpb best/worst/mean/stddev for one tag or all runs. |
| Auditing your own behavior | `resman_usage` | Telemetry totals, per-tag adoption funnel, cold (never-called) tools. |
| End of session (or every ~10 runs) | `resman_distill` | Refresh your mental model. This is also what the *next* session will read. |

`resman_near` rounds it out: after a new result, "what else landed near
this val_bpb?" — useful for grounding.

**All seventeen tools return structured JSON.** Parse, don't substring-match.

## 3. Per-run write contract (`resman_add_experiment`)

After every training run — keep, discard, or crash — call:

```
resman_add_experiment {
  tag:           "<your run tag, e.g. apr17-overnight>",
  commit:        "<short git sha>",
  val_bpb:       <float; 0 for crashes>,
  memory_gb:     <peak_vram_mb / 1024, .1f; 0 for crashes>,
  status:        "keep" | "discard" | "crash" | "best",
  description:   "<one-line idea summary>",
  parent_commit: "<commit you advanced/reset from>",
  log_tail:      "<last ~50 lines of run.log>"
}
```

Two response patterns to watch for:

1. `"recorded: [<tag>] <commit> val_bpb=..."` → success.
2. Same message followed by `"warning: ... lineage chain broken at this commit"` →
   you forgot `parent_commit`. **Always pass `parent_commit` starting at the
   second experiment of any tag**, or future distill cannot reconstruct
   lineage. Note the parent SHA *before* you commit so you have it on
   hand when calling this tool.

`log_tail` is the killer field: resman regex-classifies crashes into
typed signals (`oom`, `cuda_error`, `nan_loss`, `assert_fail`, `timeout`,
`diverged_loss`, `slow_mfu`). Pass it even on success runs so future
`resman_find_by_signal` queries see the full picture.

## 4. Verify ↔ unverify

`resman_verify` is **directional, absolute, default tolerance 0.01**.
For `val_bpb` (minimize): a new run passes if `new <= original + 0.01`.
Lower-is-better; "good enough" only bounded on the *worse* side.

If later evidence shows the verified value was a fluke, retract:

```
resman_unverify { commit: <previously-verified commit> }
```

The val_bpb is retained; only the trust label moves back to `keep`.
Never let a fluke stay verified.

## 5. When the MCP server isn't connected

If `tools/list` doesn't show `resman_*`, drop to the CLI:

```
resman doctor                                                # health probe
resman list --top 20 -o json                                 # discover
resman distill -t <tag>                                      # memory artifact
resman add -t <tag> -c <commit> -v <bpb> -s <status> -d "<desc>" \
           --parent <parent-commit> --log run.log
resman verify <commit> -v <new_value>                        # promote
resman unverify <commit>                                     # retract
```

The CLI is the human-facing fallback. Use MCP whenever possible — JSON in,
JSON out, zero substring matching.

**CLI gotcha**: `resman list` defaults to `keep`-status only. To see OOMs
and crash history, pass `--status all` or `--status crash`. MCP path
(`resman_find_by_signal`, `resman_distill`) does not have this footgun.

## 6. FAQ

**Q: Doctor says `mcp_wiring: warn — no .mcp.json found`. Should I stop?**
A: No. That check warns when there's no `.mcp.json` *in the cwd or its
parents*. If you're already using these tools via MCP, the wiring works
— the warning just means the file lives elsewhere on the host. Note it
to the user once, then continue.

**Q: I see two experiments with the same val_bpb. Should I worry?**
A: Run `resman_near { val_bpb: <that value>, n: 5 }`. If multiple
descriptions converge to similar setups, your search space has a local
attractor. Distill suggestion 6 (duplicate descriptions) catches the
worst form of this.

**Q: When do I tag a result `best` vs `keep`?**
A: Use `keep` for "this advanced the bpb"; `best` is reserved by
convention for the human-or-agent-declared champion of a run. The
composite scorer treats Keep/Best similarly but Best earns +0.2 in the
`verified` subscore.

**Q: How do I "remember" what to try next session?**
A: You don't. `resman_distill` does. Make sure every experiment is
logged (section 3) and every reproduction is verified (section 4) — then
next session's `resman_distill` reads as a coherent narrative including
your unexplored neighbors and stagnation warnings.

## 7. The contract in one sentence

**Log to resman first, TSV second, git commit third. Reset only after
resman has the failure recorded — once `git reset --hard HEAD~1` runs,
resman is the only memory of what was tried.**
