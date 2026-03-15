# /// script
# dependencies = ["claude-agent-sdk", "rich", "anyio", "pyyaml"]
# ///
"""
Cognitive Layers Testing Harness

Empirically tests whether structured cognitive layers (living-spec XML format)
improve AI agent performance on software development tasks.

Usage:
    uv run --script docs/ideas/living-spec/clayers-harness.py --repo https://github.com/pallets/flask --runs 3
"""

from __future__ import annotations

import argparse
import asyncio
import json
import os
import re
import signal
import shutil
import statistics
import subprocess
import sys
import time
from collections import deque
from dataclasses import dataclass, field, asdict
from datetime import datetime
from pathlib import Path
from typing import Any

try:
    import yaml
except ImportError:
    yaml = None  # type: ignore[assignment]

try:
    import claude_agent_sdk as _default_sdk
except ImportError:
    _default_sdk = None  # type: ignore[assignment]

try:
    from rich.console import Console, Group
    from rich.progress import (
        Progress,
        SpinnerColumn,
        BarColumn,
        TextColumn,
        TaskProgressColumn,
    )
    from rich.live import Live
    from rich.panel import Panel
    from rich.text import Text

    _HAS_RICH = True
    console = Console()
except ImportError:
    _HAS_RICH = False

    # Minimal fallback for when rich isn't installed (e.g. test runner)
    class _FallbackConsole:
        def print(self, *args, **kwargs):
            text = " ".join(str(a) for a in args)
            # Strip rich markup
            import re as _re

            text = _re.sub(r"\[/?[^\]]*\]", "", text)
            print(text)

    console = _FallbackConsole()  # type: ignore[assignment]

# ---------------------------------------------------------------------------
# Graceful shutdown: track child processes so Ctrl-C kills them
# ---------------------------------------------------------------------------

_shutting_down = False


def _kill_claude_children():
    """Find and kill all Claude CLI child processes spawned by this harness."""
    my_pid = os.getpid()
    try:
        # Use pgrep to find claude processes whose parent is us (or our children)
        result = subprocess.run(
            ["pgrep", "-P", str(my_pid)],
            capture_output=True,
            text=True,
            timeout=5,
        )
        child_pids = [int(p) for p in result.stdout.strip().split() if p.strip()]
        # Recursively find grandchildren too
        all_pids = set(child_pids)
        for pid in child_pids:
            try:
                result = subprocess.run(
                    ["pgrep", "-P", str(pid)],
                    capture_output=True,
                    text=True,
                    timeout=5,
                )
                grandchildren = [
                    int(p) for p in result.stdout.strip().split() if p.strip()
                ]
                all_pids.update(grandchildren)
            except Exception:
                pass
        # Kill all found processes (children first, then grandchildren)
        for pid in all_pids:
            try:
                os.kill(pid, signal.SIGTERM)
            except (ProcessLookupError, PermissionError, OSError):
                pass
        # Give them a moment, then SIGKILL stragglers
        if all_pids:
            time.sleep(0.5)
            for pid in all_pids:
                try:
                    os.kill(pid, signal.SIGKILL)
                except (ProcessLookupError, PermissionError, OSError):
                    pass
    except Exception:
        pass


# ---------------------------------------------------------------------------
# Default feature task (expert-hydrated, specific requirements)
# ---------------------------------------------------------------------------

DEFAULT_FEATURE_TASK = """\
Implement a document retention policy system with automated expiration and enforcement.

This system lets administrators define rules that automatically archive, delete, or flag
documents based on age, type, tags, and custom metadata. It must integrate with the
existing document model, storage backends, permissions, and background task infrastructure.

Requirements:
1. Retention policy model: a policy has a name, match criteria (document type, tags,
   age threshold, custom metadata filters), an action (archive, soft-delete, hard-delete,
   notify-only), and a priority for conflict resolution when multiple policies match.
2. Grace period: before any destructive action, documents enter a "pending expiration"
   state for a configurable grace period (default 30 days). Users are notified and can
   mark documents as "exempt" to prevent the action.
3. Scheduled background task: a periodic Celery/background task evaluates all active
   policies against all documents, transitions documents through states (active ->
   pending_expiration -> expired/archived/deleted), and records all actions in an audit log.
4. Audit log model: every retention action (policy matched, grace period started, document
   archived/deleted, exemption granted) is logged with timestamp, policy reference,
   document reference, acting user (or "system"), and action details.
5. REST API endpoints: full CRUD for retention policies, list documents pending expiration,
   grant exemptions, view audit log with filtering/pagination. Must follow the project's
   existing API patterns (serializers, viewsets, permissions, URL structure).
6. Dry-run mode: a management command and API endpoint that evaluates policies without
   taking any action, returning a report of what would happen.
7. Conflict resolution: when multiple policies match a document, the highest-priority
   policy wins. If priorities tie, the least destructive action wins (notify < archive <
   soft-delete < hard-delete).
8. Integration with existing permissions: only users with appropriate permissions can
   create/modify policies or grant exemptions. Document owners are notified during
   grace periods.
9. Tests: unit tests for policy matching logic and conflict resolution, integration tests
   for the background task lifecycle, API endpoint tests, edge cases (overlapping policies,
   concurrent task execution, exempt documents, empty match criteria).
10. The implementation must discover and follow the project's conventions by reading
    existing code: model patterns, serializer style, viewset structure, URL routing,
    test organization, and task registration.
"""

# ---------------------------------------------------------------------------
# Data classes
# ---------------------------------------------------------------------------


@dataclass
class CostTracker:
    """Accumulates total_cost_usd and token usage from ResultMessage across all agent calls."""

    phase1_cost: float = 0.0
    phase2_cost: float = 0.0
    phase3_cost: float = 0.0
    total_cost: float = 0.0
    total_input_tokens: int = 0
    total_output_tokens: int = 0
    total_duration_ms: int = 0
    calls: list[dict] = field(default_factory=list)

    def record(
        self,
        phase: str,
        cost: float | None,
        duration_ms: int = 0,
        num_turns: int = 0,
        usage: dict | None = None,
    ):
        amt = cost or 0.0
        input_tokens = (usage or {}).get("input_tokens", 0) or 0
        output_tokens = (usage or {}).get("output_tokens", 0) or 0
        self.calls.append(
            {
                "phase": phase,
                "cost_usd": amt,
                "duration_ms": duration_ms,
                "num_turns": num_turns,
                "input_tokens": input_tokens,
                "output_tokens": output_tokens,
            }
        )
        if phase.startswith("phase1"):
            self.phase1_cost += amt
        elif phase.startswith("phase2"):
            self.phase2_cost += amt
        elif phase.startswith("phase3"):
            self.phase3_cost += amt
        self.total_cost += amt
        self.total_input_tokens += input_tokens
        self.total_output_tokens += output_tokens
        self.total_duration_ms += duration_ms

    @property
    def tokens_per_minute(self) -> tuple[float, float]:
        """Return (input_tokens/min, output_tokens/min) across all calls."""
        if self.total_duration_ms <= 0:
            return (0.0, 0.0)
        minutes = self.total_duration_ms / 60_000
        return (self.total_input_tokens / minutes, self.total_output_tokens / minutes)

    def save(self, path: Path):
        path.write_text(json.dumps(asdict(self), indent=2))


@dataclass
class Checkpoint:
    phase1_done: bool = False
    comprehension_done: bool = False
    phase2_runs_completed: int = 0
    phase3_runs_completed: int = 0
    spec_dir: str | None = None
    paths: dict[str, str] = field(default_factory=dict)

    def save(self, path: Path):
        path.write_text(json.dumps(asdict(self), indent=2))

    @classmethod
    def load(cls, path: Path) -> Checkpoint:
        if path.exists():
            data = json.loads(path.read_text())
            # Filter to known fields so old checkpoints without new fields still load
            known = {f.name for f in cls.__dataclass_fields__.values()}
            return cls(**{k: v for k, v in data.items() if k in known})
        return cls()


# ---------------------------------------------------------------------------
# Rich terminal UI
# ---------------------------------------------------------------------------


class HarnessUI:
    """Live progress display with phase bars, conversation scroll, and cost tracker."""

    # How many lines to keep in the scrolling conversation log
    MAX_SCROLL_LINES = 12
    # Max width for each conversation line (truncated with ellipsis)
    MAX_LINE_WIDTH = 100

    def __init__(
        self,
        total_runs: int = 1,
        rounds: int = 2,
        cost_tracker: CostTracker | None = None,
    ):
        self._total_runs = total_runs
        self._rounds = rounds
        self._cost_tracker = cost_tracker
        self._run = 0
        self._status = ""
        self._live: Any = None
        self._agent_label = ""
        self._conversation: deque[str] = deque(maxlen=self.MAX_SCROLL_LINES)
        if not _HAS_RICH:
            return
        self._progress = Progress(
            SpinnerColumn(),
            TextColumn("[bold]{task.description}[/bold]", justify="left"),
            BarColumn(bar_width=20),
            TaskProgressColumn(),
            TextColumn("{task.fields[status]}", justify="left"),
            expand=False,
        )
        self._p1 = self._progress.add_task(
            "Phase 1: Extraction",
            total=rounds * 5 + 1,
            completed=0,
            status="pending",
        )
        self._p2 = self._progress.add_task(
            "Phase 2: Development",
            total=4,
            completed=0,
            status="pending",
        )
        self._p3 = self._progress.add_task(
            "Phase 3: Analysis",
            total=4,
            completed=0,
            status="pending",
        )

    def _renderable(self):
        cost = self._cost_tracker.total_cost if self._cost_tracker else 0.0
        status_line = Text(f"  \u25b8 {self._status}") if self._status else Text("")
        run_text = f"Run {self._run}/{self._total_runs}" if self._total_runs > 1 else ""
        info_parts = [p for p in [run_text, f"Cost: ${cost:.2f}"] if p]
        # Token throughput
        if self._cost_tracker:
            in_tpm, out_tpm = self._cost_tracker.tokens_per_minute
            total_in = self._cost_tracker.total_input_tokens
            total_out = self._cost_tracker.total_output_tokens
            if total_in or total_out:
                info_parts.append(
                    f"Tokens: {total_in:,}in/{total_out:,}out"
                    f" ({in_tpm:,.0f}/{out_tpm:,.0f} tok/min)"
                )
        info_line = Text(f"  {' \u2502 '.join(info_parts)}")

        # Build conversation scroll box
        parts: list = [self._progress, Text(""), status_line, info_line]

        if self._conversation:
            parts.append(Text(""))
            conv_renderables = [Text.from_markup(line) for line in self._conversation]
            parts.append(
                Panel(
                    Group(*conv_renderables),
                    title="[dim]Agent Activity[/dim]",
                    border_style="dim",
                    height=min(len(self._conversation) + 2, self.MAX_SCROLL_LINES + 2),
                    expand=True,
                )
            )

        return Panel(
            Group(*parts),
            title="[bold]Cognitive Layers Harness[/bold]",
            border_style="blue",
        )

    def set_agent(self, label: str):
        """Set the current agent label (e.g. 'Explorer', 'Session A')."""
        self._agent_label = label

    def add_message(self, role: str, text: str):
        """Add a message line to the scrolling conversation log."""
        if not text or not text.strip():
            return
        # Take first non-empty line, truncate
        first_line = ""
        for raw_line in text.split("\n"):
            stripped = raw_line.strip()
            if stripped:
                first_line = stripped
                break
        if not first_line:
            return
        w = self.MAX_LINE_WIDTH
        if len(first_line) > w:
            first_line = first_line[: w - 1] + "\u2026"
        # Escape markup in agent output so it renders literally
        from rich.markup import escape

        first_line = escape(first_line)
        label = self._agent_label or role
        self._conversation.append(f"[dim]{label}[/dim]  {first_line}")
        self._refresh()

    def add_tool_use(
        self,
        tool_name: str,
        tool_input: dict | None = None,
        agent_label: str | None = None,
    ):
        """Log a tool use in the conversation scroll with detail."""
        label = agent_label or self._agent_label or "Agent"
        from rich.markup import escape

        detail = ""
        if tool_input:
            # Show the most informative field
            for key in ("command", "pattern", "file_path", "prompt", "path", "query"):
                if key in tool_input and tool_input[key]:
                    val = str(tool_input[key])
                    max_detail = self.MAX_LINE_WIDTH - len(tool_name) - len(label) - 10
                    if len(val) > max_detail:
                        val = val[: max_detail - 1] + "\u2026"
                    detail = f" [dim]{escape(val)}[/dim]"
                    break
        self._conversation.append(
            f"[dim]{label}[/dim]  [cyan]{escape(tool_name)}[/cyan]{detail}"
        )
        self._refresh()

    def start(self):
        if not _HAS_RICH:
            return
        self._live = Live(self._renderable(), console=console, refresh_per_second=4)
        self._live.start()

    def stop(self):
        if self._live:
            self._live.stop()
            self._live = None

    def __enter__(self):
        self.start()
        return self

    def __exit__(self, *args):
        self.stop()

    def _refresh(self):
        if self._live:
            self._live.update(self._renderable())

    def set_run(self, run: int):
        self._run = run
        # Reset phase 2+3 for new run
        if _HAS_RICH:
            self._progress.reset(self._p2)
            self._progress.update(self._p2, status="pending")
            self._progress.reset(self._p3)
            self._progress.update(self._p3, status="pending")
        self._refresh()

    def start_phase(self, phase: int, total: int | None = None):
        if not _HAS_RICH:
            return
        task_id = [self._p1, self._p2, self._p3][phase - 1]
        if total is not None:
            self._progress.update(task_id, total=total, completed=0)
        else:
            self._progress.reset(task_id)
        self._refresh()

    def advance(self, phase: int, status: str = ""):
        if not _HAS_RICH:
            return
        task_id = [self._p1, self._p2, self._p3][phase - 1]
        self._progress.update(task_id, advance=1, status=status)
        self._status = status
        self._refresh()

    def complete_phase(self, phase: int):
        if not _HAS_RICH:
            return
        task_id = [self._p1, self._p2, self._p3][phase - 1]
        task = self._progress.tasks[task_id]
        self._progress.update(
            task_id, completed=task.total, status="[green]done[/green]"
        )
        self._status = ""
        self._refresh()

    def skip_phase(self, phase: int):
        if not _HAS_RICH:
            return
        task_id = [self._p1, self._p2, self._p3][phase - 1]
        task = self._progress.tasks[task_id]
        self._progress.update(task_id, completed=task.total, status="[dim]cached[/dim]")
        self._refresh()

    def update_status(self, text: str):
        self._status = text
        self._refresh()


_ui: HarnessUI | None = None


def _ui_advance(phase: int, status: str = ""):
    if _ui:
        _ui.advance(phase, status)


