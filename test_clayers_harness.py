# /// script
# dependencies = ["pytest"]
# ///
"""
Integration tests for clayers-harness.py using mock_sdk.

Tests orchestration logic without real API calls by injecting mock_sdk
as the SDK module. Subprocess calls (living-spec.py, git, linters) are
mocked via unittest.mock.patch.
"""

from __future__ import annotations

import asyncio
import importlib.util
import json
import subprocess
import sys
from pathlib import Path
from unittest.mock import patch, MagicMock

import pytest

# Import mock_sdk from same directory
THIS_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(THIS_DIR))
import mock_sdk  # noqa: E402

# Pre-inject mock_sdk as claude_agent_sdk so harness import succeeds without the real SDK
sys.modules["claude_agent_sdk"] = mock_sdk

# Import harness module - register in sys.modules so dataclass resolution works
_spec = importlib.util.spec_from_file_location(
    "harness", THIS_DIR / "clayers-harness.py"
)
harness = importlib.util.module_from_spec(_spec)
sys.modules["harness"] = harness
_spec.loader.exec_module(harness)


# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------


@pytest.fixture(autouse=True)
def reset_mock():
    """Reset mock_sdk state before each test."""
    mock_sdk.reset()
    yield
    mock_sdk.reset()


@pytest.fixture
def tmp_project(tmp_path):
    """Create a minimal project structure for testing."""
    proj = tmp_path / "flask"
    proj.mkdir()
    (proj / "README.md").write_text("# Flask\nA micro web framework.")
    (proj / "pyproject.toml").write_text("[project]\nname = 'flask'\n")
    (proj / "src").mkdir()
    (proj / "src" / "__init__.py").write_text("# Flask init")
    # Init git repo so git diff works
    subprocess.run(["git", "init"], cwd=proj, capture_output=True)
    subprocess.run(["git", "add", "-A"], cwd=proj, capture_output=True)
    subprocess.run(
        ["git", "commit", "-m", "init", "--allow-empty"],
        cwd=proj,
        capture_output=True,
        env={
            **dict(__import__("os").environ),
            "GIT_AUTHOR_NAME": "test",
            "GIT_AUTHOR_EMAIL": "t@t",
            "GIT_COMMITTER_NAME": "test",
            "GIT_COMMITTER_EMAIL": "t@t",
        },
    )
    return proj


@pytest.fixture
def scenarios_dir(tmp_path):
    """Create scenario directory with standard scenarios."""
    sdir = tmp_path / "scenarios"
    sdir.mkdir()

    # Explorer scenario: reads codebase and returns summary
    (sdir / "explorer.json").write_text(
        json.dumps(
            {
                "match": {"system_prompt_contains": "codebase explorer"},
                "responses": [
                    {
                        "type": "assistant",
                        "text": '{"project_name": "flask", "modules": [{"name": "app", "path": "src/flask/app.py"}]}',
                    },
                    {
                        "type": "result",
                        "total_cost_usd": 0.30,
                        "duration_ms": 800,
                        "num_turns": 8,
                    },
                ],
            }
        )
    )

    # Architect scenario: yields minimal XML
    minimal_xml = '<?xml version="1.0"?><spec:living-spec xmlns:spec="urn:livingspec:spec"><overview/></spec:living-spec>'
    (sdir / "architect.json").write_text(
        json.dumps(
            {
                "match": {"system_prompt_contains": "architect"},
                "responses": [
                    {"type": "assistant", "text": minimal_xml},
                    {
                        "type": "result",
                        "total_cost_usd": 0.50,
                        "duration_ms": 1000,
                        "num_turns": 5,
                    },
                ],
            }
        )
    )

    # Artifact mapper scenario
    (sdir / "mapper.json").write_text(
        json.dumps(
            {
                "match": {"system_prompt_contains": "artifact mapper"},
                "responses": [
                    {
                        "type": "assistant",
                        "text": "Created artifact mappings for all spec nodes.",
                    },
                    {
                        "type": "result",
                        "total_cost_usd": 0.35,
                        "duration_ms": 900,
                        "num_turns": 10,
                    },
                ],
            }
        )
    )

    # Validator scenario
    (sdir / "validator.json").write_text(
        json.dumps(
            {
                "match": {"system_prompt_contains": "domain expert"},
                "responses": [
                    {
                        "type": "assistant",
                        "text": json.dumps(
                            {
                                "accuracy_issues": [],
                                "completeness_gaps": ["missing core module"],
                                "relation_issues": [],
                                "organization_issues": [],
                                "depth_issues": [],
                                "overall_score": 7,
                            }
                        ),
                    },
                    {
                        "type": "result",
                        "total_cost_usd": 0.20,
                        "duration_ms": 500,
                        "num_turns": 3,
                    },
                ],
            }
        )
    )

    # Compliance reviewer scenario
    (sdir / "compliance.json").write_text(
        json.dumps(
            {
                "match": {"system_prompt_contains": "compliance"},
                "responses": [
                    {
                        "type": "assistant",
                        "text": json.dumps(
                            {
                                "schema_violations": [],
                                "id_issues": [],
                                "reference_issues": [],
                                "namespace_issues": [],
                                "coverage_gaps": [],
                                "overall_compliance": 8,
                            }
                        ),
                    },
                    {
                        "type": "result",
                        "total_cost_usd": 0.15,
                        "duration_ms": 400,
                        "num_turns": 2,
                    },
                ],
            }
        )
    )

    # Development session scenarios
    (sdir / "dev_with_spec.json").write_text(
        json.dumps(
            {
                "match": {"system_prompt_contains": "cognitive layers specification"},
                "responses": [
                    {
                        "type": "assistant",
                        "text": "Implemented rate limiter with spec guidance.",
                    },
                    {
                        "type": "result",
                        "total_cost_usd": 2.00,
                        "duration_ms": 30000,
                        "num_turns": 20,
                    },
                ],
            }
        )
    )

    (sdir / "dev_without_spec.json").write_text(
        json.dumps(
            {
                "match": {"system_prompt_contains": "expert developer"},
                "responses": [
                    {"type": "assistant", "text": "Implemented rate limiter."},
                    {
                        "type": "result",
                        "total_cost_usd": 1.80,
                        "duration_ms": 25000,
                        "num_turns": 18,
                    },
                ],
            }
        )
    )

    # Coverage nudge scenario
    (sdir / "nudge.json").write_text(
        json.dumps(
            {
                "match": {"prompt_contains": "artifact coverage is incomplete"},
                "responses": [
                    {"type": "assistant", "text": "Added missing artifact mappings."},
                    {
                        "type": "result",
                        "total_cost_usd": 0.30,
                        "duration_ms": 5000,
                        "num_turns": 3,
                    },
                ],
            }
        )
    )

    # Analysis scenarios
    for dim in ["code_quality", "security", "architecture", "plan_quality"]:
        (sdir / f"analysis_{dim}.json").write_text(
            json.dumps(
                {
                    "match": {"prompt_contains": dim.replace("_", " ")},
                    "responses": [
                        {
                            "type": "assistant",
                            "text": json.dumps(
                                {
                                    "score_with": 8,
                                    "score_without": 6,
                                    "findings_with": ["good"],
                                    "findings_without": ["ok"],
                                    "verdict": "with-spec better",
                                }
                            ),
                        },
                        {
                            "type": "result",
                            "total_cost_usd": 0.25,
                            "duration_ms": 3000,
                            "num_turns": 2,
                        },
                    ],
                }
            )
        )

    return sdir


