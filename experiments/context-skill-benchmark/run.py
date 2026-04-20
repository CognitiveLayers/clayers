#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyyaml"]
# ///
"""Benchmark the clayers-context skill against a raw-XML scan baseline.

For each question in the input set, runs two variants of `claude -p`
and a judge:

  baseline  — a temp dir with only the spec's *.xml files, tools
              restricted to Read/Glob/Grep. Agent has to grep the
              walls-of-XML to form its answer.
  skill     — a temp dir with `clayers adopt --skills` run on it,
              invoked via `/clayers-context <question>`. Agent uses
              semantic search + subagent fan-out per the skill body.
  judge     — a third `claude -p` call, no tools, compares the two
              answers blindly (A/B order randomized).

Metrics collected per variant: wall time, tokens in/out, cache read,
cost (USD), turn count. Per question: judge verdict. Summary:
aggregate time/cost/wins.

Usage:

  uv run --script run.py \
      --spec ../../clayers/clayers \
      --clayers-repo ../.. \
      --questions questions.yaml \
      --output results.json \
      [--model sonnet] [--judge-model sonnet] [--max-questions N]

Requires: `claude` and `clayers` CLIs in PATH.
"""
from __future__ import annotations

import argparse
import json
import os
import random
import shutil
import subprocess
import sys
import tempfile
import time
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any

import yaml


# ---------------------------------------------------------------------------
# `claude -p` invocation
# ---------------------------------------------------------------------------
@dataclass
class RunMetrics:
    answer: str
    elapsed_s: float
    total_cost_usd: float
    input_tokens: int
    output_tokens: int
    cache_read_input_tokens: int
    cache_creation_input_tokens: int
    num_turns: int
    is_error: bool
    session_id: str
    # Main-agent per-turn context size. Each API call to the main agent
    # sees `input_tokens + cache_read + cache_creation` of prior
    # conversation. Track the peak (watermark) and the final turn (steady
    # state after all work is done). Subagent-internal tool use does not
    # inflate these — it's encapsulated inside the Agent tool call.
    peak_turn_context_tokens: int
    final_turn_context_tokens: int
    turn_context_samples: list[int]  # per-iteration context size, for plots/debug


def _ctx_from_usage(u: dict[str, Any]) -> int:
    """Per-turn main-agent context size from a usage block.

    What the model saw on this API call = input_tokens (new uncached)
    + cache_read_input_tokens (retrieved from existing cache entries)
    + cache_creation_input_tokens (new tokens being added to cache,
    which the model also sees). All three are additive.
    """
    return (
        int(u.get("input_tokens", 0) or 0)
        + int(u.get("cache_read_input_tokens", 0) or 0)
        + int(u.get("cache_creation_input_tokens", 0) or 0)
    )


def _err(answer: str, elapsed: float) -> RunMetrics:
    return RunMetrics(
        answer=answer, elapsed_s=elapsed,
        total_cost_usd=0.0, input_tokens=0, output_tokens=0,
        cache_read_input_tokens=0, cache_creation_input_tokens=0,
        num_turns=0, is_error=True, session_id="",
        peak_turn_context_tokens=0, final_turn_context_tokens=0,
        turn_context_samples=[],
    )