def _ui_status(text: str):
    if _ui:
        _ui.update_status(text)


def _ui_set_agent(label: str):
    if _ui:
        _ui.set_agent(label)


# ---------------------------------------------------------------------------
# Helper functions
# ---------------------------------------------------------------------------


async def collect_messages(
    stream, sdk=_default_sdk, agent_label: str = ""
) -> tuple[list, Any]:
    """Drain async iterator, return (messages, result_message)."""
    messages = []
    result = None
    async for msg in stream:
        messages.append(msg)
        if isinstance(msg, sdk.ResultMessage):
            result = msg
        elif isinstance(msg, sdk.AssistantMessage) and _ui:
            label = agent_label or _ui._agent_label or "Agent"
            for block in msg.content:
                if isinstance(block, sdk.TextBlock) and block.text:
                    _ui.add_message(label, block.text)
                elif isinstance(block, sdk.ToolUseBlock):
                    tool_input = block.input if hasattr(block, "input") else None
                    _ui.add_tool_use(
                        block.name, tool_input=tool_input, agent_label=label
                    )
    return messages, result


def save_messages(messages, path: Path, sdk=_default_sdk):
    """Serialize messages to JSON."""
    serialized = []
    for msg in messages:
        if isinstance(msg, sdk.AssistantMessage):
            content_parts = []
            for block in msg.content:
                if isinstance(block, sdk.TextBlock):
                    content_parts.append({"type": "text", "text": block.text})
                elif isinstance(block, sdk.ToolUseBlock):
                    content_parts.append(
                        {"type": "tool_use", "name": block.name, "input": block.input}
                    )
            serialized.append({"type": "assistant", "content": content_parts})
        elif isinstance(msg, sdk.ResultMessage):
            serialized.append(
                {
                    "type": "result",
                    "total_cost_usd": msg.total_cost_usd,
                    "duration_ms": msg.duration_ms,
                    "num_turns": msg.num_turns,
                    "session_id": msg.session_id,
                }
            )
    path.write_text(json.dumps(serialized, indent=2))


def clone_project(repo_url: str, dest: Path) -> Path:
    """Git clone a project."""
    if dest.exists():
        return dest
    subprocess.run(
        ["git", "clone", "--depth=1", repo_url, str(dest)],
        check=True,
        capture_output=True,
    )
    return dest


def create_git_diff(workdir: Path) -> str:
    """Capture git diff of all changes."""
    # Stage everything first
    subprocess.run(["git", "add", "-A"], cwd=workdir, capture_output=True)
    result = subprocess.run(
        ["git", "diff", "--cached", "--stat"],
        cwd=workdir,
        capture_output=True,
        text=True,
    )
    diff_stat = result.stdout
    result = subprocess.run(
        ["git", "diff", "--cached"],
        cwd=workdir,
        capture_output=True,
        text=True,
    )
    return f"--- DIFF STAT ---\n{diff_stat}\n--- FULL DIFF ---\n{result.stdout}"


def copy_living_spec_references(ls_dir: Path, workspace: Path) -> None:
    """Copy living-spec reference materials into workspace for agent access.

    Copies AGENTS.md, examples/, and specs/ alongside the already-copied
    schemas/ directory so agents can read them via file paths instead of
    having everything embedded in the system prompt.
    """
    # AGENTS.md
    agents_md = ls_dir / "AGENTS.md"
    dest_agents = workspace / "AGENTS.md"
    if agents_md.exists() and not dest_agents.exists():
        shutil.copy2(agents_md, dest_agents)

    # Payment-processing example
    example_dir = ls_dir / "examples" / "payment-processing"
    dest_examples = workspace / "examples" / "payment-processing"
    if example_dir.exists() and not dest_examples.exists():
        shutil.copytree(example_dir, dest_examples, dirs_exist_ok=True)

    # Self-referential spec (structural template)
    ls_spec = ls_dir / "specs" / "living-spec"
    dest_spec = workspace / "examples" / "living-spec"
    if ls_spec.exists() and not dest_spec.exists():
        shutil.copytree(ls_spec, dest_spec, dirs_exist_ok=True)


def living_spec_reference_instructions(workspace: Path) -> str:
    """Return instructions telling the agent where to find reference materials.

    Instead of embedding 60K+ of content in the system prompt, agents get
    file paths and are told to read them with the Read tool.
    """
    parts = [
        "## Living-Spec Reference Materials (read these files)",
        "",
        "You have the following reference materials available in your workspace.",
        "READ them before writing any spec files:",
        "",
        f"- **Agent guide**: `{workspace / 'AGENTS.md'}` - workflows, conventions, "
        "namespace table, XPath recipes",
        f"- **Schemas**: `{workspace / 'schemas/'}*.xsd` - each XSD has `llm:describe` "
        "annotations explaining what each element does",
        f"- **Example spec (payment-processing)**: `{workspace / 'examples/payment-processing/'}*.xml` "
        "- use this as your structural template",
        f"- **Self-referential spec**: `{workspace / 'examples/living-spec/'}*.xml` "
        "- the living-spec format describing itself",
        "",
        "Start by reading AGENTS.md, then browse the example XML files to understand "
        "the format before writing any spec files.",
    ]
    return "\n".join(parts)


def run_living_spec_tool(
    spec_dir: Path,
    command: str,
    *args: str,
    ls_script: Path | None = None,
) -> str:
    """Run living-spec.py as a subprocess.

    If ls_script is provided, uses that copy of living-spec.py instead of
    the default one. This is important for artifact path resolution:
    living-spec.py uses SCRIPT_DIR to find REPO_ROOT, so placing it inside
    the workspace lets artifact paths resolve correctly.
    """
    script = ls_script or (Path(__file__).parent / "living-spec.py")
    cmd = ["uv", "run", "--script", str(script), command, str(spec_dir)] + list(args)
    result = subprocess.run(cmd, capture_output=True, text=True, timeout=120)
    return f"exit_code={result.returncode}\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}"


def detect_project_language(project_dir: Path) -> str:
    """Auto-detect project language from config files."""
    if (project_dir / "pyproject.toml").exists() or (project_dir / "setup.py").exists():
        return "python"
    if (project_dir / "package.json").exists():
        return "javascript"
    if (project_dir / "Cargo.toml").exists():
        return "rust"
    if (project_dir / "go.mod").exists():
        return "go"
    return "python"  # fallback


def compute_code_metrics(diff_text: str) -> dict:
    """Compute lines_added, lines_removed, files_changed, test_functions from diff."""
    lines_added = 0
    lines_removed = 0
    files_changed = set()
    test_functions = 0

    for line in diff_text.split("\n"):
        if line.startswith("diff --git"):
            # Extract filename
            parts = line.split(" b/")
            if len(parts) > 1:
                files_changed.add(parts[1])
        elif line.startswith("+") and not line.startswith("+++"):
            lines_added += 1
            # Count test function definitions in added lines
            stripped = line[1:].strip()
            if re.match(
                r"(def test_|async def test_|it\(|describe\(|#\[test\]|func Test)",
                stripped,
            ):
                test_functions += 1
        elif line.startswith("-") and not line.startswith("---"):
            lines_removed += 1

    return {
        "lines_added": lines_added,
        "lines_removed": lines_removed,
        "files_changed": len(files_changed),
        "test_functions": test_functions,
    }


# ---------------------------------------------------------------------------
# Phase 1: Living-spec extraction pipeline (subagent-based)
# ---------------------------------------------------------------------------