def _mock_subprocess(*args, **kwargs):
    """Mock subprocess.run that returns success for living-spec.py and git commands."""
    cmd = args[0] if args else kwargs.get("args", [])
    cmd_str = " ".join(str(c) for c in cmd) if isinstance(cmd, list) else str(cmd)

    result = MagicMock()
    result.returncode = 0
    result.stdout = ""
    result.stderr = ""

    if "validate" in cmd_str:
        result.stdout = "exit_code=0\nAll files valid."
    elif "connectivity" in cmd_str:
        result.stdout = "exit_code=0\nFully connected."
    elif "artifact" in cmd_str and "--coverage" in cmd_str:
        result.stdout = "exit_code=0\nCoverage: 95%"
    elif "git" in cmd_str and "diff" in cmd_str:
        result.stdout = "+def test_something():\n+    pass\n-old_line\n"
    elif "git" in cmd_str and "clone" in cmd_str:
        # For clone, create the directory
        parts = cmd if isinstance(cmd, list) else cmd.split()
        for i, p in enumerate(parts):
            if i > 0 and not p.startswith("-") and "/" in p and i == len(parts) - 1:
                Path(p).mkdir(parents=True, exist_ok=True)
    elif "pytest" in cmd_str or "ruff" in cmd_str:
        result.stdout = "3 passed\n"

    return result


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------