def run_claude(
    prompt: str,
    cwd: Path,
    allowed_tools: str | None = None,
    disallowed_tools: str | None = None,
    model: str | None = None,
    timeout_s: int = 900,
    verbose: bool = False,
) -> RunMetrics:
    """Invoke `claude -p --output-format stream-json` and extract per-turn metrics.

    Streaming output is required to see per-turn usage. The non-streaming
    `json` format collapses the `iterations[]` array into a single entry
    that hides per-turn growth — so we can't track the context watermark.
    """
    cmd: list[str] = [
        "claude",
        "-p", prompt,
        "--output-format", "stream-json",
        "--verbose",  # required alongside stream-json
        "--dangerously-skip-permissions",
        "--no-session-persistence",
        "--exclude-dynamic-system-prompt-sections",
    ]
    if allowed_tools is not None:
        cmd.extend(["--allowed-tools", allowed_tools])
    if disallowed_tools is not None:
        cmd.extend(["--disallowed-tools", disallowed_tools])
    if model:
        cmd.extend(["--model", model])

    if verbose:
        print(f"    $ (cwd={cwd}) claude -p {prompt!r} "
              f"allowed={allowed_tools!r} model={model!r}", file=sys.stderr)

    t0 = time.time()
    # Per-turn context, deduplicated by assistant message.id. A single
    # turn emits multiple `assistant` events (one per content block:
    # thinking, text, tool_use) all sharing the same message id. Taking
    # each unique id's usage once gives us one sample per API call,
    # matching `num_turns` in the final result.
    turn_contexts: list[int] = []
    seen_message_ids: set[str] = set()
    final_result: dict[str, Any] | None = None

    try:
        proc = subprocess.Popen(
            cmd, cwd=str(cwd),
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
        )
    except FileNotFoundError:
        return _err("(claude CLI not found in PATH)", 0.0)

    assert proc.stdout is not None
    try:
        for raw_line in proc.stdout:
            line = raw_line.strip()
            if not line:
                continue
            try:
                evt = json.loads(line)
            except json.JSONDecodeError:
                continue
            etype = evt.get("type")
            if etype == "assistant":
                # Skip subagent-internal messages explicitly. Claude Code's
                # `-p` stream does not surface them today, but we filter on
                # parent_tool_use_id anyway so this stays correct if that
                # behavior changes: main-agent messages have parent=null,
                # subagent messages have parent=<tool_use_id>.
                if evt.get("parent_tool_use_id") is not None:
                    continue
                msg = evt.get("message") or {}
                mid = msg.get("id")
                if mid and mid not in seen_message_ids:
                    seen_message_ids.add(mid)
                    ctx = _ctx_from_usage(msg.get("usage") or {})
                    turn_contexts.append(ctx)
            elif etype == "result":
                final_result = evt
    except Exception as e:  # noqa: BLE001 — best-effort stream parsing
        proc.kill()
        proc.wait(timeout=5)
        elapsed = time.time() - t0
        return _err(f"(STREAM PARSE ERROR) {e}", elapsed)

    try:
        proc.wait(timeout=timeout_s)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait(timeout=5)
        elapsed = time.time() - t0
        return _err("(TIMEOUT)", elapsed)

    elapsed = time.time() - t0
    stderr_text = proc.stderr.read() if proc.stderr else ""

    if proc.returncode != 0:
        return _err(
            f"(EXIT {proc.returncode}) {stderr_text[-500:]}", elapsed,
        )
    if final_result is None:
        return _err(
            f"(no `result` event in stream; stderr[-500:]={stderr_text[-500:]})",
            elapsed,
        )

    usage = final_result.get("usage", {}) or {}
    peak_ctx = max(turn_contexts) if turn_contexts else 0
    final_ctx = turn_contexts[-1] if turn_contexts else 0

    return RunMetrics(
        answer=final_result.get("result", ""),
        elapsed_s=elapsed,
        total_cost_usd=float(final_result.get("total_cost_usd", 0.0) or 0.0),
        input_tokens=int(usage.get("input_tokens", 0) or 0),
        output_tokens=int(usage.get("output_tokens", 0) or 0),
        cache_read_input_tokens=int(usage.get("cache_read_input_tokens", 0) or 0),
        cache_creation_input_tokens=int(usage.get("cache_creation_input_tokens", 0) or 0),
        num_turns=int(final_result.get("num_turns", 0) or 0),
        is_error=bool(final_result.get("is_error", False)),
        session_id=final_result.get("session_id", ""),
        peak_turn_context_tokens=peak_ctx,
        final_turn_context_tokens=final_ctx,
        turn_context_samples=turn_contexts,
    )


# ---------------------------------------------------------------------------
# Directory setup
# ---------------------------------------------------------------------------
def setup_baseline_dir(spec_source: Path, target: Path) -> None:
    """Copy only the .xml files. No .claude/, no clayers CLI hooks."""
    target.mkdir(parents=True, exist_ok=True)
    for xml in sorted(spec_source.glob("*.xml")):
        shutil.copy2(xml, target / xml.name)