async def phase1_extract(
    project_dir: Path,
    output_dir: Path,
    ls_dir: Path,
    rounds: int = 2,
    model: str | None = None,
    max_budget: float | None = None,
    no_cache: bool = False,
    cost_tracker: CostTracker | None = None,
    sdk=_default_sdk,
) -> Path:
    """Extract a living-spec from the project via focused subagent collaboration.

    Each round uses three focused subagents instead of one monolithic session:
      1. Explorer: reads codebase, produces structured architecture summary
      2. Spec Writer (Architect Extractor): writes XML spec files from summary
      3. Artifact Mapper: creates art:mapping elements with exact line numbers

    Then validator and compliance reviewer provide feedback for the next round.
    """
    cost_tracker = cost_tracker or CostTracker()
    project_name = project_dir.name

    # Cache check - key includes rounds + model so different configs don't collide
    model_tag = (model or "default").replace("claude-", "").replace("-", "")
    cache_key = f"{project_name}_r{rounds}_{model_tag}"
    cache_dir = output_dir.parent / "_cache" / cache_key
    if not no_cache and cache_dir.exists() and (cache_dir / "final_spec").exists():
        console.print(
            f"[green]Using cached Phase 1 spec from {cache_dir / 'final_spec'}[/green]"
        )
        if _ui:
            _ui.skip_phase(1)
        return cache_dir / "final_spec"

    extraction_dir = output_dir / "phase1_extraction"
    extraction_dir.mkdir(parents=True, exist_ok=True)

    # Agents read project docs (README.md, CONTRIBUTING.md) directly via file paths

    feedback = ""
    spec_dir = None

    if _ui:
        _ui.start_phase(1, rounds * 5 + 1)

    for round_num in range(1, rounds + 1):
        console.print(f"\n[bold blue]Phase 1 - Round {round_num}/{rounds}[/bold blue]")
        round_dir = extraction_dir / f"round_{round_num}"

        # Create workspace: git-init'd directory with project symlink + spec/
        round_workspace = round_dir / "workspace"
        round_workspace.mkdir(parents=True, exist_ok=True)
        spec_dir = round_workspace / "spec"
        spec_dir.mkdir(exist_ok=True)
        project_link = round_workspace / "project"
        if not project_link.exists():
            project_link.symlink_to(project_dir.resolve())
        # Copy living-spec.py + schemas + reference materials into workspace
        ws_ls_script = round_workspace / "living-spec.py"
        if not ws_ls_script.exists():
            shutil.copy2(ls_dir / "living-spec.py", ws_ls_script)
            if (ls_dir / "schemas").exists():
                shutil.copytree(
                    ls_dir / "schemas", round_workspace / "schemas", dirs_exist_ok=True
                )
        # Copy AGENTS.md, examples, self-referential spec for agent to read
        copy_living_spec_references(ls_dir, round_workspace)
        # Init git repo so living-spec.py's REPO_ROOT resolves to workspace
        if not (round_workspace / ".git").exists():
            subprocess.run(["git", "init"], cwd=round_workspace, capture_output=True)
            subprocess.run(
                ["git", "add", "-A"], cwd=round_workspace, capture_output=True
            )
            subprocess.run(
                ["git", "commit", "-m", "init", "--allow-empty"],
                cwd=round_workspace,
                capture_output=True,
                env={
                    **os.environ,
                    "GIT_AUTHOR_NAME": "harness",
                    "GIT_AUTHOR_EMAIL": "h@h",
                    "GIT_COMMITTER_NAME": "harness",
                    "GIT_COMMITTER_EMAIL": "h@h",
                },
            )

        # ---------------------------------------------------------------
        # Subagent 1: Codebase Explorer
        # ---------------------------------------------------------------
        console.print("  [cyan]Subagent 1/3: Exploring codebase...[/cyan]")
        _ui_set_agent("Explorer")
        explorer_prompt = (
            f"Explore the project in {project_link}/ (absolute path). "
            "Read key source files (__init__.py, app.py, core modules, test files, config). "
            "Produce a comprehensive summary covering:\n"
            "- Every major module, class, function with file path, line numbers, purpose, relationships\n"
            "- Data serialization / API layer: serializers, schemas, marshmallow/pydantic models, "
            "  REST/GraphQL viewsets, form classes - how data enters and leaves the system\n"
            "- Test organization: where tests live, naming conventions, fixtures, factories, "
            "  conftest patterns, what percentage is unit vs integration vs e2e\n"
            "- Framework-specific idioms: middleware chain, signal/hook registration patterns, "
            "  plugin/extension discovery mechanisms, configuration system (settings hierarchy, "
            "  env var overrides, feature flags)\n"
            "- Key design patterns, extension points, and project-specific terminology\n"
            "- External dependencies and integration points (databases, caches, message queues, "
            "  third-party APIs) with how they are configured and accessed"
        )
        if feedback:
            explorer_prompt += (
                f"\n\nPrevious round feedback is at: {feedback}\n"
                "Read it to understand gaps that need to be addressed."
            )

        stream = sdk.query(
            prompt=explorer_prompt,
            options=sdk.ClaudeAgentOptions(
                system_prompt=(
                    "You are a Codebase Explorer. Read the project source code thoroughly and "
                    "produce a structured summary covering:\n"
                    "- Architecture: modules, classes, functions with file paths and line numbers\n"
                    "- Data flow: how data enters (API/CLI/UI), gets processed, stored, and returned\n"
                    "- Serialization layer: serializers, schemas, form/validation classes\n"
                    "- Test organization: test directory structure, naming conventions, fixtures, "
                    "factories, conftest patterns\n"
                    "- Framework idioms: middleware, signals/hooks, plugin discovery, config system\n"
                    "- External integrations: databases, caches, queues, third-party APIs\n"
                    "- CLI/management commands and admin tools\n\n"
                    "Include file paths and line numbers for ALL key constructs. Be exhaustive.\n\n"
                    "The project's README.md and CONTRIBUTING.md are in the project/ directory. "
                    "Read them for context on the project's purpose and architecture."
                ),
                cwd=str(round_workspace),
                permission_mode="bypassPermissions",
                model=model,
                max_turns=20,
                max_budget_usd=max_budget,
            ),
        )
        explorer_msgs, explorer_result = await collect_messages(stream, sdk=sdk)
        save_messages(explorer_msgs, round_dir / "explorer_messages.json", sdk=sdk)
        if explorer_result:
            cost_tracker.record(
                "phase1_explorer",
                explorer_result.total_cost_usd,
                explorer_result.duration_ms,
                explorer_result.num_turns,
                usage=explorer_result.usage,
            )
            console.print(
                f"  Explorer cost: ${explorer_result.total_cost_usd or 0:.2f}"
            )
        _ui_advance(1, f"R{round_num}: Explorer done")

        # Save explorer summary to file so Architect can read it
        explorer_summary_text = ""
        for m in explorer_msgs:
            if hasattr(m, "content"):
                for block in m.content:
                    if hasattr(block, "text"):
                        explorer_summary_text += block.text
        explorer_summary_path = round_dir / "explorer_summary.txt"
        explorer_summary_path.write_text(explorer_summary_text)

        # ---------------------------------------------------------------
        # Subagent 2: Spec Writer (Architect Extractor)
        # ---------------------------------------------------------------
        console.print(
            "  [cyan]Subagent 2/3: Writing spec files (Architect Extractor)...[/cyan]"
        )
        _ui_set_agent("Architect")
        ls_ref_instructions = living_spec_reference_instructions(round_workspace)
        architect_system = f"""\
You are a Living-Spec Architect Extractor. Your job is to write living-spec XML
specification files based on a codebase analysis summary.

{ls_ref_instructions}

## Codebase Analysis Summary

Read the explorer's analysis at: `{explorer_summary_path}`

## Instructions

Write spec files into the spec/ directory. Organize by CONCEPT, not by source file:
- index.xml (file manifest listing all spec files)
- overview.xml (architecture overview, technology stack, project philosophy)
- revision.xml (revision snapshot as "draft-1")
- Additional files named by concept domain (e.g., data-model.xml, api-layer.xml,
  processing-pipeline.xml, configuration.xml, testing.xml). Choose file names that
  reflect the project's actual conceptual domains. Do NOT mirror source file names
  (e.g., don't create "views.xml" just because there's a views.py).

For each concept:
1. Define terminology (trm:term) with precise definitions that distinguish this
   project's usage from generic meanings. E.g., if "document" means something
   specific, say exactly what makes it different from the generic word.
2. Write prose sections (pr:section) that explain WHY the concept exists and HOW
   it fits into the bigger picture, not just WHAT it is.
3. Classify topics (org:concept for domain objects, org:task for workflows/processes,
   org:reference for configuration/API surfaces).
4. Add relations (rel:relation) capturing real architectural dependencies:
   depends-on, precedes, refines, extends. Every concept should have at least
   one relation connecting it to another concept.
5. Add source citations (src:source, src:cite) for external references: link to
   the project's official documentation, relevant PEPs, RFCs, or library docs
   that explain design choices. Use src:source declarations in overview.xml and
   src:cite inline where relevant.
6. Write LLM descriptions (llm:node) that answer: "When should an agent read
   this section? What questions does it answer? What decisions does it inform?"
   Do NOT just restate the prose content. The LLM description is a routing hint
   for agents deciding which spec sections to consult.

Use the payment-processing example as your structural template.

IMPORTANT: Do NOT create a separate file for every source file in the project.
Group related concepts together. A spec about a web framework might have
data-model.xml (ORM, serializers, validation), request-handling.xml (routing,
middleware, views), and extension-system.xml (plugins, hooks, signals) rather
than models.xml, serializers.xml, views.xml, urls.xml, middleware.xml.
"""

        architect_prompt = (
            f"Create the living-spec XML files in {spec_dir}/ (absolute path). "
            "Use the codebase analysis summary to accurately describe the project's "
            "architecture, concepts, terminology, and relationships."
        )
        if feedback:
            architect_prompt += (
                f"\n\nPrevious round feedback is at: {feedback}\n"
                "Read it to understand issues that need to be fixed."
            )

        stream = sdk.query(
            prompt=architect_prompt,
            options=sdk.ClaudeAgentOptions(
                system_prompt=architect_system,
                cwd=str(round_workspace),
                permission_mode="bypassPermissions",
                model=model,
                max_turns=30,
                max_budget_usd=max_budget,
            ),
        )
        writer_msgs, writer_result = await collect_messages(stream, sdk=sdk)
        save_messages(writer_msgs, round_dir / "architect_messages.json", sdk=sdk)
        if writer_result:
            cost_tracker.record(
                "phase1_architect",
                writer_result.total_cost_usd,
                writer_result.duration_ms,
                writer_result.num_turns,
                usage=writer_result.usage,
            )
            console.print(
                f"  Spec Writer cost: ${writer_result.total_cost_usd or 0:.2f}"
            )
        _ui_advance(1, f"R{round_num}: Spec Writer done")

        # ---------------------------------------------------------------
        # Subagent 3: Artifact Mapper
        # ---------------------------------------------------------------
        console.print("  [cyan]Subagent 3/3: Creating artifact mappings...[/cyan]")
        _ui_set_agent("Mapper")
        mapper_system = f"""\
You are an Artifact Mapper for living-spec specifications. Your job is to create
art:mapping elements that link spec nodes to their actual source code locations.

## Artifact Mapping Format
```xml
<art:mapping id="map-CONCEPT-NAME">
  <art:spec-ref node="CONCEPT-ID" revision="draft-1" node-hash="sha256:placeholder"/>
  <art:artifact repo="workspace" repo-revision="HEAD"
            path="project/path/to/file.py">
    <art:range hash="sha256:placeholder"
               start-line="START" end-line="END"/>
  </art:artifact>
  <art:coverage>full</art:coverage>
  <art:note>Brief description of WHAT this code implements relative to the spec concept.</art:note>
</art:mapping>
```

## Instructions
1. Read each spec file in {spec_dir}/ to find all spec nodes (elements with id attributes)
2. For each spec node describing a concrete code construct (class, function, module):
   - Read the ACTUAL source file in {project_link}/ to find exact line numbers
   - Verify the line numbers are correct by reading the file and counting lines
   - Create an art:mapping linking the spec node to the code location
   - Use repo="workspace" and prefix paths with "project/"
   - Use sha256:placeholder for all hashes (tooling computes real values)
   - A single spec node can map to MULTIPLE code locations (add multiple art:artifact
     elements or multiple art:range elements within one artifact)
3. Add mappings to the appropriate spec file or create traceability.xml
4. Update index.xml if you create a new file
5. After creating ALL mappings, run these commands to compute real hashes:
   ```
   uv run --script living-spec.py artifact --fix-node-hash {spec_dir}/
   uv run --script living-spec.py artifact --fix-artifact-hash {spec_dir}/
   ```

Aim for comprehensive coverage: every spec node describing code should have a mapping.
Do NOT map abstract concepts that have no single code location (e.g., "architecture
philosophy") - only map concepts with concrete implementations.
"""
        mapper_prompt = (
            f"Create artifact mappings for all spec nodes in {spec_dir}/. "
            f"Read source files in {project_link}/ to find exact line numbers. "
            "Every spec node describing a concrete code construct must have an art:mapping."
        )
        stream = sdk.query(
            prompt=mapper_prompt,
            options=sdk.ClaudeAgentOptions(
                system_prompt=mapper_system,
                cwd=str(round_workspace),
                permission_mode="bypassPermissions",
                model=model,
                max_turns=25,
                max_budget_usd=max_budget,
            ),
        )
        mapper_msgs, mapper_result = await collect_messages(stream, sdk=sdk)
        save_messages(mapper_msgs, round_dir / "mapper_messages.json", sdk=sdk)
        if mapper_result:
            cost_tracker.record(
                "phase1_mapper",
                mapper_result.total_cost_usd,
                mapper_result.duration_ms,
                mapper_result.num_turns,
                usage=mapper_result.usage,
            )
            console.print(
                f"  Artifact Mapper cost: ${mapper_result.total_cost_usd or 0:.2f}"
            )
        _ui_advance(1, f"R{round_num}: Mapper done")

        # ---------------------------------------------------------------
        # Post-extraction validation + artifact coverage nudge
        # ---------------------------------------------------------------
        ws_ls = round_workspace / "living-spec.py"
        console.print(
            "  [cyan]Running validate + connectivity + artifact coverage...[/cyan]"
        )
        validate_output = run_living_spec_tool(spec_dir, "validate", ls_script=ws_ls)
        connectivity_output = run_living_spec_tool(
            spec_dir, "connectivity", ls_script=ws_ls
        )
        coverage_output = run_living_spec_tool(
            spec_dir, "artifact", "--coverage", ls_script=ws_ls
        )
        (round_dir / "validation_result.txt").write_text(validate_output)
        (round_dir / "connectivity_result.txt").write_text(connectivity_output)
        (round_dir / "coverage_result.txt").write_text(coverage_output)

        # Coverage nudge loop: keep nudging until near-100% coverage
        for nudge_idx in range(3):
            if "exit_code=0" in coverage_output:
                console.print("  [green]Artifact coverage OK[/green]")
                break
            console.print(
                f"  [yellow]Artifact coverage incomplete (nudge {nudge_idx + 1}/3)...[/yellow]"
            )
            nudge_prompt = (
                f"The artifact coverage check found gaps. Here is the output:\n\n"
                f"{coverage_output}\n\n"
                f"Add art:mapping elements for all uncovered spec nodes. "
                f"Read the project source code in {project_link}/ to find exact file paths and "
                f"line numbers. Write updated spec files to {spec_dir}/ (absolute path). "
                f"Every spec node describing a concrete code construct (class, function, "
                f"module, config variable) must have an artifact mapping."
            )
            stream = sdk.query(
                prompt=nudge_prompt,
                options=sdk.ClaudeAgentOptions(
                    system_prompt=mapper_system,
                    cwd=str(round_workspace),
                    permission_mode="bypassPermissions",
                    model=model,
                    max_turns=20,
                    max_budget_usd=max_budget,
                ),
            )
            nudge_msgs, nudge_result = await collect_messages(stream, sdk=sdk)
            if nudge_result:
                cost_tracker.record(
                    "phase1_coverage_nudge",
                    nudge_result.total_cost_usd,
                    nudge_result.duration_ms,
                    nudge_result.num_turns,
                    usage=nudge_result.usage,
                )
            coverage_output = run_living_spec_tool(
                spec_dir, "artifact", "--coverage", ls_script=ws_ls
            )
            (round_dir / f"coverage_nudge_{nudge_idx}.txt").write_text(coverage_output)

        # ---------------------------------------------------------------
        # Domain Validator agent
        # ---------------------------------------------------------------
        validator_system = """\
You are a domain expert reviewer for the target project. You have deep knowledge
of its architecture, design patterns, and idioms. Analyze the extracted specification for:

- Accuracy: do term definitions match actual project semantics? Does each term
  distinguish the project's specific usage from the generic meaning of the word?
- Completeness: check EACH of these categories explicitly:
  * Core data model: are all major models/entities covered?
  * API/serialization layer: are serializers, schemas, form classes, viewsets captured?
  * Test patterns: is the test organization, fixture system, factory patterns documented?
  * Configuration system: settings hierarchy, env var overrides, feature flags?
  * External integrations: databases, caches, message queues, third-party APIs?
  * Extension/plugin system: hook registration, middleware chain, signal patterns?
  * CLI/management commands: are admin tools and management utilities covered?
  Report any missing categories as completeness gaps.
- Relations: are dependency/precedes/refines edges directionally correct? Does every
  concept have at least one relation? Are there obvious missing relations?
- Organization: are concept/task/reference classifications appropriate? Concepts
  should be domain objects, tasks should be workflows/processes, references should
  be configuration/API surfaces.
- Depth: are descriptions substantive or superficial? Do they explain WHY the
  concept exists and HOW it fits into the bigger picture, or do they just restate
  what can be seen from reading the source code?
- Structure: are spec files organized by concept domain (good) or do they mirror
  source file names (bad)? A file named "views.xml" just because there's a views.py
  is a structural problem.

Return your feedback as JSON with this schema:
{"accuracy_issues": [...], "completeness_gaps": [...], "relation_issues": [...],
  "organization_issues": [...], "depth_issues": [...], "structure_issues": [...],
  "overall_score": 1-10}
"""
        console.print("  [cyan]Running Domain Validator...[/cyan]")
        _ui_set_agent("Validator")
        stream = sdk.query(
            prompt=(
                f"Review the extracted specification. "
                f"Read all XML files in {spec_dir}/ to see the spec content. "
                f"Read {project_link}/README.md for project context."
            ),
            options=sdk.ClaudeAgentOptions(
                system_prompt=validator_system,
                permission_mode="bypassPermissions",
                model=model,
                max_turns=10,
                max_budget_usd=max_budget,
                output_format={
                    "type": "json_schema",
                    "schema": {
                        "type": "object",
                        "properties": {
                            "accuracy_issues": {
                                "type": "array",
                                "items": {"type": "string"},
                            },
                            "completeness_gaps": {
                                "type": "array",
                                "items": {"type": "string"},
                            },
                            "relation_issues": {
                                "type": "array",
                                "items": {"type": "string"},
                            },
                            "organization_issues": {
                                "type": "array",
                                "items": {"type": "string"},
                            },
                            "depth_issues": {
                                "type": "array",
                                "items": {"type": "string"},
                            },
                            "structure_issues": {
                                "type": "array",
                                "items": {"type": "string"},
                            },
                            "overall_score": {"type": "number"},
                        },
                    },
                },
            ),
        )
        validator_msgs, validator_result = await collect_messages(stream, sdk=sdk)
        save_messages(validator_msgs, round_dir / "validator_messages.json", sdk=sdk)
        if validator_result:
            cost_tracker.record(
                "phase1_validator",
                validator_result.total_cost_usd,
                validator_result.duration_ms,
                validator_result.num_turns,
                usage=validator_result.usage,
            )
        _ui_advance(1, f"R{round_num}: Validator done")

        # ---------------------------------------------------------------
        # Schema Compliance Reviewer agent
        # ---------------------------------------------------------------
        compliance_system = """\
You are a Schema Compliance Reviewer for living-spec specifications.
Review the extracted spec for structural compliance with the living-spec format.

You have the output of the validate and connectivity commands below.

Check:
- All XML validates against the schemas
- IDs are unique across all files
- Cross-references resolve (rel:relation from/to, trm:ref, pr:xref)
- Namespace prefixes are consistent with the convention table
- Every spec node has organization typing and LLM description
- Connectivity graph is balanced (no orphaned nodes)
- The index.xml lists all files
- Artifact mappings exist: spec nodes describing concrete code constructs
  (classes, functions, modules) must have art:mapping elements linking to
  source file paths and line ranges. Flag any spec nodes that describe code
  but lack artifact mappings.
- Source citations (src:source / src:cite): the spec should declare external
  references (official project docs, relevant PEPs/RFCs, library documentation)
  using src:source elements, and cite them inline with src:cite. A spec with
  zero src:source declarations is a compliance gap.
- LLM description quality: each llm:node must NOT merely restate the prose
  content. Instead it should answer: "When should an agent read this? What
  questions does it answer? What decisions does it inform?" Flag any llm:node
  whose text is essentially a restatement of the pr:section content.
- Artifact hash status: flag any art:mapping that still has sha256:placeholder
  hashes. After mapping, the tooling should have been run to compute real
  hashes via --fix-node-hash and --fix-artifact-hash.

Return your feedback as JSON:
{"schema_violations": [...], "id_issues": [...], "reference_issues": [...],
  "namespace_issues": [...], "coverage_gaps": [...], "source_citation_issues": [...],
  "llm_description_issues": [...], "hash_issues": [...], "overall_compliance": 1-10}
"""
        console.print("  [cyan]Running Schema Compliance Reviewer...[/cyan]")
        _ui_set_agent("Compliance")
        # Save tool outputs for the compliance reviewer to read
        validate_path = round_dir / "validation_result.txt"
        connectivity_path = round_dir / "connectivity_result.txt"
        stream = sdk.query(
            prompt=(
                f"Review the spec for compliance. Read these files:\n"
                f"- Spec XML files: {spec_dir}/*.xml\n"
                f"- Validation output: {validate_path}\n"
                f"- Connectivity output: {connectivity_path}"
            ),
            options=sdk.ClaudeAgentOptions(
                system_prompt=compliance_system,
                permission_mode="bypassPermissions",
                model=model,
                max_turns=10,
                max_budget_usd=max_budget,
                output_format={
                    "type": "json_schema",
                    "schema": {
                        "type": "object",
                        "properties": {
                            "schema_violations": {
                                "type": "array",
                                "items": {"type": "string"},
                            },
                            "id_issues": {"type": "array", "items": {"type": "string"}},
                            "reference_issues": {
                                "type": "array",
                                "items": {"type": "string"},
                            },
                            "namespace_issues": {
                                "type": "array",
                                "items": {"type": "string"},
                            },
                            "coverage_gaps": {
                                "type": "array",
                                "items": {"type": "string"},
                            },
                            "source_citation_issues": {
                                "type": "array",
                                "items": {"type": "string"},
                            },
                            "llm_description_issues": {
                                "type": "array",
                                "items": {"type": "string"},
                            },
                            "hash_issues": {
                                "type": "array",
                                "items": {"type": "string"},
                            },
                            "overall_compliance": {"type": "number"},
                        },
                    },
                },
            ),
        )
        compliance_msgs, compliance_result = await collect_messages(stream, sdk=sdk)
        save_messages(compliance_msgs, round_dir / "compliance_messages.json", sdk=sdk)
        if compliance_result:
            cost_tracker.record(
                "phase1_compliance",
                compliance_result.total_cost_usd,
                compliance_result.duration_ms,
                compliance_result.num_turns,
                usage=compliance_result.usage,
            )
        _ui_advance(1, f"R{round_num}: Compliance done")

        # ---------------------------------------------------------------
        # Build feedback for next round
        # ---------------------------------------------------------------
        validator_feedback = ""
        for m in validator_msgs:
            if hasattr(m, "content"):
                for block in m.content:
                    if hasattr(block, "text"):
                        validator_feedback += block.text

        compliance_feedback = ""
        for m in compliance_msgs:
            if hasattr(m, "content"):
                for block in m.content:
                    if hasattr(block, "text"):
                        compliance_feedback += block.text

        # Save feedback to a file so next round's agents can read it
        feedback_text = (
            f"## Domain Validator Feedback\n{validator_feedback}\n\n"
            f"## Schema Compliance Feedback\n{compliance_feedback}\n\n"
            f"## Validation Tool Output\n{validate_output}\n\n"
            f"## Connectivity Tool Output\n{connectivity_output}\n\n"
            f"## Artifact Coverage Output\n{coverage_output}"
        )
        feedback_path = round_dir / "feedback.txt"
        feedback_path.write_text(feedback_text)
        feedback = str(feedback_path)

        console.print(f"  Round {round_num} complete.")

    # Final quality gate: validation must pass and connectivity must be acceptable
    assert spec_dir is not None
    final_spec_dir = extraction_dir / "final_spec"
    if final_spec_dir.exists():
        shutil.rmtree(final_spec_dir)
    shutil.copytree(spec_dir, final_spec_dir)

    # Copy living-spec.py + schemas into final_spec's parent for repair agent
    ws_ls_script = final_spec_dir.parent / "living-spec.py"
    if not ws_ls_script.exists():
        shutil.copy2(ls_dir / "living-spec.py", ws_ls_script)
        if (ls_dir / "schemas").exists():
            shutil.copytree(
                ls_dir / "schemas",
                final_spec_dir.parent / "schemas",
                dirs_exist_ok=True,
            )

    max_repair_attempts = 3
    for repair_attempt in range(max_repair_attempts + 1):
        final_validate = run_living_spec_tool(final_spec_dir, "validate")
        final_connectivity = run_living_spec_tool(final_spec_dir, "connectivity")
        (extraction_dir / "validation_result.txt").write_text(final_validate)
        (extraction_dir / "connectivity_result.txt").write_text(final_connectivity)

        validation_ok = "exit_code=0" in final_validate
        # Check connectivity: no isolated nodes, no cycles with acyclic types
        connectivity_ok = (
            "Isolated Nodes: 0" in final_connectivity
            or "Isolated" not in final_connectivity
        ) and "Cycle contains acyclic" not in final_connectivity

        if validation_ok and connectivity_ok:
            console.print(
                "[green]Spec passed validation and connectivity checks.[/green]"
            )
            break

        if repair_attempt >= max_repair_attempts:
            console.print(
                f"[red]Spec failed quality gate after {max_repair_attempts} repair attempts. "
                "Aborting - will not proceed to Phase 2 with invalid spec.[/red]"
            )
            raise RuntimeError(
                f"Phase 1 quality gate failed: "
                f"validation={'PASS' if validation_ok else 'FAIL'}, "
                f"connectivity={'PASS' if connectivity_ok else 'FAIL'}"
            )

        # Launch repair agent
        issues = []
        if not validation_ok:
            issues.append("VALIDATION ERRORS")
        if not connectivity_ok:
            issues.append("CONNECTIVITY ISSUES (isolated nodes or acyclic cycles)")
        console.print(
            f"  [yellow]Spec quality gate failed ({', '.join(issues)}). "
            f"Repair attempt {repair_attempt + 1}/{max_repair_attempts}...[/yellow]"
        )
        _ui_set_agent("Repair")

        # Save tool outputs for repair agent to read
        repair_validate_path = (
            extraction_dir / f"repair_{repair_attempt}_validation.txt"
        )
        repair_connectivity_path = (
            extraction_dir / f"repair_{repair_attempt}_connectivity.txt"
        )
        repair_validate_path.write_text(final_validate)
        repair_connectivity_path.write_text(final_connectivity)

        repair_system = f"""\
You are a Living-Spec Repair Agent. The spec at {final_spec_dir}/ has failed quality checks.
Your job is to fix ALL issues so validation passes and connectivity is clean.

Read the validation and connectivity output files to understand what's wrong, then
read and fix the spec XML files.

Common fixes:
- art:mapping with multiple art:artifact children: the schema requires art:coverage
  immediately after the FIRST art:artifact. If a spec node maps to multiple files,
  use multiple art:mapping elements (one per file), not multiple art:artifact in one mapping.
- src:cite missing 'source' attribute: src:cite requires source="SOURCE_ID" referencing
  a src:source declaration. If no src:source exists, either add one or remove the src:cite.
- Isolated nodes: add rel:relation elements connecting isolated nodes to related concepts.
- Acyclic cycles (depends-on/precedes): reverse one direction or change type to 'references'.
- After fixing, run: uv run --script {ws_ls_script} validate {final_spec_dir}/
  and: uv run --script {ws_ls_script} connectivity {final_spec_dir}/
  to verify your fixes worked.

Read the schemas in {final_spec_dir.parent / "schemas"}/ if you need to understand
the expected XML structure.
"""
        repair_prompt = (
            f"Fix the spec at {final_spec_dir}/. Read these files for the issues:\n"
            f"- Validation output: {repair_validate_path}\n"
            f"- Connectivity output: {repair_connectivity_path}\n"
            "Fix ALL issues, then run validate and connectivity to verify."
        )
        stream = sdk.query(
            prompt=repair_prompt,
            options=sdk.ClaudeAgentOptions(
                system_prompt=repair_system,
                cwd=str(final_spec_dir.parent),
                permission_mode="bypassPermissions",
                model=model,
                max_turns=30,
                max_budget_usd=max_budget,
            ),
        )
        repair_msgs, repair_result = await collect_messages(
            stream, sdk=sdk, agent_label="Repair"
        )
        save_messages(
            repair_msgs,
            extraction_dir / f"repair_{repair_attempt}_messages.json",
            sdk=sdk,
        )
        if repair_result:
            cost_tracker.record(
                "phase1_repair",
                repair_result.total_cost_usd,
                repair_result.duration_ms,
                repair_result.num_turns,
                usage=repair_result.usage,
            )
            console.print(f"  Repair cost: ${repair_result.total_cost_usd or 0:.2f}")

    # Cache the validated result
    cache_dir.mkdir(parents=True, exist_ok=True)
    cache_spec = cache_dir / "final_spec"
    if cache_spec.exists():
        shutil.rmtree(cache_spec)
    shutil.copytree(final_spec_dir, cache_spec)
    console.print(f"[green]Phase 1 complete. Spec cached at {cache_spec}[/green]")
    if _ui:
        _ui.advance(1, "Final validation")
        _ui.complete_phase(1)

    return final_spec_dir