class TestPhase1Extraction:
    """Test Phase 1 extraction orchestration."""

    def test_phase1_calls_subagents_per_round(
        self, tmp_path, tmp_project, scenarios_dir
    ):
        """Verify subagents (explorer, architect, mapper) are called each round."""
        mock_sdk.load_scenarios(scenarios_dir)
        output_dir = tmp_path / "output"
        output_dir.mkdir()

        with patch("subprocess.run", side_effect=_mock_subprocess):
            asyncio.run(
                harness.phase1_extract(
                    project_dir=tmp_project,
                    output_dir=output_dir,
                    ls_dir=THIS_DIR,
                    rounds=2,
                    cost_tracker=harness.CostTracker(),
                    sdk=mock_sdk,
                )
            )

        # Architect extractor should be called once per round
        architect_calls = [
            c
            for c in mock_sdk.call_log
            if c["options"]
            and c["options"].system_prompt
            and "architect extractor" in c["options"].system_prompt.lower()
        ]
        assert len(architect_calls) == 2, (
            f"Expected 2 architect calls, got {len(architect_calls)}"
        )

        # Explorer should be called once per round
        explorer_calls = [
            c
            for c in mock_sdk.call_log
            if c["options"]
            and c["options"].system_prompt
            and "codebase explorer" in c["options"].system_prompt.lower()
        ]
        assert len(explorer_calls) == 2, (
            f"Expected 2 explorer calls, got {len(explorer_calls)}"
        )

        # Artifact mapper should be called once per round
        mapper_calls = [
            c
            for c in mock_sdk.call_log
            if c["options"]
            and c["options"].system_prompt
            and "artifact mapper" in c["options"].system_prompt.lower()
        ]
        assert len(mapper_calls) == 2, (
            f"Expected 2 mapper calls, got {len(mapper_calls)}"
        )

    def test_phase1_calls_validator_and_compliance(
        self, tmp_path, tmp_project, scenarios_dir
    ):
        """Verify validator and compliance reviewer are called each round."""
        mock_sdk.load_scenarios(scenarios_dir)
        output_dir = tmp_path / "output"
        output_dir.mkdir()

        with patch("subprocess.run", side_effect=_mock_subprocess):
            asyncio.run(
                harness.phase1_extract(
                    project_dir=tmp_project,
                    output_dir=output_dir,
                    ls_dir=THIS_DIR,
                    rounds=1,
                    cost_tracker=harness.CostTracker(),
                    sdk=mock_sdk,
                )
            )

        validator_calls = [
            c
            for c in mock_sdk.call_log
            if c["options"]
            and c["options"].system_prompt
            and "domain expert" in c["options"].system_prompt.lower()
        ]
        compliance_calls = [
            c
            for c in mock_sdk.call_log
            if c["options"]
            and c["options"].system_prompt
            and "compliance" in c["options"].system_prompt.lower()
        ]
        assert len(validator_calls) >= 1, "Must call domain validator"
        assert len(compliance_calls) >= 1, "Must call compliance reviewer"

    def test_phase1_feedback_loop(self, tmp_path, tmp_project, scenarios_dir):
        """Verify feedback from validator/compliance is fed back to subagents."""
        mock_sdk.load_scenarios(scenarios_dir)
        output_dir = tmp_path / "output"
        output_dir.mkdir()

        with patch("subprocess.run", side_effect=_mock_subprocess):
            asyncio.run(
                harness.phase1_extract(
                    project_dir=tmp_project,
                    output_dir=output_dir,
                    ls_dir=THIS_DIR,
                    rounds=2,
                    cost_tracker=harness.CostTracker(),
                    sdk=mock_sdk,
                )
            )

        # Second round's explorer should have feedback in prompt
        explorer_calls = [
            c
            for c in mock_sdk.call_log
            if c["options"]
            and c["options"].system_prompt
            and "codebase explorer" in c["options"].system_prompt.lower()
        ]
        assert len(explorer_calls) == 2
        second_explorer_prompt = explorer_calls[1]["prompt"]
        assert "feedback" in second_explorer_prompt.lower(), (
            "Second round explorer must include feedback from previous round"
        )

        # Second round's architect should also have feedback
        architect_calls = [
            c
            for c in mock_sdk.call_log
            if c["options"]
            and c["options"].system_prompt
            and "architect extractor" in c["options"].system_prompt.lower()
        ]
        assert len(architect_calls) == 2
        second_architect_prompt = architect_calls[1]["prompt"]
        assert "feedback" in second_architect_prompt.lower(), (
            "Second round architect must include feedback from previous round"
        )

    def test_phase1_caching(self, tmp_path, tmp_project, scenarios_dir):
        """Verify cached spec is reused on second call."""
        mock_sdk.load_scenarios(scenarios_dir)
        output_dir = tmp_path / "output"
        output_dir.mkdir()

        # Create a cached spec
        cache_dir = output_dir.parent / "_cache" / "flask_r2_default" / "final_spec"
        cache_dir.mkdir(parents=True)
        (cache_dir / "index.xml").write_text("<index/>")

        with patch("subprocess.run", side_effect=_mock_subprocess):
            result = asyncio.run(
                harness.phase1_extract(
                    project_dir=tmp_project,
                    output_dir=output_dir,
                    ls_dir=THIS_DIR,
                    rounds=2,
                    no_cache=False,
                    cost_tracker=harness.CostTracker(),
                    sdk=mock_sdk,
                )
            )

        # Should have used cache - no SDK calls
        assert len(mock_sdk.call_log) == 0, "Should skip extraction when cache exists"
        assert result == cache_dir

    def test_phase1_no_cache_flag(self, tmp_path, tmp_project, scenarios_dir):
        """Verify --no-cache bypasses cached spec."""
        mock_sdk.load_scenarios(scenarios_dir)
        output_dir = tmp_path / "output"
        output_dir.mkdir()

        # Create a cached spec
        cache_dir = output_dir.parent / "_cache" / "flask_r2_default" / "final_spec"
        cache_dir.mkdir(parents=True)
        (cache_dir / "index.xml").write_text("<index/>")

        with patch("subprocess.run", side_effect=_mock_subprocess):
            asyncio.run(
                harness.phase1_extract(
                    project_dir=tmp_project,
                    output_dir=output_dir,
                    ls_dir=THIS_DIR,
                    rounds=1,
                    no_cache=True,
                    cost_tracker=harness.CostTracker(),
                    sdk=mock_sdk,
                )
            )

        assert len(mock_sdk.call_log) > 0, "Must run extraction when --no-cache is set"


