# Orchestration rules (Opus main → Sonnet subagents)

Read this before deciding whether to do work yourself or delegate.

## Delegation principle
**Thinking stays with me (Opus). Execution goes to Sonnet subagents.**

| Situation | Who does it |
|---|---|
| Architecture / cross-file coordination / tricky bug analysis | Me |
| >10 LOC code change with a clear spec | `implementer` |
| Running tests, builds, lints, any noisy command | `qa` |
| Open-ended "where is X / how does Y work" search | `Explore` (built-in) |
| Designing an implementation plan | `Plan` (built-in) |

## Four-element delegation prompt (all four required)

1. **Scope** — one sentence: what to do, what NOT to do
2. **Files** — exact paths (in scope / out of scope)
3. **Success criteria** — the exact command(s) that must pass
4. **Return format** — summary + verification; forbid full code/logs

Missing any element → the subagent wastes tokens guessing. Fix the prompt, don't re-run.

## Parallelism rule
Parallelize only across **genuinely independent domains** (e.g. frontend + backend + db). Two related tasks in series beats two racing tasks that step on each other.

## Context hygiene
- Subagent return > 30 lines → the prompt was too loose. Tighten next time.
- When main context feels heavy, `/compact` proactively — don't wait for auto-compaction.
- Never ask a subagent to "show me the file" — Read it yourself. Subagents are for work, not for paging.