# ---------------------------------------------------------------------------
# Phase 2: Parallel A/B feature development
# ---------------------------------------------------------------------------


async def phase2_develop(
    project_dir: Path,
    spec_dir: Path,
    feature_task: str,
    output_dir: Path,
    model: str | None = None,
    with_spec_model: str | None = None,
    without_spec_model: str | None = None,
    max_budget: float | None = None,
    cost_tracker: CostTracker | None = None,
    sdk=_default_sdk,
) -> dict:
    """Run parallel A/B development sessions.

    with_spec_model/without_spec_model override model for their respective sessions.
    """
    with_spec_model = with_spec_model or model
    without_spec_model = without_spec_model or model
    cost_tracker = cost_tracker or CostTracker()
    dev_dir = output_dir / "phase2_development"
    dev_dir.mkdir(parents=True, exist_ok=True)

    # Clone project twice for isolation
    with_spec_dir = dev_dir / "dev-with-spec"
    without_spec_dir = dev_dir / "dev-without-spec"

    if with_spec_dir.exists():
        shutil.rmtree(with_spec_dir)
    if without_spec_dir.exists():
        shutil.rmtree(without_spec_dir)

    shutil.copytree(project_dir, with_spec_dir)
    shutil.copytree(project_dir, without_spec_dir)

    # Copy spec into with-spec session
    spec_dest = with_spec_dir / "specs"
    shutil.copytree(spec_dir, spec_dest)

    # Copy living-spec.py and schemas for the agent to use
    ls_dir = Path(__file__).parent
    shutil.copy2(ls_dir / "living-spec.py", with_spec_dir / "living-spec.py")
    if (ls_dir / "schemas").exists():
        shutil.copytree(ls_dir / "schemas", with_spec_dir / "schemas")

    # Both sessions get the same instruction to read project docs (fair comparison)
    # Session A system prompt: spec-first, plan-first workflow
    with_spec_system = """\
You are an expert developer with access to a structured cognitive layers specification
of this project. You have full web access to look up documentation, tutorials, and references.

Read the project's README.md and CONTRIBUTING.md for context before starting.

You MUST follow this workflow:

1. First, READ the spec: browse specs/ files to understand the project's architecture,
   terminology, relations, and patterns. Use the terminology and relations to build your
   mental model.

2. Then, CREATE A PLAN in living-spec XML format. Write a `pln:plan` element with:
   - A title and overview
   - `pln:item` elements for each implementation step, each with:
     - A title and description
     - `pln:acceptance` with `pln:criterion` elements describing what must be true
     - `pln:witness` elements proving criteria are met (type='command' for shell commands
       like test runners, type='script' for verification scripts, type='manual' for review)
     - `pln:item-status` (start as 'pending')
   - Use `rel:relation type='depends-on'` between items for ordering
   Save the plan as `specs/plan-feature.xml` and add it to `specs/index.xml`

3. Then, IMPLEMENT the feature following your plan step by step. After completing each step,
   update its `pln:item-status` to 'completed' and run any command witnesses to verify.

4. Finally, UPDATE the spec: add new terminology, prose sections, relations, and artifact
   mappings for ALL new code you wrote. Every new function, class, or module must have an
   artifact mapping. Run validation to verify completeness.
"""

    # Session B system prompt: standard developer with docs + web access
    without_spec_system = """\
You are an expert developer. You have full web access to look up official documentation,
tutorials, Stack Overflow, or any other online resources.

Read the project's README.md and CONTRIBUTING.md for context before starting.

You MUST follow this workflow:

1. First, READ the existing codebase to understand its architecture, patterns, and conventions.
   Explore key files: __init__.py, app.py, core modules, existing tests, config files.

2. Then, PLAN your implementation: decide which files to create/modify, what patterns to follow,
   and how to structure your tests.

3. Then, IMPLEMENT the feature following the project's established patterns.

4. Then, WRITE comprehensive tests: unit tests, integration tests, edge case tests.
   Install any test dependencies needed (e.g. `pip install -e '.[testing]'`).

5. Finally, RUN the test suite to verify everything passes:
   - Run: `python -m pytest --tb=short -q`
   - If tests fail, fix the issues and re-run until all tests pass
   - Do NOT stop until your tests actually pass
"""

    # Run both sessions in parallel using ClaudeSDKClient for persistent processes.
    # Session A's client is kept alive for the coverage nudge loop (saves ~12s per nudge).
    console.print("\n[bold blue]Phase 2 - Parallel A/B Development[/bold blue]")
    _ui_status("Sessions A + B running in parallel")

    session_a_options = sdk.ClaudeAgentOptions(
        system_prompt=with_spec_system,
        cwd=str(with_spec_dir),
        permission_mode="bypassPermissions",
        model=with_spec_model,
        max_turns=100,
        max_budget_usd=max_budget,
    )
    session_a_client = sdk.ClaudeSDKClient(options=session_a_options)
    await session_a_client.connect()

    async def run_session_a():
        console.print(
            f"  [cyan]Session A (with-spec, model={with_spec_model or 'default'}): starting...[/cyan]"
        )
        await session_a_client.query(feature_task)
        msgs, result = await collect_messages(
            session_a_client.receive_response(), sdk=sdk, agent_label="Session A"
        )
        save_messages(msgs, dev_dir / "with_spec_messages.json", sdk=sdk)
        if result:
            cost_tracker.record(
                "phase2_with_spec",
                result.total_cost_usd,
                result.duration_ms,
                result.num_turns,
                usage=result.usage,
            )
            console.print(f"  Session A cost: ${result.total_cost_usd or 0:.2f}")
        return msgs, result

    async def run_session_b():
        console.print(
            f"  [cyan]Session B (without-spec, model={without_spec_model or 'default'}): starting...[/cyan]"
        )
        stream = sdk.query(
            prompt=feature_task,
            options=sdk.ClaudeAgentOptions(
                system_prompt=without_spec_system,
                cwd=str(without_spec_dir),
                permission_mode="bypassPermissions",
                model=without_spec_model,
                max_turns=100,
                max_budget_usd=max_budget,
            ),
        )
        msgs, result = await collect_messages(stream, sdk=sdk, agent_label="Session B")
        save_messages(msgs, dev_dir / "without_spec_messages.json", sdk=sdk)
        if result:
            cost_tracker.record(
                "phase2_without_spec",
                result.total_cost_usd,
                result.duration_ms,
                result.num_turns,
                usage=result.usage,
            )
            console.print(f"  Session B cost: ${result.total_cost_usd or 0:.2f}")
        return msgs, result

    (a_msgs, a_result), (b_msgs, b_result) = await asyncio.gather(
        run_session_a(), run_session_b()
    )
    _ui_advance(2, "Sessions A + B complete")

    # Coverage nudge loop - reuses Session A's persistent client (no subprocess restart)
    console.print("  [cyan]Running coverage nudge loop for Session A...[/cyan]")
    coverage_complete = False
    for nudge_round in range(3):
        coverage_output = run_living_spec_tool(spec_dest, "artifact", "--coverage")
        (dev_dir / f"coverage_round_{nudge_round}.txt").write_text(coverage_output)

        if "exit_code=0" in coverage_output or nudge_round == 2:
            coverage_complete = True
            break

        # Nudge via the same persistent client - saves ~12s subprocess startup per nudge
        console.print(
            f"  [yellow]Coverage incomplete (nudge {nudge_round + 1}/3), retry...[/yellow]"
        )
        nudge_prompt = (
            f"Your artifact coverage is incomplete. The coverage report:\n{coverage_output}\n\n"
            "Add artifact mappings for all uncovered code. Run validation when done."
        )
        await session_a_client.query(nudge_prompt)
        nudge_msgs, nudge_result = await collect_messages(
            session_a_client.receive_response(), sdk=sdk, agent_label="Nudge A"
        )
        if nudge_result:
            cost_tracker.record(
                "phase2_nudge",
                nudge_result.total_cost_usd,
                nudge_result.duration_ms,
                nudge_result.num_turns,
                usage=nudge_result.usage,
            )

    await session_a_client.disconnect()
    _ui_advance(2, "Coverage nudge done")

    # Create git diffs
    with_diff = create_git_diff(with_spec_dir)
    without_diff = create_git_diff(without_spec_dir)
    (dev_dir / "with_spec_diff.patch").write_text(with_diff)
    (dev_dir / "without_spec_diff.patch").write_text(without_diff)
    _ui_advance(2, "Diffs captured")
    if _ui:
        _ui.complete_phase(2)

    return {
        "with_spec_dir": str(with_spec_dir),
        "without_spec_dir": str(without_spec_dir),
        "with_diff": with_diff,
        "without_diff": without_diff,
        "coverage_complete": coverage_complete,
    }