class TestPhase1QualityGate:
    """Test Phase 1 quality gate: validation + connectivity must pass before Phase 2."""

    def test_repair_agent_called_on_validation_failure(
        self, tmp_path, tmp_project, scenarios_dir
    ):
        """Verify repair agent is invoked when final validation fails."""
        mock_sdk.load_scenarios(scenarios_dir)
        # Add repair scenario
        mock_sdk.load_scenario(
            {
                "match": {"system_prompt_contains": "repair agent"},
                "responses": [
                    {"type": "assistant", "text": "Fixed all validation errors."},
                    {
                        "type": "result",
                        "total_cost_usd": 0.40,
                        "duration_ms": 2000,
                        "num_turns": 5,
                    },
                ],
            }
        )
        output_dir = tmp_path / "output"
        output_dir.mkdir()

        # Track validate calls to return failure first, then success
        # With 1 round: call 1 = round validation, call 2 = quality gate validate
        validate_call_count = {"n": 0}

        def mock_subprocess_failing_validate(*args, **kwargs):
            cmd = args[0] if args else kwargs.get("args", [])
            cmd_str = (
                " ".join(str(c) for c in cmd) if isinstance(cmd, list) else str(cmd)
            )

            result = MagicMock()
            result.returncode = 0
            result.stdout = ""
            result.stderr = ""

            if "validate" in cmd_str:
                validate_call_count["n"] += 1
                if validate_call_count["n"] <= 1:
                    # Round validation passes
                    result.stdout = "All files valid."
                elif validate_call_count["n"] == 2:
                    # Quality gate validation fails
                    result.returncode = 1
                    result.stdout = (
                        "FAIL: Unexpected child with tag 'art:artifact' at position 3. "
                        "Tag 'art:coverage' expected.\n"
                        "FAIL: missing required attribute 'source'"
                    )
                else:
                    # After repair, passes
                    result.stdout = "All files valid."
            elif "connectivity" in cmd_str:
                result.stdout = "Connected Components: 1\nIsolated Nodes: 0"
            elif "artifact" in cmd_str and "--coverage" in cmd_str:
                result.stdout = "Coverage: 95%"
            elif "git" in cmd_str:
                pass  # default empty success

            return result

        with patch("subprocess.run", side_effect=mock_subprocess_failing_validate):
            asyncio.run(
                harness.phase1_extract(
                    project_dir=tmp_project,
                    output_dir=output_dir,
                    ls_dir=THIS_DIR,
                    rounds=1,
                    cost_tracker=harness.CostTracker(),
                    sdk=mock_sdk,
                )
            )

        # Verify repair agent was called
        repair_calls = [
            c
            for c in mock_sdk.call_log
            if c["options"]
            and c["options"].system_prompt
            and "repair agent" in (c["options"].system_prompt or "").lower()
        ]
        assert len(repair_calls) >= 1, (
            "Repair agent must be called when validation fails"
        )

    def test_repair_agent_called_on_connectivity_failure(
        self, tmp_path, tmp_project, scenarios_dir
    ):
        """Verify repair agent is invoked when connectivity has isolated nodes or cycles."""
        mock_sdk.load_scenarios(scenarios_dir)
        mock_sdk.load_scenario(
            {
                "match": {"system_prompt_contains": "repair agent"},
                "responses": [
                    {
                        "type": "assistant",
                        "text": "Connected isolated nodes and fixed cycles.",
                    },
                    {
                        "type": "result",
                        "total_cost_usd": 0.35,
                        "duration_ms": 1500,
                        "num_turns": 4,
                    },
                ],
            }
        )
        output_dir = tmp_path / "output"
        output_dir.mkdir()

        connectivity_call_count = {"n": 0}

        def mock_subprocess_failing_connectivity(*args, **kwargs):
            cmd = args[0] if args else kwargs.get("args", [])
            cmd_str = (
                " ".join(str(c) for c in cmd) if isinstance(cmd, list) else str(cmd)
            )

            result = MagicMock()
            result.returncode = 0
            result.stdout = ""
            result.stderr = ""

            if "validate" in cmd_str:
                result.stdout = "All files valid."
            elif "connectivity" in cmd_str:
                connectivity_call_count["n"] += 1
                if connectivity_call_count["n"] <= 1:
                    # Round connectivity passes
                    result.stdout = "Connected Components: 1\nIsolated Nodes: 0"
                elif connectivity_call_count["n"] == 2:
                    # Final connectivity fails - isolated nodes + acyclic cycles
                    result.returncode = 1
                    result.stdout = (
                        "Connected Components: 5\n"
                        "Isolated Nodes: 4\n"
                        "  type-decision\n  type-plan\n  type-witness\n  type-relation\n"
                        "Cycles: 1\n"
                        "  ERROR term-a -> term-b -> term-a\n"
                        "    Cycle contains acyclic relation type(s): depends-on"
                    )
                else:
                    # After repair, passes
                    result.stdout = "Connected Components: 1\nIsolated Nodes: 0"
            elif "artifact" in cmd_str and "--coverage" in cmd_str:
                result.stdout = "Coverage: 95%"
            elif "git" in cmd_str:
                pass

            return result

        with patch("subprocess.run", side_effect=mock_subprocess_failing_connectivity):
            asyncio.run(
                harness.phase1_extract(
                    project_dir=tmp_project,
                    output_dir=output_dir,
                    ls_dir=THIS_DIR,
                    rounds=1,
                    cost_tracker=harness.CostTracker(),
                    sdk=mock_sdk,
                )
            )

        repair_calls = [
            c
            for c in mock_sdk.call_log
            if c["options"]
            and c["options"].system_prompt
            and "repair agent" in (c["options"].system_prompt or "").lower()
        ]
        assert len(repair_calls) >= 1, (
            "Repair agent must be called when connectivity has isolated nodes or cycles"
        )

    def test_aborts_after_max_repair_attempts(
        self, tmp_path, tmp_project, scenarios_dir
    ):
        """Verify harness aborts if spec cannot be repaired after max attempts."""
        mock_sdk.load_scenarios(scenarios_dir)
        mock_sdk.load_scenario(
            {
                "match": {"system_prompt_contains": "repair agent"},
                "responses": [
                    {
                        "type": "assistant",
                        "text": "Attempted repair but issues remain.",
                    },
                    {
                        "type": "result",
                        "total_cost_usd": 0.30,
                        "duration_ms": 1000,
                        "num_turns": 3,
                    },
                ],
            }
        )
        output_dir = tmp_path / "output"
        output_dir.mkdir()

        # Track validate calls: round validation passes, quality gate always fails
        always_fail_validate_count = {"n": 0}

        def mock_subprocess_always_fail(*args, **kwargs):
            cmd = args[0] if args else kwargs.get("args", [])
            cmd_str = (
                " ".join(str(c) for c in cmd) if isinstance(cmd, list) else str(cmd)
            )

            result = MagicMock()
            result.returncode = 0
            result.stdout = ""
            result.stderr = ""

            if "validate" in cmd_str:
                always_fail_validate_count["n"] += 1
                if always_fail_validate_count["n"] <= 1:
                    # Round validation passes
                    result.stdout = "All files valid."
                else:
                    # Quality gate + all repair attempts: always fail
                    result.returncode = 1
                    result.stdout = "FAIL: missing required attribute 'source'"
            elif "connectivity" in cmd_str:
                result.stdout = "Connected Components: 1\nIsolated Nodes: 0"
            elif "artifact" in cmd_str and "--coverage" in cmd_str:
                result.stdout = "Coverage: 95%"
            elif "git" in cmd_str:
                pass

            return result

        with patch("subprocess.run", side_effect=mock_subprocess_always_fail):
            with pytest.raises(RuntimeError, match="quality gate failed"):
                asyncio.run(
                    harness.phase1_extract(
                        project_dir=tmp_project,
                        output_dir=output_dir,
                        ls_dir=THIS_DIR,
                        rounds=1,
                        cost_tracker=harness.CostTracker(),
                        sdk=mock_sdk,
                    )
                )

        # Should have tried 3 repair attempts before aborting
        repair_calls = [
            c
            for c in mock_sdk.call_log
            if c["options"]
            and c["options"].system_prompt
            and "repair agent" in (c["options"].system_prompt or "").lower()
        ]
        assert len(repair_calls) == 3, (
            f"Must attempt exactly 3 repairs before aborting, got {len(repair_calls)}"
        )


