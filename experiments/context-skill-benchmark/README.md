# Context Skill Benchmark

Compares two ways of answering questions about a clayers spec:

- **baseline** — `claude -p` in a temp dir containing only the spec's
  XML files, with tools restricted to `Read`, `Glob`, `Grep`. The agent
  must scan the walls-of-XML itself.
- **skill** — `claude -p` in a temp dir where `clayers adopt --skills`
  has planted `clayers-context`, invoked via `/clayers-context <question>`.
  The skill orchestrates semantic search + subagent fan-out per the
  four-phase playbook in `agent-guidance.xml`.

For each question, both variants run and a third `claude -p` call
(no tools, A/B order randomized) judges which answer is more accurate
and complete.

## Metrics captured

Per run:

- **wall time** (seconds)
- **cost** (USD via `total_cost_usd`)
- **output tokens** (model generation volume)
- **turns** (main-agent API calls)
- **main-agent context size per turn** — computed from the
  `usage.iterations[]` array: each iteration is one API call to the
  main agent, and we track `input_tokens + cache_read_input_tokens
  + cache_creation_input_tokens` (all three = what the model saw on
  that turn). Subagent-internal tool use does NOT inflate these; it's
  encapsulated inside the `Agent` tool call from the main agent's
  perspective. This is the key metric that distinguishes "agent reads
  XML directly" from "agent fans out to subagents":
  - `peak_turn_context_tokens` — watermark across all turns
  - `final_turn_context_tokens` — steady state at the end of the run
  - `turn_context_samples` — full per-turn trace (for plots)
- **error status**

Per question: judge verdict (`skill` / `baseline` / `tie` / `?`).

Aggregate: totals for cost/time, **averages and max-across-questions
for context-size watermarks**, judge win counts.

## Requirements

- `claude` CLI in PATH
- `clayers` CLI in PATH (so the skill's bash snippets resolve)

## Run

```bash
uv run --script run.py \
    --spec ../../clayers/clayers \
    --clayers-repo ../.. \
    --questions questions.yaml \
    --output results.json
```

Optional flags:

- `--model sonnet|opus|haiku|<id>` — which model to use for the
  benchmark runs (default: the model `claude` selects).
- `--judge-model <id>` — override just the judge's model
  (defaults to `--model`).
- `--max-questions N` — run only the first N questions (quick sanity).
- `--only baseline|skill|judge` — run a subset of phases
  (repeatable).
- `--seed N` — RNG seed for A/B-order randomization (default 42).
- `--verbose` — print the full `claude` command line for each run.

## Output

`results.json` contains:

- `summary`: aggregates
- `results`: per-question metrics + answers + verdict
- `args`: the invocation args, for reproducibility

The file is written incrementally as the benchmark runs, so a
mid-run crash preserves completed questions.

## Interpreting the verdict

- `skill` / `baseline` — judge picked that variant's answer.
- `tie` — judge explicitly said both are equally good.
- `?` — judge's reply didn't parse cleanly. Usually means the
  judge hedged in prose despite the "one token" instruction.
- `n/a` — judging was skipped (error in one variant or
  `--only` excluded judge).