# ---------------------------------------------------------------------------
# Phase 3: Comparative analysis
# ---------------------------------------------------------------------------


async def collect_objective_metrics(
    workdir: Path,
    diff_text: str,
    model: str | None = None,
    max_budget: float | None = None,
    cost_tracker: CostTracker | None = None,
    sdk=_default_sdk,
) -> dict:
    """Collect objective, tool-based metrics. Uses SDK agent as fallback for errors."""
    lang = detect_project_language(workdir)
    metrics = {
        "tests_passed": 0,
        "tests_failed": 0,
        "test_exit_code": -1,
        "lint_warnings": 0,
        "lint_errors": 0,
    }

    # Try running tests directly first; on failure, use SDK agent to resolve
    test_ok = False
    try:
        if lang == "python":
            # Ignore spec/schema dirs that may exist in with-spec sessions
            pytest_cmd = [
                "python",
                "-m",
                "pytest",
                "--tb=no",
                "-q",
                "--ignore=specs",
                "--ignore=schemas",
                "--ignore=living-spec.py",
            ]
            baseline_result = subprocess.run(
                pytest_cmd,
                cwd=workdir,
                capture_output=True,
                text=True,
                timeout=300,
            )
            metrics["test_output"] = baseline_result.stdout
            match = re.search(r"(\d+) passed", baseline_result.stdout)
            metrics["tests_passed"] = int(match.group(1)) if match else 0
            match = re.search(r"(\d+) failed", baseline_result.stdout)
            metrics["tests_failed"] = int(match.group(1)) if match else 0
            metrics["test_exit_code"] = baseline_result.returncode
            # exit code 4 = no tests collected, 5 = no tests ran
            # Only consider test run successful if tests were actually found
            test_ok = baseline_result.returncode not in (4, 5)
    except (subprocess.TimeoutExpired, FileNotFoundError, OSError) as e:
        console.print(f"  [yellow]Direct test run failed: {e}[/yellow]")

    # Fallback: use SDK agent to install deps and run tests
    if not test_ok and sdk is not None:
        console.print("  [cyan]Using SDK agent to resolve test environment...[/cyan]")
        try:
            agent_prompt = (
                "Install this project's test dependencies and run its test suite. "
                "Steps:\n"
                "1. Check for pyproject.toml, setup.py, or requirements files\n"
                "2. Install test dependencies (pip install -e '.[testing]' or similar)\n"
                "3. Run: python -m pytest --tb=no -q\n"
                "4. Report the EXACT pytest summary line (e.g. '15 passed, 2 failed')\n"
                "If tests cannot run at all, report 'NO_TESTS_RUN' and explain why."
            )
            stream = sdk.query(
                prompt=agent_prompt,
                options=sdk.ClaudeAgentOptions(
                    system_prompt="You are a CI environment setup agent. Install deps and run tests. Be concise.",
                    cwd=str(workdir),
                    permission_mode="bypassPermissions",
                    model=model,
                    max_turns=20,
                    max_budget_usd=max_budget or 2.0,
                ),
            )
            msgs, result = await collect_messages(stream, sdk=sdk)
            if cost_tracker and result:
                cost_tracker.record(
                    "phase3_test_setup",
                    result.total_cost_usd,
                    result.duration_ms,
                    result.num_turns,
                    usage=result.usage,
                )
            # Extract test results from agent output
            for m in msgs:
                if hasattr(m, "content"):
                    for block in m.content:
                        if hasattr(block, "text"):
                            text = block.text
                            match = re.search(r"(\d+) passed", text)
                            if match:
                                metrics["tests_passed"] = int(match.group(1))
                            match = re.search(r"(\d+) failed", text)
                            if match:
                                metrics["tests_failed"] = int(match.group(1))
                            if "passed" in text or "failed" in text:
                                metrics["test_exit_code"] = (
                                    0 if metrics["tests_failed"] == 0 else 1
                                )
                                metrics["test_output"] = text
        except Exception as e:
            console.print(f"  [yellow]SDK test agent also failed: {e}[/yellow]")

    # Linting
    try:
        if lang == "python":
            lint_result = subprocess.run(
                ["ruff", "check", "--output-format", "json", "."],
                cwd=workdir,
                capture_output=True,
                text=True,
                timeout=60,
            )
            try:
                lint_data = (
                    json.loads(lint_result.stdout) if lint_result.stdout.strip() else []
                )
                metrics["lint_warnings"] = len(lint_data)
                metrics["lint_errors"] = sum(
                    1 for i in lint_data if i.get("severity", "") == "error"
                )
            except json.JSONDecodeError:
                pass
    except (subprocess.TimeoutExpired, FileNotFoundError, OSError) as e:
        console.print(f"  [yellow]Lint failed: {e}[/yellow]")

    # Code metrics from diff (language-agnostic)
    code_metrics = compute_code_metrics(diff_text)
    metrics.update(code_metrics)

    return metrics


async def phase3_analyze(
    dev_result: dict,
    output_dir: Path,
    model: str | None = None,
    max_budget: float | None = None,
    cost_tracker: CostTracker | None = None,
    sdk=_default_sdk,
) -> dict:
    """Phase 3: Comparative analysis of A/B results."""
    cost_tracker = cost_tracker or CostTracker()
    analysis_dir = output_dir / "phase3_analysis"
    analysis_dir.mkdir(parents=True, exist_ok=True)

    with_dir = Path(dev_result["with_spec_dir"])
    without_dir = Path(dev_result["without_spec_dir"])
    with_diff = dev_result["with_diff"]
    without_diff = dev_result["without_diff"]

    # Phase 3A: Objective metrics (uses SDK agent as fallback for test env issues)
    console.print("\n[bold blue]Phase 3A - Objective Metrics[/bold blue]")
    _ui_status("Collecting objective metrics")

    with_metrics = await collect_objective_metrics(
        with_dir,
        with_diff,
        model=model,
        max_budget=max_budget,
        cost_tracker=cost_tracker,
        sdk=sdk,
    )
    _ui_advance(3, "Objective: with-spec done")
    without_metrics = await collect_objective_metrics(
        without_dir,
        without_diff,
        model=model,
        max_budget=max_budget,
        cost_tracker=cost_tracker,
        sdk=sdk,
    )
    _ui_advance(3, "Objective: without-spec done")

    # Compute deltas
    objective_results = {
        "with_spec": with_metrics,
        "without_spec": without_metrics,
        "delta": {
            "tests_passed_delta": with_metrics.get("tests_passed", 0)
            - without_metrics.get("tests_passed", 0),
            "lint_warnings_delta": without_metrics.get("lint_warnings", 0)
            - with_metrics.get("lint_warnings", 0),
            "lines_added_delta": with_metrics.get("lines_added", 0)
            - without_metrics.get("lines_added", 0),
            "test_functions_delta": with_metrics.get("test_functions", 0)
            - without_metrics.get("test_functions", 0),
        },
    }
    (analysis_dir / "objective_metrics.json").write_text(
        json.dumps(objective_results, indent=2, default=str)
    )

    # Phase 3B: LLM-judged analysis (parallel)
    console.print("\n[bold blue]Phase 3B - LLM-Judged Analysis[/bold blue]")

    analysis_schema = {
        "type": "object",
        "properties": {
            "score_with": {"type": "number"},
            "score_without": {"type": "number"},
            "findings_with": {"type": "array", "items": {"type": "string"}},
            "findings_without": {"type": "array", "items": {"type": "string"}},
            "verdict": {"type": "string"},
        },
    }

    plan_quality_schema = {
        "type": "object",
        "properties": {
            "plan_valid_xml": {"type": "boolean"},
            "items_total": {"type": "number"},
            "items_completed": {"type": "number"},
            "witnesses_total": {"type": "number"},
            "witnesses_passed": {"type": "number"},
            "plan_followed": {"type": "boolean"},
            "score": {"type": "number"},
            "findings": {"type": "array", "items": {"type": "string"}},
        },
    }

    def _extract_scores_from_text(text: str) -> dict:
        """Fallback: extract scores from prose when structured output fails."""
        scores = {}
        # Look for "With: N" / "Without: N" patterns (e.g. "With: 7 | Without: 8")
        with_match = re.search(
            r"(?:with[- ]spec|session a|with)\s*[:=]\s*(\d+(?:\.\d+)?)", text, re.I
        )
        without_match = re.search(
            r"(?:without[- ]spec|session b|without)\s*[:=]\s*(\d+(?:\.\d+)?)",
            text,
            re.I,
        )
        if with_match:
            scores["score_with"] = float(with_match.group(1))
        if without_match:
            scores["score_without"] = float(without_match.group(1))
        # Look for "Score: N/10" patterns
        if "score_with" not in scores:
            all_scores = re.findall(r"(\d+(?:\.\d+)?)\s*/\s*10", text)
            if len(all_scores) >= 2:
                scores["score_with"] = float(all_scores[0])
                scores["score_without"] = float(all_scores[1])
        # Look for "N | N" pattern after dimension keywords
        if "score_with" not in scores:
            pair = re.search(
                r"(\d+(?:\.\d+)?)\s*\|\s*(?:without|session b)[^0-9]*(\d+(?:\.\d+)?)",
                text,
                re.I,
            )
            if pair:
                scores["score_with"] = float(pair.group(1))
                scores["score_without"] = float(pair.group(2))
        return scores

    async def analyze_dimension(
        dimension: str, system_prompt: str, schema: dict
    ) -> dict:
        stream = sdk.query(
            prompt=(
                f"Analyze the {dimension} of both implementations.\n\n"
                f"## With-Spec Diff:\n```\n{with_diff[:20000]}\n```\n\n"
                f"## Without-Spec Diff:\n```\n{without_diff[:20000]}\n```\n\n"
                f"## Objective Metrics:\n{json.dumps(objective_results, indent=2, default=str)}\n\n"
                "All the information you need is above. Do NOT use any tools - just analyze "
                "the diffs and metrics provided. Respond with ONLY valid JSON matching the "
                "required schema."
            ),
            options=sdk.ClaudeAgentOptions(
                system_prompt=system_prompt,
                model=model,
                max_turns=1,
                max_budget_usd=max_budget,
                output_format={"type": "json_schema", "schema": schema},
                disallowed_tools=["Read", "Glob", "Grep", "Bash", "Write", "Edit"],
            ),
        )
        msgs, result = await collect_messages(
            stream, sdk=sdk, agent_label=f"Analyst:{dimension}"
        )
        if result:
            cost_tracker.record(
                f"phase3_{dimension}",
                result.total_cost_usd,
                result.duration_ms,
                result.num_turns,
                usage=result.usage,
            )
        # Extract structured output
        analysis_data = {"dimension": dimension}

        # Check ResultMessage.structured_output first (used when output_format is set)
        if result and hasattr(result, "structured_output") and result.structured_output:
            try:
                if isinstance(result.structured_output, dict):
                    analysis_data.update(result.structured_output)
                elif isinstance(result.structured_output, str):
                    analysis_data.update(json.loads(result.structured_output))
            except (json.JSONDecodeError, TypeError, ValueError):
                pass

        # Fall back to text blocks if structured_output didn't provide scores
        if "score_with" not in analysis_data and "score" not in analysis_data:
            for m in msgs:
                if hasattr(m, "content"):
                    for block in m.content:
                        if hasattr(block, "text"):
                            try:
                                analysis_data.update(json.loads(block.text))
                            except (json.JSONDecodeError, TypeError):
                                analysis_data["raw_text"] = block.text
                                # Fallback: try to extract scores from prose
                                extracted = _extract_scores_from_text(block.text)
                                if extracted:
                                    analysis_data.update(extracted)
        return analysis_data

    # Launch all analysis dimensions in parallel
    quality_task = analyze_dimension(
        "code_quality",
        "You are a code quality reviewer. Score naming, modularity, error handling, "
        "and project idiom adherence on a 1-10 scale for both implementations.",
        analysis_schema,
    )

    security_task = analyze_dimension(
        "security",
        "You are a security reviewer. Score input validation, injection risks, "
        "bypass vectors, and error leakage on a 1-10 scale for both implementations.",
        analysis_schema,
    )

    architecture_task = analyze_dimension(
        "architecture",
        "You are an architecture reviewer. Score adherence to the project's established "
        "patterns, extension conventions, and backward compatibility on a 1-10 scale.",
        analysis_schema,
    )

    plan_quality_task = analyze_dimension(
        "plan_quality",
        "You are a plan quality reviewer. Evaluate the living-spec plan (pln:plan) created "
        "by Session A. Was the plan XML valid? Did the agent follow the plan sequentially? "
        "Were witness commands actually run? Were item statuses updated? "
        "How many criteria had witnesses vs. being unwitnessed?",
        plan_quality_schema,
    )

    quality_result, security_result, arch_result, plan_result = await asyncio.gather(
        quality_task, security_task, architecture_task, plan_quality_task
    )
    _ui_advance(3, "LLM analysis complete")

    # Save all results
    for name, data in [
        ("code_quality", quality_result),
        ("security", security_result),
        ("architecture", arch_result),
        ("plan_quality", plan_result),
    ]:
        (analysis_dir / f"{name}.json").write_text(
            json.dumps(data, indent=2, default=str)
        )

    # Spec alignment check for Session A
    spec_alignment = {
        "coverage_complete": dev_result.get("coverage_complete", False),
    }
    (analysis_dir / "spec_alignment.json").write_text(
        json.dumps(spec_alignment, indent=2)
    )
    _ui_advance(3, "Results saved")
    if _ui:
        _ui.complete_phase(3)

    return {
        "objective": objective_results,
        "code_quality": quality_result,
        "security": security_result,
        "architecture": arch_result,
        "plan_quality": plan_result,
        "spec_alignment": spec_alignment,
    }