class TestPhase2Development:
    """Test Phase 2 parallel development orchestration."""

    def test_phase2_launches_both_sessions(self, tmp_path, tmp_project, scenarios_dir):
        """Verify both with-spec and without-spec sessions are launched."""
        mock_sdk.load_scenarios(scenarios_dir)
        output_dir = tmp_path / "output"
        output_dir.mkdir()

        spec_dir = tmp_path / "spec"
        spec_dir.mkdir()
        (spec_dir / "index.xml").write_text("<index/>")

        with patch("subprocess.run", side_effect=_mock_subprocess):
            with patch.object(
                harness, "create_git_diff", return_value="+new line\n-old line\n"
            ):
                asyncio.run(
                    harness.phase2_develop(
                        project_dir=tmp_project,
                        spec_dir=spec_dir,
                        feature_task="Implement rate limiter",
                        output_dir=output_dir,
                        cost_tracker=harness.CostTracker(),
                        sdk=mock_sdk,
                    )
                )

        # Should have calls for both sessions
        with_spec_calls = [
            c
            for c in mock_sdk.call_log
            if c["options"]
            and c["options"].system_prompt
            and "cognitive layers specification"
            in (c["options"].system_prompt or "").lower()
        ]
        without_spec_calls = [
            c
            for c in mock_sdk.call_log
            if c["options"]
            and c["options"].system_prompt
            and "expert developer" in (c["options"].system_prompt or "").lower()
        ]
        assert len(with_spec_calls) >= 1, "Must launch Session A (with-spec)"
        assert len(without_spec_calls) >= 1, "Must launch Session B (without-spec)"

    def test_phase2_session_b_gets_project_docs(
        self, tmp_path, tmp_project, scenarios_dir
    ):
        """Verify Session B receives project documentation."""
        mock_sdk.load_scenarios(scenarios_dir)
        output_dir = tmp_path / "output"
        output_dir.mkdir()

        spec_dir = tmp_path / "spec"
        spec_dir.mkdir()
        (spec_dir / "index.xml").write_text("<index/>")

        with patch("subprocess.run", side_effect=_mock_subprocess):
            with patch.object(harness, "create_git_diff", return_value=""):
                asyncio.run(
                    harness.phase2_develop(
                        project_dir=tmp_project,
                        spec_dir=spec_dir,
                        feature_task="Implement rate limiter",
                        output_dir=output_dir,
                        cost_tracker=harness.CostTracker(),
                        sdk=mock_sdk,
                    )
                )

        without_spec_calls = [
            c
            for c in mock_sdk.call_log
            if c["options"]
            and c["options"].system_prompt
            and "expert developer" in (c["options"].system_prompt or "").lower()
        ]
        assert len(without_spec_calls) >= 1
        sp = without_spec_calls[0]["options"].system_prompt
        # Session B must have project docs (README content)
        assert (
            "Flask" in sp or "reference" in sp.lower() or "documentation" in sp.lower()
        ), "Session B must receive project documentation"

    def test_phase2_session_isolation(self, tmp_path, tmp_project, scenarios_dir):
        """Verify sessions use separate directories."""
        mock_sdk.load_scenarios(scenarios_dir)
        output_dir = tmp_path / "output"
        output_dir.mkdir()

        spec_dir = tmp_path / "spec"
        spec_dir.mkdir()
        (spec_dir / "index.xml").write_text("<index/>")

        with patch("subprocess.run", side_effect=_mock_subprocess):
            with patch.object(harness, "create_git_diff", return_value=""):
                result = asyncio.run(
                    harness.phase2_develop(
                        project_dir=tmp_project,
                        spec_dir=spec_dir,
                        feature_task="Implement rate limiter",
                        output_dir=output_dir,
                        cost_tracker=harness.CostTracker(),
                        sdk=mock_sdk,
                    )
                )

        assert result["with_spec_dir"] != result["without_spec_dir"], (
            "Sessions must use separate directories"
        )

    def test_phase2_coverage_nudge_loop(self, tmp_path, tmp_project, scenarios_dir):
        """Verify coverage nudge triggers when coverage is incomplete."""
        mock_sdk.load_scenarios(scenarios_dir)
        output_dir = tmp_path / "output"
        output_dir.mkdir()

        spec_dir = tmp_path / "spec"
        spec_dir.mkdir()
        (spec_dir / "index.xml").write_text("<index/>")

        call_count = [0]

        def mock_sub_with_coverage(*args, **kwargs):
            cmd = args[0] if args else kwargs.get("args", [])
            cmd_str = (
                " ".join(str(c) for c in cmd) if isinstance(cmd, list) else str(cmd)
            )
            result = MagicMock()
            result.returncode = 0
            result.stdout = ""
            result.stderr = ""
            if "artifact" in cmd_str and "--coverage" in cmd_str:
                call_count[0] += 1
                if call_count[0] <= 1:
                    result.returncode = 1
                    result.stdout = (
                        "exit_code=1\nCoverage: 60%\nUncovered: src/limiter.py"
                    )
                else:
                    result.stdout = "exit_code=0\nCoverage: 95%"
            elif "git" in cmd_str and "diff" in cmd_str:
                result.stdout = "+line\n"
            return result

        with patch("subprocess.run", side_effect=mock_sub_with_coverage):
            with patch.object(harness, "create_git_diff", return_value=""):
                asyncio.run(
                    harness.phase2_develop(
                        project_dir=tmp_project,
                        spec_dir=spec_dir,
                        feature_task="Implement rate limiter",
                        output_dir=output_dir,
                        cost_tracker=harness.CostTracker(),
                        sdk=mock_sdk,
                    )
                )

        nudge_calls = [
            c
            for c in mock_sdk.call_log
            if "coverage" in c["prompt"].lower() and "incomplete" in c["prompt"].lower()
        ]
        assert len(nudge_calls) >= 1, "Must nudge when coverage is incomplete"