def setup_skill_dir(clayers_repo: Path, spec_source: Path, target: Path) -> None:
    """Plant the clayers-context skill into a clean adoption.

    Also copy the spec XMLs into `clayers/<project>/` so the skill's
    `clayers/{{PROJECT_NAME}}/` references resolve. The project name
    is whatever `adopt` picks (the target directory's basename).
    """
    target.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        ["clayers", "adopt", "--skills", str(target)],
        cwd=str(clayers_repo), check=True, capture_output=True, text=True,
    )
    # The skill's bash snippets reference clayers/<project-name>/ as the
    # spec path. Copy the XMLs to that location so the skill finds them.
    project = target.name
    spec_target = target / "clayers" / project
    spec_target.mkdir(parents=True, exist_ok=True)
    for xml in sorted(spec_source.glob("*.xml")):
        shutil.copy2(xml, spec_target / xml.name)


# ---------------------------------------------------------------------------
# Judge
# ---------------------------------------------------------------------------
JUDGE_PROMPT_TEMPLATE = """You are an impartial judge comparing two answers
to a technical question about a specification. Judge factual accuracy and
completeness only. Ignore formatting differences.

Question:
{question}

=== Answer A ===
{answer_a}

=== Answer B ===
{answer_b}

Which answer is more accurate and complete? Reply with ONLY one token:
"A", "B", or "tie". No explanation, no punctuation."""


def judge_answers(
    question: str, answer_a: str, answer_b: str, model: str | None, verbose: bool
) -> str:
    """Return 'A', 'B', 'tie', or '?' (parse failure)."""
    prompt = JUDGE_PROMPT_TEMPLATE.format(
        question=question, answer_a=answer_a, answer_b=answer_b,
    )
    # Judge runs in cwd with no tools; the comparison is pure text.
    with tempfile.TemporaryDirectory() as tmp:
        metrics = run_claude(
            prompt, cwd=Path(tmp),
            # Deny every tool the judge might be tempted to use.
            disallowed_tools="Read Glob Grep Bash Edit Write Skill Agent WebSearch WebFetch",
            model=model, timeout_s=120, verbose=verbose,
        )
    verdict = metrics.answer.strip().lower()
    if verdict.startswith("a"):
        return "A"
    if verdict.startswith("b"):
        return "B"
    if "tie" in verdict:
        return "tie"
    return "?"


# ---------------------------------------------------------------------------
# Main benchmark loop
# ---------------------------------------------------------------------------
def load_questions(path: Path) -> list[dict[str, Any]]:
    text = path.read_text()
    if path.suffix in (".yaml", ".yml"):
        data = yaml.safe_load(text)
    else:
        data = json.loads(text)
    if isinstance(data, dict):
        data = data.get("questions", [])
    if not isinstance(data, list):
        raise ValueError(f"{path}: expected a list of questions")
    out: list[dict[str, Any]] = []
    for i, q in enumerate(data):
        if isinstance(q, str):
            out.append({"question": q})
        elif isinstance(q, dict) and "question" in q:
            out.append(q)
        else:
            raise ValueError(f"question {i}: must be string or {{question: ...}} dict")
    return out


def check_tool(name: str) -> None:
    if shutil.which(name) is None:
        print(f"error: `{name}` CLI not found in PATH", file=sys.stderr)
        sys.exit(2)


BASELINE_PROMPT = (
    "Answer the following question about the specifications stored as XML "
    "files in the current working directory. Read the relevant files first, "
    "then give a concise, technically accurate answer. Cite specific IDs, "
    "file paths, and line ranges where applicable.\n\n"
    "Question: {question}"
)