# ---------------------------------------------------------------------------
# Comprehension Test (elective phase)
# ---------------------------------------------------------------------------


async def phase_comprehension(
    project_dir: Path,
    spec_dir: Path,
    output_dir: Path,
    model: str | None = None,
    max_budget: float | None = None,
    cost_tracker: CostTracker | None = None,
    num_questions: int = 12,
    sdk=_default_sdk,
) -> dict:
    """Run comprehension test: generate questions, ask both sessions, judge answers.

    Questions are generated from the spec but designed to test understanding,
    not simple lookup. The spec serves as the answer key for judging.
    """
    console.print("\n[bold blue]Comprehension Test[/bold blue]")
    comp_dir = output_dir / "comprehension"
    comp_dir.mkdir(parents=True, exist_ok=True)

    if not cost_tracker:
        cost_tracker = CostTracker()

    # Step 1: Read the spec for question generation
    spec_content = ""
    for xml_file in sorted(spec_dir.glob("*.xml")):
        spec_content += f"\n\n--- {xml_file.name} ---\n{xml_file.read_text()}"

    # Step 2: Generate questions from the spec
    console.print("  [cyan]Generating comprehension questions...[/cyan]")
    _ui_set_agent("QuestionGen")

    question_schema = {
        "type": "object",
        "properties": {
            "questions": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "id": {"type": "string"},
                        "category": {
                            "type": "string",
                            "enum": [
                                "architecture",
                                "dependencies",
                                "impact_analysis",
                                "design_rationale",
                                "cross_cutting",
                            ],
                        },
                        "question": {"type": "string"},
                        "reference_answer": {"type": "string"},
                        "grading_rubric": {"type": "string"},
                    },
                },
            }
        },
    }

    gen_prompt = f"""\
You have a structured specification of a software project. Generate exactly \
{num_questions} comprehension questions that test deep understanding of the \
system's architecture, design, and behavior.

CRITICAL RULES for question design:
- Questions MUST require REASONING about the system, not simple fact lookup
- Do NOT ask "what is X?" or "where is X defined?" - those are trivial lookups
- Do NOT ask questions whose answers are literally stated in a single term \
definition or description
- DO ask questions that require connecting multiple concepts, understanding \
implications, or predicting consequences

Good question types:
1. DEPENDENCY REASONING: "If component X were removed or significantly changed, \
which other parts of the system would be affected and why?"
2. DESIGN RATIONALE: "Why does the system use pattern X instead of alternative Y \
for this particular concern? What tradeoffs does this create?"
3. CROSS-CUTTING: "How do concerns A and B interact? What happens when both are \
active simultaneously?"
4. IMPACT ANALYSIS: "A developer wants to add feature X. Which existing components \
would they need to modify, and what constraints would they face?"
5. ARCHITECTURE: "What would happen if the system needed to handle 100x the current \
scale? Which components would become bottlenecks first?"

For each question provide:
- A reference answer (the ideal answer, drawn from the spec but requiring synthesis)
- A grading rubric (what constitutes partial vs full credit)

The spec content follows:
{spec_content[:40000]}"""

    gen_stream = sdk.query(
        prompt=gen_prompt,
        options=sdk.ClaudeAgentOptions(
            system_prompt=(
                "You are a technical assessment designer. Generate questions that "
                "test genuine understanding of software systems, not memorization. "
                "Every question should require connecting at least two concepts or "
                "reasoning about consequences. Output valid JSON only."
            ),
            model=model,
            max_turns=1,
            max_budget_usd=max_budget or 2.0,
            output_format={"type": "json_schema", "schema": question_schema},
            disallowed_tools=["Read", "Glob", "Grep", "Bash", "Write", "Edit"],
        ),
    )
    gen_msgs, gen_result = await collect_messages(
        gen_stream, sdk=sdk, agent_label="QuestionGen"
    )
    if gen_result and cost_tracker:
        cost_tracker.record(
            "comprehension_gen",
            gen_result.total_cost_usd,
            gen_result.duration_ms,
            gen_result.num_turns,
            usage=getattr(gen_result, "usage", None),
        )

    # Extract questions from structured output or text
    questions = []
    if (
        gen_result
        and hasattr(gen_result, "structured_output")
        and gen_result.structured_output
    ):
        so = gen_result.structured_output
        if isinstance(so, dict):
            questions = so.get("questions", [])
        elif isinstance(so, str):
            try:
                questions = json.loads(so).get("questions", [])
            except (json.JSONDecodeError, TypeError):
                pass
    if not questions:
        for m in gen_msgs:
            if hasattr(m, "content"):
                for block in m.content:
                    if hasattr(block, "text"):
                        try:
                            data = json.loads(block.text)
                            questions = data.get("questions", [])
                        except (json.JSONDecodeError, TypeError):
                            pass

    if not questions:
        console.print("  [red]Failed to generate questions[/red]")
        return {"error": "question_generation_failed"}

    console.print(f"  Generated {len(questions)} questions")
    (comp_dir / "questions.json").write_text(json.dumps(questions, indent=2))

    # Step 3: Ask questions to both sessions in parallel
    # Session A (with-spec): has access to the spec directory
    # Session B (without-spec): has access to the project only
    # Both have read access to the codebase (no web, no tools beyond Read/Grep/Glob)

    question_text = "\n\n".join(
        f"Q{i + 1} [{q.get('category', 'general')}]: {q['question']}"
        for i, q in enumerate(questions)
    )

    answerer_tools = ["Read", "Glob", "Grep"]

    async def ask_session(session_name: str, cwd: str, system_prompt: str) -> dict:
        """Ask all questions to a session, measure time."""
        _ui_set_agent(f"Comp:{session_name}")
        start_time = time.time()
        stream = sdk.query(
            prompt=(
                "Answer each question below about this software project. "
                "You may read source code files to inform your answers. "
                "For each question, provide a thorough answer that demonstrates "
                "understanding of how the system works and why.\n\n"
                f"{question_text}"
            ),
            options=sdk.ClaudeAgentOptions(
                system_prompt=system_prompt,
                cwd=cwd,
                permission_mode="bypassPermissions",
                model=model,
                max_turns=30,
                max_budget_usd=max_budget or 5.0,
                allowed_tools=answerer_tools,
            ),
        )
        msgs, result = await collect_messages(
            stream, sdk=sdk, agent_label=f"Comp:{session_name}"
        )
        elapsed = time.time() - start_time

        if result and cost_tracker:
            cost_tracker.record(
                f"comprehension_{session_name}",
                result.total_cost_usd,
                result.duration_ms,
                result.num_turns,
                usage=getattr(result, "usage", None),
            )

        # Extract answer text
        answer_text = ""
        for m in msgs:
            if hasattr(m, "content"):
                for block in m.content:
                    if hasattr(block, "text"):
                        answer_text += block.text + "\n"

        return {
            "session": session_name,
            "answers_text": answer_text,
            "elapsed_seconds": round(elapsed, 1),
            "turns": result.num_turns if result else 0,
            "cost_usd": result.total_cost_usd if result else 0,
        }

    with_spec_system = (
        "You are a software architect answering questions about this project. "
        "You have access to a structured specification in specs/ that describes "
        "the project's architecture, terminology, and component relationships. "
        "Read the spec files first, then use the codebase to supplement. "
        "Demonstrate deep understanding in your answers."
    )
    without_spec_system = (
        "You are a software architect answering questions about this project. "
        "Read the codebase to understand the architecture, patterns, and design. "
        "Demonstrate deep understanding in your answers."
    )

    # Prepare workdirs: with-spec gets a copy with the spec, without doesn't
    comp_with_dir = comp_dir / "with-spec"
    comp_without_dir = comp_dir / "without-spec"
    if not comp_with_dir.exists():
        shutil.copytree(project_dir, comp_with_dir, dirs_exist_ok=True)
        spec_dest = comp_with_dir / "specs"
        spec_dest.mkdir(parents=True, exist_ok=True)
        shutil.copytree(spec_dir, spec_dest / spec_dir.name, dirs_exist_ok=True)
    if not comp_without_dir.exists():
        shutil.copytree(project_dir, comp_without_dir, dirs_exist_ok=True)

    console.print("  [cyan]Asking questions to both sessions in parallel...[/cyan]")
    with_result, without_result = await asyncio.gather(
        ask_session("with_spec", str(comp_with_dir), with_spec_system),
        ask_session("without_spec", str(comp_without_dir), without_spec_system),
    )

    (comp_dir / "with_spec_answers.json").write_text(
        json.dumps(with_result, indent=2, default=str)
    )
    (comp_dir / "without_spec_answers.json").write_text(
        json.dumps(without_result, indent=2, default=str)
    )
    console.print(
        f"  With-spec: {with_result['elapsed_seconds']}s, "
        f"${with_result['cost_usd'] or 0:.2f}"
    )
    console.print(
        f"  Without-spec: {without_result['elapsed_seconds']}s, "
        f"${without_result['cost_usd'] or 0:.2f}"
    )

    # Step 4: Judge answers
    console.print("  [cyan]Judging answers...[/cyan]")
    _ui_set_agent("Judge")

    judge_schema = {
        "type": "object",
        "properties": {
            "scores": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "question_id": {"type": "string"},
                        "with_spec_score": {"type": "number"},
                        "without_spec_score": {"type": "number"},
                        "with_spec_reasoning": {"type": "string"},
                        "without_spec_reasoning": {"type": "string"},
                    },
                },
            },
            "summary": {
                "type": "object",
                "properties": {
                    "with_spec_mean": {"type": "number"},
                    "without_spec_mean": {"type": "number"},
                    "verdict": {"type": "string"},
                },
            },
        },
    }

    judge_prompt = f"""\
You are judging answers to comprehension questions about a software project.

Score each answer on a 0-3 scale:
  0 = Wrong or irrelevant
  1 = Partially correct but missing key aspects
  2 = Correct but surface-level, missing nuance or connections
  3 = Excellent: correct, demonstrates deep understanding, connects concepts

Use the reference answers and rubrics as your grading guide. Be strict and \
consistent - apply the SAME standard to both sessions.

QUESTIONS AND RUBRICS:
{json.dumps(questions, indent=2)}

SESSION A (with-spec) ANSWERS:
{with_result["answers_text"][:30000]}

SESSION B (without-spec) ANSWERS:
{without_result["answers_text"][:30000]}

Score every question for both sessions. Provide reasoning for each score."""

    judge_stream = sdk.query(
        prompt=judge_prompt,
        options=sdk.ClaudeAgentOptions(
            system_prompt=(
                "You are an impartial technical judge. Score answers strictly "
                "against the rubric. Do not favor either session. "
                "Output valid JSON only."
            ),
            model=model,
            max_turns=1,
            max_budget_usd=max_budget or 3.0,
            output_format={"type": "json_schema", "schema": judge_schema},
            disallowed_tools=["Read", "Glob", "Grep", "Bash", "Write", "Edit"],
        ),
    )
    judge_msgs, judge_result = await collect_messages(
        judge_stream, sdk=sdk, agent_label="Judge"
    )
    if judge_result and cost_tracker:
        cost_tracker.record(
            "comprehension_judge",
            judge_result.total_cost_usd,
            judge_result.duration_ms,
            judge_result.num_turns,
            usage=getattr(judge_result, "usage", None),
        )

    # Extract judge results
    judge_data: dict = {}
    if (
        judge_result
        and hasattr(judge_result, "structured_output")
        and judge_result.structured_output
    ):
        so = judge_result.structured_output
        if isinstance(so, dict):
            judge_data = so
        elif isinstance(so, str):
            try:
                judge_data = json.loads(so)
            except (json.JSONDecodeError, TypeError):
                pass
    if not judge_data:
        for m in judge_msgs:
            if hasattr(m, "content"):
                for block in m.content:
                    if hasattr(block, "text"):
                        try:
                            judge_data = json.loads(block.text)
                        except (json.JSONDecodeError, TypeError):
                            judge_data["raw_text"] = block.text

    (comp_dir / "judge_results.json").write_text(
        json.dumps(judge_data, indent=2, default=str)
    )

    # Compute summary statistics
    scores = judge_data.get("scores", [])
    with_scores = [
        s.get("with_spec_score", 0) for s in scores if "with_spec_score" in s
    ]
    without_scores = [
        s.get("without_spec_score", 0) for s in scores if "without_spec_score" in s
    ]

    comp_summary = {
        "num_questions": len(questions),
        "with_spec": {
            "mean_score": round(statistics.mean(with_scores), 2) if with_scores else 0,
            "scores": with_scores,
            "elapsed_seconds": with_result["elapsed_seconds"],
            "cost_usd": with_result["cost_usd"],
            "turns": with_result["turns"],
        },
        "without_spec": {
            "mean_score": round(statistics.mean(without_scores), 2)
            if without_scores
            else 0,
            "scores": without_scores,
            "elapsed_seconds": without_result["elapsed_seconds"],
            "cost_usd": without_result["cost_usd"],
            "turns": without_result["turns"],
        },
        "delta_mean": round(
            (statistics.mean(with_scores) if with_scores else 0)
            - (statistics.mean(without_scores) if without_scores else 0),
            2,
        ),
        "judge_verdict": judge_data.get("summary", {}).get("verdict", ""),
    }

    (comp_dir / "comprehension_summary.json").write_text(
        json.dumps(comp_summary, indent=2)
    )

    console.print(
        f"  [green]With-spec mean: {comp_summary['with_spec']['mean_score']}/3.0[/green]"
    )
    console.print(
        f"  [green]Without-spec mean: {comp_summary['without_spec']['mean_score']}/3.0[/green]"
    )
    console.print(f"  [green]Delta: {comp_summary['delta_mean']:+.2f}[/green]")

    return comp_summary