class TestPhase3Analysis:
    """Test Phase 3 analysis orchestration."""

    def test_phase3_collects_objective_metrics(self, tmp_path, scenarios_dir):
        """Verify objective metrics are collected."""
        mock_sdk.load_scenarios(scenarios_dir)

        dev_result = {
            "with_spec_dir": str(tmp_path / "with"),
            "without_spec_dir": str(tmp_path / "without"),
            "with_diff": "+def test_one():\n+    pass\n-old\n",
            "without_diff": "+def something():\n-old\n",
            "coverage_complete": True,
        }
        for d in ["with", "without"]:
            (tmp_path / d).mkdir()
            (tmp_path / d / "pyproject.toml").write_text("[project]\nname='t'\n")

        output_dir = tmp_path / "output"
        output_dir.mkdir()

        with patch("subprocess.run", side_effect=_mock_subprocess):
            result = asyncio.run(
                harness.phase3_analyze(
                    dev_result=dev_result,
                    output_dir=output_dir,
                    cost_tracker=harness.CostTracker(),
                    sdk=mock_sdk,
                )
            )

        assert "objective" in result
        assert "with_spec" in result["objective"]
        assert "without_spec" in result["objective"]
        assert "delta" in result["objective"]

    def test_phase3_runs_llm_analysis(self, tmp_path, scenarios_dir):
        """Verify LLM analysis dimensions are invoked."""
        mock_sdk.load_scenarios(scenarios_dir)

        dev_result = {
            "with_spec_dir": str(tmp_path / "with"),
            "without_spec_dir": str(tmp_path / "without"),
            "with_diff": "+new\n",
            "without_diff": "+new\n",
            "coverage_complete": True,
        }
        for d in ["with", "without"]:
            (tmp_path / d).mkdir()
            (tmp_path / d / "pyproject.toml").write_text("[project]\nname='t'\n")

        output_dir = tmp_path / "output"
        output_dir.mkdir()

        with patch("subprocess.run", side_effect=_mock_subprocess):
            result = asyncio.run(
                harness.phase3_analyze(
                    dev_result=dev_result,
                    output_dir=output_dir,
                    cost_tracker=harness.CostTracker(),
                    sdk=mock_sdk,
                )
            )

        assert "code_quality" in result
        assert "security" in result
        assert "architecture" in result
        assert "plan_quality" in result

    def test_phase3_plan_quality_analysis(self, tmp_path, scenarios_dir):
        """Verify plan quality is analyzed for Session A."""
        mock_sdk.load_scenarios(scenarios_dir)

        dev_result = {
            "with_spec_dir": str(tmp_path / "with"),
            "without_spec_dir": str(tmp_path / "without"),
            "with_diff": "+new\n",
            "without_diff": "+new\n",
            "coverage_complete": True,
        }
        for d in ["with", "without"]:
            (tmp_path / d).mkdir()
            (tmp_path / d / "pyproject.toml").write_text("[project]\nname='t'\n")

        output_dir = tmp_path / "output"
        output_dir.mkdir()

        with patch("subprocess.run", side_effect=_mock_subprocess):
            result = asyncio.run(
                harness.phase3_analyze(
                    dev_result=dev_result,
                    output_dir=output_dir,
                    cost_tracker=harness.CostTracker(),
                    sdk=mock_sdk,
                )
            )

        pq = result["plan_quality"]
        assert pq["dimension"] == "plan_quality"

    def test_phase3_analysis_receives_both_diffs(self, tmp_path, scenarios_dir):
        """Verify analysis agents receive both diffs."""
        mock_sdk.load_scenarios(scenarios_dir)

        dev_result = {
            "with_spec_dir": str(tmp_path / "with"),
            "without_spec_dir": str(tmp_path / "without"),
            "with_diff": "+WITH_SPEC_MARKER\n",
            "without_diff": "+WITHOUT_SPEC_MARKER\n",
            "coverage_complete": True,
        }
        for d in ["with", "without"]:
            (tmp_path / d).mkdir()
            (tmp_path / d / "pyproject.toml").write_text("[project]\nname='t'\n")

        output_dir = tmp_path / "output"
        output_dir.mkdir()

        with patch("subprocess.run", side_effect=_mock_subprocess):
            asyncio.run(
                harness.phase3_analyze(
                    dev_result=dev_result,
                    output_dir=output_dir,
                    cost_tracker=harness.CostTracker(),
                    sdk=mock_sdk,
                )
            )

        # Analysis calls should have both diffs in their prompts
        analysis_calls = [
            c
            for c in mock_sdk.call_log
            if "with-spec diff" in c["prompt"].lower()
            or "without-spec diff" in c["prompt"].lower()
        ]
        assert len(analysis_calls) >= 1, "Analysis must receive both diffs"


class TestCheckpointAndCost:
    """Test checkpoint and cost tracking."""

    def test_checkpoint_save_load(self, tmp_path):
        """Verify checkpoint serialization round-trip."""
        cp = harness.Checkpoint(phase1_done=True, phase2_runs_completed=2)
        path = tmp_path / "checkpoint.json"
        cp.save(path)

        loaded = harness.Checkpoint.load(path)
        assert loaded.phase1_done is True
        assert loaded.phase2_runs_completed == 2

    def test_cost_tracking(self, tmp_path, tmp_project, scenarios_dir):
        """Verify costs accumulate from ResultMessage.total_cost_usd."""
        mock_sdk.load_scenarios(scenarios_dir)
        output_dir = tmp_path / "output"
        output_dir.mkdir()

        cost_tracker = harness.CostTracker()

        with patch("subprocess.run", side_effect=_mock_subprocess):
            asyncio.run(
                harness.phase1_extract(
                    project_dir=tmp_project,
                    output_dir=output_dir,
                    ls_dir=THIS_DIR,
                    rounds=1,
                    cost_tracker=cost_tracker,
                    sdk=mock_sdk,
                )
            )

        assert cost_tracker.total_cost > 0, "Must track costs from agent calls"
        assert len(cost_tracker.calls) >= 5, (
            "Must record at least explorer + architect + mapper + validator + compliance"
        )

    def test_cost_save(self, tmp_path):
        """Verify cost tracker serializes to JSON."""
        ct = harness.CostTracker()
        ct.record("phase1", 1.5, 1000, 5)
        ct.record("phase2", 2.0, 2000, 10)
        path = tmp_path / "costs.json"
        ct.save(path)

        data = json.loads(path.read_text())
        assert data["total_cost"] == 3.5
        assert len(data["calls"]) == 2