SKILL_PROMPT = "/clayers-context {question}"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--spec", type=Path, required=True,
                        help="Directory containing the spec's .xml files.")
    parser.add_argument("--clayers-repo", type=Path, required=True,
                        help="Path to the clayers repo for `adopt --skills`.")
    parser.add_argument("--questions", type=Path, required=True,
                        help="YAML/JSON file with list of {question: str}.")
    parser.add_argument("--output", type=Path, default=Path("results.json"),
                        help="Where to write the results JSON.")
    parser.add_argument("--model", default=None,
                        help="Model alias for benchmark runs (sonnet/opus/haiku).")
    parser.add_argument("--judge-model", default=None,
                        help="Model for the judge (defaults to --model).")
    parser.add_argument("--max-questions", type=int, default=None,
                        help="Limit to first N questions (for quick sanity runs).")
    parser.add_argument("--only", choices=["baseline", "skill", "judge"],
                        action="append", default=None,
                        help="Run only a subset (repeatable).")
    parser.add_argument("--seed", type=int, default=42,
                        help="RNG seed for A/B-order randomization.")
    parser.add_argument("--verbose", action="store_true")
    args = parser.parse_args()

    check_tool("claude")
    check_tool("clayers")

    args.spec = args.spec.resolve()
    args.clayers_repo = args.clayers_repo.resolve()
    args.questions = args.questions.resolve()
    args.output = args.output.resolve()

    if not args.spec.is_dir():
        print(f"error: --spec {args.spec} is not a directory", file=sys.stderr)
        return 2

    questions = load_questions(args.questions)
    if args.max_questions:
        questions = questions[: args.max_questions]

    only = set(args.only) if args.only else {"baseline", "skill", "judge"}
    rng = random.Random(args.seed)

    with tempfile.TemporaryDirectory(prefix="clayers-benchmark-") as tmp:
        tmp = Path(tmp)
        baseline_dir = tmp / "baseline"
        skill_dir = tmp / "skill"

        if "baseline" in only:
            print(f"[setup] baseline dir: {baseline_dir}")
            setup_baseline_dir(args.spec, baseline_dir)
        if "skill" in only:
            print(f"[setup] skill dir: {skill_dir}")
            setup_skill_dir(args.clayers_repo, args.spec, skill_dir)

        results = []
        for i, q in enumerate(questions, 1):
            qtext = q["question"]
            print(f"\n[{i}/{len(questions)}] {qtext}")

            baseline = skill = None
            if "baseline" in only:
                print("  baseline: running…")
                baseline = run_claude(
                    BASELINE_PROMPT.format(question=qtext),
                    cwd=baseline_dir,
                    allowed_tools="Read Glob Grep",
                    model=args.model, verbose=args.verbose,
                )
                print(f"  baseline: {baseline.elapsed_s:.1f}s  "
                      f"${baseline.total_cost_usd:.4f}  "
                      f"out={baseline.output_tokens}  "
                      f"peak_ctx={baseline.peak_turn_context_tokens:,}  "
                      f"final_ctx={baseline.final_turn_context_tokens:,}  "
                      f"turns={baseline.num_turns}"
                      f"{'  [ERROR]' if baseline.is_error else ''}")

            if "skill" in only:
                print("  skill: running…")
                skill = run_claude(
                    SKILL_PROMPT.format(question=qtext),
                    cwd=skill_dir,
                    # Don't restrict — let the skill use whatever its body
                    # declares. The skill is self-contained.
                    allowed_tools=None,
                    model=args.model, verbose=args.verbose,
                )
                print(f"  skill:    {skill.elapsed_s:.1f}s  "
                      f"${skill.total_cost_usd:.4f}  "
                      f"out={skill.output_tokens}  "
                      f"peak_ctx={skill.peak_turn_context_tokens:,}  "
                      f"final_ctx={skill.final_turn_context_tokens:,}  "
                      f"turns={skill.num_turns}"
                      f"{'  [ERROR]' if skill.is_error else ''}")

            verdict = "n/a"
            judge_a_was = None
            if "judge" in only and baseline and skill and \
                    not baseline.is_error and not skill.is_error:
                swap = rng.random() < 0.5
                if swap:
                    a_ans, b_ans = skill.answer, baseline.answer
                    a_label, b_label = "skill", "baseline"
                else:
                    a_ans, b_ans = baseline.answer, skill.answer
                    a_label, b_label = "baseline", "skill"
                judge_a_was = a_label
                print("  judge: comparing…")
                raw = judge_answers(
                    qtext, a_ans, b_ans,
                    model=args.judge_model or args.model, verbose=args.verbose,
                )
                if raw == "A":
                    verdict = a_label
                elif raw == "B":
                    verdict = b_label
                else:
                    verdict = raw
                print(f"  judge: {verdict} (A was {a_label}, raw={raw!r})")

            results.append({
                "question": qtext,
                "baseline": asdict(baseline) if baseline else None,
                "skill": asdict(skill) if skill else None,
                "judge_verdict": verdict,
                "judge_a_was": judge_a_was,
            })

            # Flush progress to disk incrementally so a mid-run crash
            # doesn't lose earlier results.
            args.output.write_text(json.dumps({
                "progress": {"completed": i, "total": len(questions)},
                "results": results,
            }, indent=2))

        summary = compute_summary(results)
        output = {"summary": summary, "results": results, "args": {
            "spec": str(args.spec),
            "clayers_repo": str(args.clayers_repo),
            "questions": str(args.questions),
            "model": args.model,
            "judge_model": args.judge_model,
            "seed": args.seed,
        }}
        args.output.write_text(json.dumps(output, indent=2))

        print("\n" + "=" * 72)
        print("SUMMARY")
        print("=" * 72)
        print_summary(summary)
        print(f"\nResults saved to {args.output}")
    return 0