def generate_comparative_report(
    all_run_results: list[dict],
    output_dir: Path,
    cost_tracker: CostTracker,
    comprehension_result: dict | None = None,
):
    """Generate aggregate comparative report across all runs."""
    report_lines = ["# Cognitive Layers A/B Test Report\n"]
    report_lines.append(f"Generated: {datetime.now().isoformat()}\n")
    report_lines.append(f"Total runs: {len(all_run_results)}\n")

    # Aggregate objective metrics
    report_lines.append("\n## Objective Metrics\n")
    if all_run_results:
        metric_keys = [
            "tests_passed",
            "lint_warnings",
            "lines_added",
            "lines_removed",
            "files_changed",
            "test_functions",
        ]
        for key in metric_keys:
            with_vals = [
                r["objective"]["with_spec"].get(key, 0) for r in all_run_results
            ]
            without_vals = [
                r["objective"]["without_spec"].get(key, 0) for r in all_run_results
            ]
            with_mean = statistics.mean(with_vals) if with_vals else 0
            without_mean = statistics.mean(without_vals) if without_vals else 0
            with_stddev = statistics.stdev(with_vals) if len(with_vals) > 1 else 0
            without_stddev = (
                statistics.stdev(without_vals) if len(without_vals) > 1 else 0
            )
            report_lines.append(
                f"| {key} | with: {with_mean:.1f} (+-{with_stddev:.1f}) "
                f"| without: {without_mean:.1f} (+-{without_stddev:.1f}) |\n"
            )

    # Aggregate subjective scores
    report_lines.append("\n## Subjective Scores (LLM-judged)\n")
    for dim in ["code_quality", "security", "architecture"]:
        with_scores = [r.get(dim, {}).get("score_with", 0) for r in all_run_results]
        without_scores = [
            r.get(dim, {}).get("score_without", 0) for r in all_run_results
        ]
        if with_scores:
            w_mean = statistics.mean(with_scores)
            wo_mean = statistics.mean(without_scores)
            w_std = statistics.stdev(with_scores) if len(with_scores) > 1 else 0
            wo_std = statistics.stdev(without_scores) if len(without_scores) > 1 else 0
            report_lines.append(
                f"| {dim} | with: {w_mean:.1f} (+-{w_std:.1f}) "
                f"| without: {wo_mean:.1f} (+-{wo_std:.1f}) |\n"
            )

    # Plan quality summary
    report_lines.append("\n## Plan Quality (Session A only)\n")
    for r in all_run_results:
        pq = r.get("plan_quality", {})
        report_lines.append(
            f"- Plan valid XML: {pq.get('plan_valid_xml', 'N/A')}, "
            f"Items: {pq.get('items_completed', '?')}/{pq.get('items_total', '?')}, "
            f"Witnesses: {pq.get('witnesses_passed', '?')}/{pq.get('witnesses_total', '?')}, "
            f"Score: {pq.get('score', 'N/A')}\n"
        )

    # Comprehension test results
    if comprehension_result:
        report_lines.append("\n## Comprehension Test\n")
        w = comprehension_result.get("with_spec", {})
        wo = comprehension_result.get("without_spec", {})
        report_lines.append(
            f"- Questions: {comprehension_result.get('num_questions', 'N/A')}\n"
        )
        report_lines.append(
            f"- With-spec mean score: {w.get('mean_score', 'N/A')}/3.0 "
            f"({w.get('elapsed_seconds', 0):.0f}s, ${w.get('cost_usd', 0):.2f})\n"
        )
        report_lines.append(
            f"- Without-spec mean score: {wo.get('mean_score', 'N/A')}/3.0 "
            f"({wo.get('elapsed_seconds', 0):.0f}s, ${wo.get('cost_usd', 0):.2f})\n"
        )
        report_lines.append(
            f"- Delta: {comprehension_result.get('delta_mean', 0):+.2f}\n"
        )
        verdict = comprehension_result.get("judge_verdict", "")
        if verdict:
            report_lines.append(f"- Verdict: {verdict}\n")

    # Cost summary
    report_lines.append("\n## Cost Summary\n")
    report_lines.append(f"- Phase 1 (extraction): ${cost_tracker.phase1_cost:.2f}\n")
    report_lines.append(f"- Phase 2 (development): ${cost_tracker.phase2_cost:.2f}\n")
    report_lines.append(f"- Phase 3 (analysis): ${cost_tracker.phase3_cost:.2f}\n")
    report_lines.append(f"- **Total**: ${cost_tracker.total_cost:.2f}\n")

    report_text = "".join(report_lines)
    (output_dir / "phase3_analysis" / "aggregate_report.md").write_text(report_text)
    console.print(
        f"\n[green]Report saved to {output_dir / 'phase3_analysis' / 'aggregate_report.md'}[/green]"
    )
    return report_text


# ---------------------------------------------------------------------------
# Summarize results
# ---------------------------------------------------------------------------


async def summarize_results(output_dir: Path, sdk=_default_sdk):
    """Load results from a completed run and produce an LLM-interpreted summary."""
    output_dir = Path(output_dir)
    if not output_dir.exists():
        console.print(f"[red]Output directory not found: {output_dir}[/red]")
        return

    # Collect all result data
    data_parts = []

    # Costs
    costs_path = output_dir / "costs.json"
    if costs_path.exists():
        data_parts.append(f"## Cost Breakdown\n```json\n{costs_path.read_text()}\n```")

    # Aggregate report (if exists)
    report_path = output_dir / "phase3_analysis" / "aggregate_report.md"
    if report_path.exists():
        data_parts.append(f"## Aggregate Report\n{report_path.read_text()}")

    # Per-run results
    run_dirs = sorted(output_dir.glob("run_*/"))
    for run_dir in run_dirs:
        run_parts = [f"## {run_dir.name}"]
        analysis_dir = run_dir / "phase3_analysis"
        if analysis_dir.exists():
            for result_file in sorted(analysis_dir.glob("*.json")):
                run_parts.append(
                    f"### {result_file.stem}\n```json\n{result_file.read_text()}\n```"
                )
        # Diff stats (just the stat summary, not full diff)
        dev_dir = run_dir / "phase2_development"
        for label in ["with_spec", "without_spec"]:
            diff_path = dev_dir / f"{label}_diff.patch"
            if diff_path.exists():
                diff_text = diff_path.read_text()
                # Extract just the stat section
                stat_end = diff_text.find("--- FULL DIFF ---")
                stat_summary = (
                    diff_text[:stat_end].strip() if stat_end > 0 else diff_text[:2000]
                )
                run_parts.append(f"### {label} diff stats\n```\n{stat_summary}\n```")
        data_parts.append("\n".join(run_parts))

    # Checkpoint for config info
    checkpoint_path = output_dir / "checkpoint.json"
    if checkpoint_path.exists():
        data_parts.append(f"## Checkpoint\n```json\n{checkpoint_path.read_text()}\n```")

    results_text = "\n\n---\n\n".join(data_parts)

    # Print raw summary
    console.print(f"\n[bold]Results from: {output_dir}[/bold]\n")
    console.print(results_text[:3000])
    if len(results_text) > 3000:
        console.print(
            f"\n[dim]... ({len(results_text)} chars total, sending to LLM for analysis)[/dim]"
        )

    # Send to LLM for interpretation
    if sdk is None:
        console.print("[yellow]No SDK available, showing raw data only[/yellow]")
        return

    system_prompt = """\
You are an experiment analyst reviewing A/B test results from the Cognitive Layers \
Testing Harness. This harness tests whether structured cognitive layers specifications \
(living-spec XML format) improve AI agent performance on software development tasks.

The experiment runs two parallel sessions:
- Session A (with-spec): Gets a structured specification of the project's architecture, \
  terminology, relationships, and code mappings. Follows a spec-first, plan-first workflow \
  with living-spec XML plans, acceptance criteria, and witnesses.
- Session B (without-spec): Gets only raw project documentation (README, CONTRIBUTING). \
  Standard developer workflow.

Both sessions implement the same feature on the same project codebase.

Your analysis MUST include:

1. **Verdict**: Declare a clear winner (or tie) with confidence level (high/medium/low). \
   Don't hedge unless the data genuinely shows a tie.

2. **Why the winner won**: What specific advantages did the spec provide (or fail to \
   provide)? Point to concrete metrics: test counts, lint warnings, code volume, \
   subjective scores.

3. **Dimension breakdown**: For each measured dimension (tests, lint, code quality, \
   security, architecture, plan quality), which session won and by how much? \
   Use a table.

4. **Surprising findings**: Anything that contradicts expectations? Did the spec \
   hurt in any dimension? Did the weaker model outperform?

5. **Cost-effectiveness**: Was the spec extraction cost (Phase 1) justified by \
   the development quality improvement? Calculate the cost premium and quality delta.

6. **Model gap analysis**: If the sessions used different models, did the spec \
   close the model capability gap? By how much?

7. **Bottom line**: One sentence recommendation. Should this team adopt cognitive \
   layers specs for their development workflow?

Be direct, opinionated, and quantitative. Reference specific numbers from the data. \
Do not summarize what the experiment is - the reader already knows. Go straight to findings."""

    console.print("\n[cyan]Analyzing results with LLM...[/cyan]")
    stream = sdk.query(
        prompt=(
            f"Analyze these A/B test results and provide your findings.\n\n"
            f"{results_text}"
        ),
        options=sdk.ClaudeAgentOptions(
            system_prompt=system_prompt,
            max_turns=1,
            disallowed_tools=["Read", "Glob", "Grep", "Bash", "Write", "Edit"],
        ),
    )

    analysis_text = ""
    async for msg in stream:
        if hasattr(msg, "content"):
            for block in msg.content:
                if hasattr(block, "text"):
                    analysis_text = block.text
                    console.print("\n[bold]Analysis[/bold]\n")
                    console.print(block.text)
        if isinstance(msg, sdk.ResultMessage):
            if msg.total_cost_usd:
                console.print(f"\n[dim]Summary cost: ${msg.total_cost_usd:.2f}[/dim]")

    # Save as publishable markdown
    if analysis_text:
        summary_path = output_dir / "summary.md"
        summary_path.write_text(analysis_text)
        console.print(f"\n[green]Saved to {summary_path}[/green]")


# ---------------------------------------------------------------------------
# Battery runner
# ---------------------------------------------------------------------------


async def run_battery(battery_path: Path, sdk=_default_sdk):
    """Run a battery of experiments from a meta-config."""
    if yaml is None:
        console.print("[red]pyyaml is required for battery mode[/red]")
        sys.exit(1)

    battery = yaml.safe_load(battery_path.read_text()) or {}
    battery_name = battery.get("name", "battery")
    experiments = battery.get("experiments", [])

    if not experiments:
        console.print("[red]No experiments defined in battery config[/red]")
        return

    timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
    battery_dir = Path("harness-results") / f"{timestamp}_{battery_name}"
    battery_dir.mkdir(parents=True, exist_ok=True)

    console.print("[bold]Cognitive Layers Battery Run[/bold]")
    console.print(f"Battery: {battery_name}")
    console.print(f"Experiments: {len(experiments)}")
    console.print(f"Output: {battery_dir}\n")

    script_path = str(Path(__file__))
    results = []

    for i, exp in enumerate(experiments, 1):
        config_file = exp.get("config", "")
        config_path = Path(config_file)
        # Resolve relative paths against battery file location
        if not config_path.is_absolute():
            config_path = battery_path.parent / config_path
        if not config_path.exists():
            console.print(f"[red]Config not found: {config_path}, skipping[/red]")
            results.append({"name": config_file, "status": "skipped"})
            continue

        exp_name = exp.get("name", config_path.stem)
        exp_output = battery_dir / exp_name

        console.print(f"[bold magenta]{'=' * 60}[/bold magenta]")
        console.print(
            f"[bold magenta]Experiment {i}/{len(experiments)}: {exp_name}[/bold magenta]"
        )

        # Run as subprocess for clean isolation
        cmd = [
            "uv",
            "run",
            "--script",
            script_path,
            str(config_path),
            "--output",
            str(exp_output),
        ]
        # Forward overrides from battery config
        if "runs" in exp:
            cmd.extend(["--runs", str(exp["runs"])])
        if "rounds" in exp:
            cmd.extend(["--rounds", str(exp["rounds"])])
        if exp.get("no_cache"):
            cmd.append("--no-cache")
        if exp.get("comprehension"):
            cmd.append("--comprehension")

        start = time.time()
        proc = subprocess.run(cmd, timeout=7200)  # 2 hour timeout per experiment
        elapsed = time.time() - start

        status = "ok" if proc.returncode == 0 else f"failed (exit {proc.returncode})"
        console.print(
            f"  [{('green' if proc.returncode == 0 else 'red')}]{exp_name}: {status} ({elapsed / 60:.1f}m)[/{('green' if proc.returncode == 0 else 'red')}]"
        )

        results.append(
            {
                "name": exp_name,
                "config": str(config_path),
                "output": str(exp_output),
                "status": status,
                "duration_minutes": round(elapsed / 60, 1),
            }
        )

    # Save battery manifest
    (battery_dir / "battery.json").write_text(
        json.dumps(
            {
                "battery_name": battery_name,
                "source": str(battery_path),
                "experiments": results,
            },
            indent=2,
        )
    )

    # Per-experiment summaries (saved as markdown)
    for exp in results:
        if exp.get("status") == "ok":
            exp_dir = Path(exp["output"])
            console.print(f"\n[cyan]Summarizing {exp['name']}...[/cyan]")
            await summarize_results(exp_dir, sdk=sdk)

    # Cross-experiment summary
    console.print(f"\n[bold]{'=' * 60}[/bold]")
    console.print("[bold]Cross-Experiment Analysis[/bold]\n")
    await summarize_battery(battery_dir, results, sdk=sdk)