class TestMultiRunAggregation:
    """Test multi-run aggregation."""

    def test_comparative_report_generation(self, tmp_path):
        """Verify report is generated with mean/stddev."""
        run_results = [
            {
                "objective": {
                    "with_spec": {
                        "tests_passed": 10,
                        "lint_warnings": 2,
                        "lines_added": 100,
                        "lines_removed": 5,
                        "files_changed": 3,
                        "test_functions": 5,
                    },
                    "without_spec": {
                        "tests_passed": 8,
                        "lint_warnings": 5,
                        "lines_added": 80,
                        "lines_removed": 3,
                        "files_changed": 2,
                        "test_functions": 3,
                    },
                    "delta": {},
                },
                "code_quality": {"score_with": 8, "score_without": 6},
                "security": {"score_with": 7, "score_without": 7},
                "architecture": {"score_with": 9, "score_without": 6},
                "plan_quality": {
                    "plan_valid_xml": True,
                    "items_total": 5,
                    "items_completed": 5,
                    "witnesses_total": 3,
                    "witnesses_passed": 3,
                    "score": 9,
                },
                "spec_alignment": {"coverage_complete": True},
            },
            {
                "objective": {
                    "with_spec": {
                        "tests_passed": 12,
                        "lint_warnings": 1,
                        "lines_added": 110,
                        "lines_removed": 8,
                        "files_changed": 4,
                        "test_functions": 6,
                    },
                    "without_spec": {
                        "tests_passed": 7,
                        "lint_warnings": 4,
                        "lines_added": 90,
                        "lines_removed": 4,
                        "files_changed": 3,
                        "test_functions": 4,
                    },
                    "delta": {},
                },
                "code_quality": {"score_with": 9, "score_without": 7},
                "security": {"score_with": 8, "score_without": 6},
                "architecture": {"score_with": 8, "score_without": 7},
                "plan_quality": {
                    "plan_valid_xml": True,
                    "items_total": 6,
                    "items_completed": 5,
                    "witnesses_total": 4,
                    "witnesses_passed": 3,
                    "score": 8,
                },
                "spec_alignment": {"coverage_complete": True},
            },
        ]

        output_dir = tmp_path / "output"
        (output_dir / "phase3_analysis").mkdir(parents=True)
        cost_tracker = harness.CostTracker()
        cost_tracker.record("phase1", 5.0)
        cost_tracker.record("phase2", 3.0)
        cost_tracker.record("phase3", 1.0)

        report = harness.generate_comparative_report(
            run_results, output_dir, cost_tracker
        )
        assert "Cognitive Layers" in report
        assert "Plan Quality" in report
        assert "Cost Summary" in report
        report_path = output_dir / "phase3_analysis" / "aggregate_report.md"
        assert report_path.exists()


class TestHelpers:
    """Test helper functions."""

    def test_compute_code_metrics(self):
        """Verify code metrics computation from diff."""
        diff = (
            "diff --git a/src/limiter.py b/src/limiter.py\n"
            "+def test_bucket():\n"
            "+    pass\n"
            "+def rate_limit():\n"
            "-old_function()\n"
            "diff --git a/tests/test_limiter.py b/tests/test_limiter.py\n"
            "+def test_rate_limit():\n"
            "+    assert True\n"
        )
        metrics = harness.compute_code_metrics(diff)
        assert metrics["lines_added"] == 5
        assert metrics["lines_removed"] == 1
        assert metrics["files_changed"] == 2
        assert metrics["test_functions"] >= 2

    def test_detect_project_language(self, tmp_path):
        """Verify language detection."""
        (tmp_path / "pyproject.toml").write_text("[project]")
        assert harness.detect_project_language(tmp_path) == "python"

        js_dir = tmp_path / "js"
        js_dir.mkdir()
        (js_dir / "package.json").write_text("{}")
        assert harness.detect_project_language(js_dir) == "javascript"

    def test_collect_messages(self):
        """Verify collect_messages drains stream correctly."""

        async def fake_stream():
            yield mock_sdk.AssistantMessage(content=[mock_sdk.TextBlock(text="hello")])
            yield mock_sdk.ResultMessage(total_cost_usd=1.0)

        msgs, result = asyncio.run(
            harness.collect_messages(fake_stream(), sdk=mock_sdk)
        )
        assert len(msgs) == 2
        assert isinstance(result, mock_sdk.ResultMessage)
        assert result.total_cost_usd == 1.0