def compute_summary(results: list[dict[str, Any]]) -> dict[str, Any]:
    def _sum(field: str, variant: str) -> float:
        return sum(
            r[variant].get(field, 0) if r.get(variant) else 0
            for r in results
        )

    def _mean(field: str, variant: str) -> float:
        vals = [
            r[variant].get(field, 0) for r in results
            if r.get(variant) and not r[variant].get("is_error")
        ]
        return sum(vals) / len(vals) if vals else 0.0

    def _max(field: str, variant: str) -> int:
        vals = [
            r[variant].get(field, 0) for r in results
            if r.get(variant) and not r[variant].get("is_error")
        ]
        return max(vals) if vals else 0

    def _variant_summary(v: str) -> dict[str, Any]:
        return {
            "total_cost_usd": _sum("total_cost_usd", v),
            "total_elapsed_s": _sum("elapsed_s", v),
            "total_output_tokens": _sum("output_tokens", v),
            "total_cache_read_input_tokens": _sum("cache_read_input_tokens", v),
            "total_turns": _sum("num_turns", v),
            # Main-agent context watermark metrics — these answer "how
            # much of my context window does this approach consume?".
            # Peak = watermark during the run; final = steady state at
            # the end. Averaged across questions, and also max-across
            # for worst-case sizing.
            "avg_peak_turn_context_tokens": _mean("peak_turn_context_tokens", v),
            "avg_final_turn_context_tokens": _mean("final_turn_context_tokens", v),
            "max_peak_turn_context_tokens": _max("peak_turn_context_tokens", v),
            "max_final_turn_context_tokens": _max("final_turn_context_tokens", v),
            "errors": sum(1 for r in results
                          if r.get(v) and r[v].get("is_error")),
        }

    return {
        "questions": len(results),
        "baseline": _variant_summary("baseline"),
        "skill": _variant_summary("skill"),
        "judge": {
            "skill_wins": sum(1 for r in results if r["judge_verdict"] == "skill"),
            "baseline_wins": sum(1 for r in results if r["judge_verdict"] == "baseline"),
            "ties": sum(1 for r in results if r["judge_verdict"] == "tie"),
            "unparsed": sum(1 for r in results if r["judge_verdict"] == "?"),
            "skipped": sum(1 for r in results if r["judge_verdict"] == "n/a"),
        },
    }


def print_summary(s: dict[str, Any]) -> None:
    def _fmt_variant(name: str, v: dict[str, Any]) -> None:
        print(f"  {name}:")
        print(f"    cost:       ${v['total_cost_usd']:.4f}")
        print(f"    time:       {v['total_elapsed_s']:.1f}s")
        print(f"    output:     {v['total_output_tokens']:,} tokens")
        print(f"    turns:      {v['total_turns']}")
        print(f"    main-agent context size per question:")
        print(f"      avg peak:   {int(v['avg_peak_turn_context_tokens']):,} tokens")
        print(f"      avg final:  {int(v['avg_final_turn_context_tokens']):,} tokens")
        print(f"      max peak:   {v['max_peak_turn_context_tokens']:,} tokens")
        print(f"      max final:  {v['max_final_turn_context_tokens']:,} tokens")
        if v.get("errors"):
            print(f"    errors:     {v['errors']}")

    print(f"  questions: {s['questions']}")
    _fmt_variant("baseline", s["baseline"])
    _fmt_variant("skill", s["skill"])
    j = s["judge"]
    print(f"  judge:")
    print(f"    skill wins:    {j['skill_wins']}")
    print(f"    baseline wins: {j['baseline_wins']}")
    print(f"    ties:          {j['ties']}")
    if j.get("unparsed"):
        print(f"    unparsed:      {j['unparsed']}")
    if j.get("skipped"):
        print(f"    skipped:       {j['skipped']}")


if __name__ == "__main__":
    sys.exit(main())