async def summarize_battery(
    battery_dir: Path, experiments: list[dict], sdk=_default_sdk
):
    """Produce a cross-experiment analysis comparing all experiments."""
    # Collect results from each experiment
    all_data = []
    for exp in experiments:
        if exp.get("status") != "ok":
            all_data.append(
                f"## {exp['name']}\nStatus: {exp.get('status', 'unknown')}\n"
            )
            continue

        exp_dir = Path(exp["output"])
        parts = [f"## {exp['name']}"]

        # Config
        config_path = exp.get("config")
        if config_path and Path(config_path).exists():
            parts.append(f"### Config\n```yaml\n{Path(config_path).read_text()}\n```")

        # Aggregate report
        report_path = exp_dir / "phase3_analysis" / "aggregate_report.md"
        if report_path.exists():
            parts.append(f"### Report\n{report_path.read_text()}")

        # Costs
        costs_path = exp_dir / "costs.json"
        if costs_path.exists():
            parts.append(f"### Costs\n```json\n{costs_path.read_text()}\n```")

        # Per-run analysis results
        for run_dir in sorted(exp_dir.glob("run_*/")):
            analysis_dir = run_dir / "phase3_analysis"
            if analysis_dir.exists():
                for f in sorted(analysis_dir.glob("*.json")):
                    parts.append(
                        f"### {run_dir.name}/{f.stem}\n```json\n{f.read_text()}\n```"
                    )

        all_data.append("\n".join(parts))

    combined = "\n\n---\n\n".join(all_data)

    if sdk is None:
        console.print(combined[:5000])
        return

    system_prompt = """\
You are a research analyst reviewing results from a BATTERY of A/B experiments \
testing whether structured cognitive layers specifications improve AI agent \
performance on software development tasks.

Each experiment varies one or more parameters: model (Sonnet vs Opus), \
extraction quality (Haiku vs Opus, 1 vs 2 vs 3 rounds), project (Python/Go/TS), \
and task complexity (simple CRUD vs medium feature vs cross-cutting refactor).

Your analysis MUST include:

1. **Overall verdict**: Across all experiments, does the cognitive layers spec \
   provide consistent value? Or is it situational?

2. **Experiment comparison table**: For each experiment, one row showing: \
   experiment name, with-spec model, without-spec model, with-spec score, \
   without-spec score, winner, margin.

3. **Factor analysis**: Which factors matter most?
   - Model gap: Does spec close it? By how much?
   - Extraction quality: Does Haiku-spec help? Do more rounds help?
   - Project/language: Does the spec help more on some languages/frameworks?
   - Task complexity: Does the spec help more on complex vs simple tasks?

4. **Cost-benefit**: Total cost of each experiment. Cost of extraction vs \
   quality improvement. What's the ROI sweet spot?

5. **Surprising findings**: Anything that contradicts expectations across \
   experiments?

6. **Recommendations**: Specific, actionable. When should teams use cognitive \
   layers specs? When should they skip it? What extraction config is optimal?

Be quantitative. Reference specific numbers. Draw clear conclusions. \
The reader wants a decision framework, not a literature review."""

    console.print("[cyan]Running cross-experiment analysis...[/cyan]")
    stream = sdk.query(
        prompt=f"Analyze these experiment results and produce cross-experiment findings.\n\n{combined}",
        options=sdk.ClaudeAgentOptions(
            system_prompt=system_prompt,
            max_turns=1,
            disallowed_tools=["Read", "Glob", "Grep", "Bash", "Write", "Edit"],
        ),
    )

    analysis_text = ""
    async for msg in stream:
        if hasattr(msg, "content"):
            for block in msg.content:
                if hasattr(block, "text"):
                    analysis_text = block.text
                    console.print("\n[bold]Cross-Experiment Analysis[/bold]\n")
                    console.print(block.text)
        if isinstance(msg, sdk.ResultMessage):
            if msg.total_cost_usd:
                console.print(f"\n[dim]Analysis cost: ${msg.total_cost_usd:.2f}[/dim]")

    # Save the analysis
    if analysis_text:
        (battery_dir / "cross_experiment_analysis.md").write_text(analysis_text)
        console.print(
            f"\n[green]Saved to {battery_dir / 'cross_experiment_analysis.md'}[/green]"
        )


# ---------------------------------------------------------------------------
# Main orchestrator
# ---------------------------------------------------------------------------


async def main(sdk=_default_sdk):
    parser = argparse.ArgumentParser(
        description="Cognitive Layers Testing Harness - A/B test structured specs vs plain docs"
    )
    parser.add_argument(
        "config",
        nargs="?",
        default=None,
        help="Path to YAML config file (overrides CLI defaults)",
    )
    parser.add_argument(
        "--repo", default=None, help="Git repository URL to test against"
    )
    parser.add_argument("--feature", default=None, help="Feature task description")
    parser.add_argument(
        "--output",
        default=None,
        help="Output directory (default: harness-results/{timestamp}_{project})",
    )
    parser.add_argument(
        "--rounds", type=int, default=None, help="Number of extraction feedback rounds"
    )
    parser.add_argument(
        "--runs", type=int, default=None, help="Number of Phase 2+3 trial repetitions"
    )
    parser.add_argument("--model", default=None, help="Model to use for agent calls")
    parser.add_argument(
        "--max-budget", type=float, default=None, help="Max USD budget per agent call"
    )
    parser.add_argument(
        "--no-cache", action="store_true", help="Bypass Phase 1 spec cache"
    )
    parser.add_argument(
        "--resume", default=None, help="Path to existing output dir to resume"
    )
    parser.add_argument(
        "--comprehension",
        action="store_true",
        default=False,
        help="Run comprehension test (elective, tests understanding not implementation)",
    )
    parser.add_argument(
        "--summarize",
        default=None,
        metavar="OUTPUT_DIR",
        help="Summarize results from a completed run (skips all phases)",
    )
    parser.add_argument(
        "--battery",
        default=None,
        metavar="BATTERY_YAML",
        help="Run a battery of experiments from a meta-config",
    )

    args = parser.parse_args()

    # Battery mode: run all experiments and exit
    if args.battery:
        await run_battery(Path(args.battery), sdk=sdk)
        return

    # Summarize mode: load results and exit
    if args.summarize:
        await summarize_results(Path(args.summarize), sdk=sdk)
        return

    # Load config from YAML file (CLI flags override config values)
    cfg: dict[str, Any] = {}
    if args.config:
        if yaml is None:
            console.print(
                "[red]pyyaml is required for config files (pip install pyyaml)[/red]"
            )
            sys.exit(1)
        config_path = Path(args.config)
        if not config_path.exists():
            console.print(f"[red]Config file not found: {config_path}[/red]")
            sys.exit(1)
        cfg = yaml.safe_load(config_path.read_text()) or {}

    # Merge: CLI flag > config file > built-in default
    args.repo = args.repo or cfg.get(
        "repo", "https://github.com/paperless-ngx/paperless-ngx"
    )
    args.feature = args.feature or cfg.get("feature", DEFAULT_FEATURE_TASK)
    args.output = args.output or cfg.get("output")
    args.rounds = args.rounds if args.rounds is not None else cfg.get("rounds", 2)
    args.runs = args.runs if args.runs is not None else cfg.get("runs", 3)
    args.model = args.model or cfg.get("model")
    args.max_budget = (
        args.max_budget if args.max_budget is not None else cfg.get("max_budget")
    )
    args.no_cache = args.no_cache or cfg.get("no_cache", False)
    args.resume = args.resume or cfg.get("resume")
    args.comprehension = args.comprehension or cfg.get("comprehension", False)

    # Per-phase models: --model overrides all; otherwise per-phase config; then defaults
    # Default strategy: Opus for extraction, Sonnet+spec vs Opus without spec
    base_model = args.model
    extraction_model = base_model or cfg.get("extraction_model", "claude-opus-4-6")
    with_spec_model = base_model or cfg.get("with_spec_model", "claude-sonnet-4-6")
    without_spec_model = base_model or cfg.get("without_spec_model", "claude-opus-4-6")
    analysis_model = base_model or cfg.get("analysis_model")

    # Determine project name and output dir
    project_name = args.repo.rstrip("/").split("/")[-1]
    if args.resume:
        output_dir = Path(args.resume)
    elif args.output:
        output_dir = Path(args.output)
    else:
        timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
        output_dir = Path("harness-results") / f"{timestamp}_{project_name}"

    output_dir.mkdir(parents=True, exist_ok=True)
    checkpoint_path = output_dir / "checkpoint.json"
    checkpoint = Checkpoint.load(checkpoint_path)
    cost_tracker = CostTracker()

    # Set up live progress UI
    global _ui
    ui = HarnessUI(total_runs=args.runs, rounds=args.rounds, cost_tracker=cost_tracker)
    _ui = ui
    ui.start()

    ls_dir = Path(__file__).parent

    console.print("[bold]Cognitive Layers Testing Harness[/bold]")
    console.print(f"Project: {args.repo}")
    console.print(f"Output: {output_dir}")
    console.print(f"Runs: {args.runs}")
    console.print(
        f"Models: extraction={extraction_model or 'default'}, "
        f"with-spec={with_spec_model or 'default'}, "
        f"without-spec={without_spec_model or 'default'}, "
        f"analysis={analysis_model or 'default'}"
    )

    # Install signal handler for Ctrl-C during async execution
    loop = asyncio.get_running_loop()

    def _async_sigint():
        global _shutting_down
        _shutting_down = True
        if _ui:
            _ui.stop()
        console.print(
            "\n[red]Interrupted. Saving checkpoint and killing agents...[/red]"
        )
        cost_tracker.save(output_dir / "costs.json")
        checkpoint.save(checkpoint_path)
        _kill_claude_children()
        # Cancel all running tasks
        for task in asyncio.all_tasks(loop):
            task.cancel()

    loop.add_signal_handler(signal.SIGINT, _async_sigint)

    # Phase 1: Extract spec
    if not checkpoint.phase1_done:
        project_dir = output_dir / "project"
        clone_project(args.repo, project_dir)

        spec_dir = await phase1_extract(
            project_dir=project_dir,
            output_dir=output_dir,
            ls_dir=ls_dir,
            rounds=args.rounds,
            model=extraction_model,
            max_budget=args.max_budget,
            no_cache=args.no_cache,
            cost_tracker=cost_tracker,
            sdk=sdk,
        )
        checkpoint.phase1_done = True
        checkpoint.spec_dir = str(spec_dir)
        checkpoint.save(checkpoint_path)
    else:
        spec_dir = Path(checkpoint.spec_dir)
        console.print("[green]Phase 1 already complete (resuming)[/green]")

    project_dir = output_dir / "project"

    # Comprehension test (elective, runs once after Phase 1)
    comprehension_result = None
    if args.comprehension and not checkpoint.comprehension_done:
        comprehension_result = await phase_comprehension(
            project_dir=project_dir,
            spec_dir=spec_dir,
            output_dir=output_dir,
            model=analysis_model,
            max_budget=args.max_budget,
            cost_tracker=cost_tracker,
            sdk=sdk,
        )
        checkpoint.comprehension_done = True
        checkpoint.save(checkpoint_path)
    elif args.comprehension and checkpoint.comprehension_done:
        console.print("[green]Comprehension test already complete (resuming)[/green]")
        comp_summary_path = output_dir / "comprehension" / "comprehension_summary.json"
        if comp_summary_path.exists():
            comprehension_result = json.loads(comp_summary_path.read_text())

    # Phase 2+3: Repeated runs
    # Resume from the earliest incomplete run (min of phase2 and phase3 completion)
    all_run_results = []
    resume_from = min(
        checkpoint.phase2_runs_completed, checkpoint.phase3_runs_completed
    )

    # Load already-completed run results from disk
    for completed_idx in range(resume_from):
        result_dir = output_dir / f"run_{completed_idx + 1}" / "phase3_analysis"
        if result_dir.exists():
            run_result = {}
            for name in [
                "objective_metrics",
                "code_quality",
                "security",
                "architecture",
                "plan_quality",
                "spec_alignment",
            ]:
                fpath = result_dir / f"{name}.json"
                if fpath.exists():
                    key = name.replace("_metrics", "")
                    run_result[key] = json.loads(fpath.read_text())
            all_run_results.append(run_result)

    for run_idx in range(resume_from, args.runs):
        if _ui:
            _ui.set_run(run_idx + 1)
        console.print(f"\n[bold magenta]{'=' * 60}[/bold magenta]")
        console.print(f"[bold magenta]Run {run_idx + 1}/{args.runs}[/bold magenta]")

        run_output = output_dir / f"run_{run_idx + 1}"
        run_output.mkdir(parents=True, exist_ok=True)

        # Phase 2 (skip if already done for this run)
        dev_result_path = run_output / "phase2_development"
        if run_idx < checkpoint.phase2_runs_completed and dev_result_path.exists():
            console.print(
                f"  [green]Phase 2 already complete for run {run_idx + 1}[/green]"
            )
            # Reconstruct dev_result from saved files
            with_diff_path = dev_result_path / "with_spec_diff.patch"
            without_diff_path = dev_result_path / "without_spec_diff.patch"
            dev_result = {
                "with_spec_dir": str(dev_result_path / "dev-with-spec"),
                "without_spec_dir": str(dev_result_path / "dev-without-spec"),
                "with_diff": with_diff_path.read_text()
                if with_diff_path.exists()
                else "",
                "without_diff": without_diff_path.read_text()
                if without_diff_path.exists()
                else "",
                "coverage_complete": True,
            }
        else:
            dev_result = await phase2_develop(
                project_dir=project_dir,
                spec_dir=spec_dir,
                feature_task=args.feature,
                output_dir=run_output,
                with_spec_model=with_spec_model,
                without_spec_model=without_spec_model,
                max_budget=args.max_budget,
                cost_tracker=cost_tracker,
                sdk=sdk,
            )
            checkpoint.phase2_runs_completed = run_idx + 1
            checkpoint.save(checkpoint_path)

        # Phase 3
        analysis = await phase3_analyze(
            dev_result=dev_result,
            output_dir=run_output,
            model=analysis_model,
            max_budget=args.max_budget,
            cost_tracker=cost_tracker,
            sdk=sdk,
        )

        all_run_results.append(analysis)
        checkpoint.phase3_runs_completed = run_idx + 1
        checkpoint.save(checkpoint_path)

    # Generate aggregate report
    (output_dir / "phase3_analysis").mkdir(parents=True, exist_ok=True)
    report = generate_comparative_report(
        all_run_results,
        output_dir,
        cost_tracker,
        comprehension_result=comprehension_result,
    )
    cost_tracker.save(output_dir / "costs.json")

    # Stop the live UI before final output
    ui.stop()
    _ui = None

    console.print(
        f"\n[bold green]All done! Total cost: ${cost_tracker.total_cost:.2f}[/bold green]"
    )
    console.print(report)


if __name__ == "__main__":
    # Allow launching Claude CLI from within a Claude Code session
    os.environ.pop("CLAUDECODE", None)
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        _shutting_down = True
        if _ui:
            _ui.stop()
        console.print("\n[red]Interrupted. Killing agent processes...[/red]")
        _kill_claude_children()
        sys.exit(130)