class TestConfigLoading:
    """Test YAML config file loading and CLI merge."""

    def test_config_yaml_loaded(self, tmp_path):
        """Verify YAML config is parsed and merged with defaults."""
        config = {
            "repo": "https://github.com/example/repo",
            "rounds": 3,
            "runs": 5,
            "extraction_model": "claude-opus-4-6",
            "with_spec_model": "claude-haiku-4-5-20251001",
            "without_spec_model": "claude-opus-4-6",
            "feature": "Build a thing",
        }
        config_path = tmp_path / "test-config.yaml"
        try:
            import yaml

            config_path.write_text(yaml.dump(config))
        except ImportError:
            config_path.write_text(json.dumps(config))  # yaml not available in test env
            pytest.skip("pyyaml not available")

        # Verify config can be loaded (we test the parse, not full main())
        loaded = __import__("yaml").safe_load(config_path.read_text())
        assert loaded["repo"] == "https://github.com/example/repo"
        assert loaded["rounds"] == 3
        assert loaded["runs"] == 5
        assert loaded["extraction_model"] == "claude-opus-4-6"
        assert loaded["with_spec_model"] == "claude-haiku-4-5-20251001"

    def test_per_phase_models_in_source(self):
        """Verify harness supports per-phase model configuration."""
        source = (THIS_DIR / "clayers-harness.py").read_text()
        assert "extraction_model" in source, "Must support extraction_model config"
        assert "with_spec_model" in source, "Must support with_spec_model config"
        assert "without_spec_model" in source, "Must support without_spec_model config"
        assert "analysis_model" in source, "Must support analysis_model config"

    def test_phase2_respects_per_phase_models(
        self, tmp_path, tmp_project, scenarios_dir
    ):
        """Verify Phase 2 passes different models to with-spec and without-spec sessions."""
        mock_sdk.load_scenarios(scenarios_dir)
        output_dir = tmp_path / "output"
        output_dir.mkdir()

        spec_dir = tmp_path / "spec"
        spec_dir.mkdir()
        (spec_dir / "index.xml").write_text("<index/>")

        with patch("subprocess.run", side_effect=_mock_subprocess):
            with patch.object(harness, "create_git_diff", return_value=""):
                asyncio.run(
                    harness.phase2_develop(
                        project_dir=tmp_project,
                        spec_dir=spec_dir,
                        feature_task="Implement something",
                        output_dir=output_dir,
                        with_spec_model="claude-sonnet-4-6",
                        without_spec_model="claude-opus-4-6",
                        cost_tracker=harness.CostTracker(),
                        sdk=mock_sdk,
                    )
                )

        with_spec_calls = [
            c
            for c in mock_sdk.call_log
            if c["options"]
            and c["options"].system_prompt
            and "cognitive layers specification"
            in (c["options"].system_prompt or "").lower()
        ]
        # Session B has "expert developer" but NOT "cognitive layers specification"
        without_spec_calls = [
            c
            for c in mock_sdk.call_log
            if c["options"]
            and c["options"].system_prompt
            and "expert developer" in (c["options"].system_prompt or "").lower()
            and "cognitive layers specification"
            not in (c["options"].system_prompt or "").lower()
        ]
        assert len(with_spec_calls) >= 1
        assert len(without_spec_calls) >= 1
        # Verify models are different
        assert with_spec_calls[0]["options"].model == "claude-sonnet-4-6"
        assert without_spec_calls[0]["options"].model == "claude-opus-4-6"


class TestSummarize:
    """Test summarize and battery features."""

    def test_summarize_saves_markdown(self, tmp_path):
        """Verify summarize_results saves analysis as markdown file."""
        # Create a minimal result directory
        output_dir = tmp_path / "results"
        output_dir.mkdir()
        (output_dir / "costs.json").write_text(
            json.dumps({"total_cost": 5.0, "calls": [{"phase": "phase1", "cost": 5.0}]})
        )

        # Add a summarize scenario
        sdir = tmp_path / "summarize_scenarios"
        sdir.mkdir()
        (sdir / "summarize.json").write_text(
            json.dumps(
                {
                    "match": {"prompt_contains": "Analyze these A/B test results"},
                    "responses": [
                        {
                            "type": "assistant",
                            "text": "# Analysis\n\nThe with-spec session won.",
                        },
                        {
                            "type": "result",
                            "total_cost_usd": 0.10,
                            "duration_ms": 500,
                            "num_turns": 1,
                        },
                    ],
                }
            )
        )
        mock_sdk.reset()
        mock_sdk.load_scenarios(sdir)

        asyncio.run(harness.summarize_results(output_dir, sdk=mock_sdk))

        summary_path = output_dir / "summary.md"
        assert summary_path.exists(), "summarize_results must save summary.md"
        content = summary_path.read_text()
        assert "Analysis" in content

    def test_battery_config_parsed(self, tmp_path):
        """Verify battery YAML config is parseable."""
        battery_config = {
            "name": "test-battery",
            "experiments": [
                {"name": "exp1", "config": "config1.yaml"},
                {"name": "exp2", "config": "config2.yaml"},
            ],
        }
        try:
            import yaml

            battery_path = tmp_path / "battery.yaml"
            battery_path.write_text(yaml.dump(battery_config))
            loaded = yaml.safe_load(battery_path.read_text())
            assert loaded["name"] == "test-battery"
            assert len(loaded["experiments"]) == 2
        except ImportError:
            pytest.skip("pyyaml not available")

    def test_summarize_battery_saves_markdown(self, tmp_path):
        """Verify summarize_battery saves cross_experiment_analysis.md."""
        battery_dir = tmp_path / "battery"
        battery_dir.mkdir()

        # Add analysis scenario
        sdir = tmp_path / "battery_scenarios"
        sdir.mkdir()
        (sdir / "battery_analysis.json").write_text(
            json.dumps(
                {
                    "match": {"prompt_contains": "cross-experiment findings"},
                    "responses": [
                        {
                            "type": "assistant",
                            "text": "# Cross-Experiment Analysis\n\nSpecs help consistently.",
                        },
                        {
                            "type": "result",
                            "total_cost_usd": 0.15,
                            "duration_ms": 600,
                            "num_turns": 1,
                        },
                    ],
                }
            )
        )
        mock_sdk.reset()
        mock_sdk.load_scenarios(sdir)

        experiments = [
            {
                "name": "exp1",
                "config": "/tmp/c.yaml",
                "output": str(battery_dir / "exp1"),
                "status": "ok",
            },
        ]
        # Create minimal experiment output
        (battery_dir / "exp1").mkdir()

        asyncio.run(harness.summarize_battery(battery_dir, experiments, sdk=mock_sdk))

        analysis_path = battery_dir / "cross_experiment_analysis.md"
        assert analysis_path.exists(), (
            "summarize_battery must save cross_experiment_analysis.md"
        )


if __name__ == "__main__":
    sys.exit(subprocess.call(["pytest", __file__, "-v"]))
