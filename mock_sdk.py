"""
Mock SDK module implementing the same interface as claude_agent_sdk.

Scenario-driven: tests write JSON scenario files that define what the mock
returns for each call matching specific patterns.

Usage:
    import mock_sdk
    mock_sdk.reset()
    mock_sdk.load_scenarios(Path("scenarios/"))
    async for msg in mock_sdk.query(prompt="...", options=mock_sdk.ClaudeAgentOptions(...)):
        ...
    assert len(mock_sdk.call_log) == 1
"""

from __future__ import annotations

import json
import re
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, AsyncIterator


# ---------------------------------------------------------------------------
# Message types (mirrors claude_agent_sdk)
# ---------------------------------------------------------------------------


@dataclass
class TextBlock:
    text: str


@dataclass
class ToolUseBlock:
    id: str
    name: str
    input: dict[str, Any]


@dataclass
class ToolResultBlock:
    tool_use_id: str
    content: str | list[dict[str, Any]] | None = None
    is_error: bool | None = None


@dataclass
class AssistantMessage:
    content: list[TextBlock | ToolUseBlock]
    model: str = "mock-model"
    parent_tool_use_id: str | None = None
    error: Any = None


@dataclass
class UserMessage:
    content: str | list
    uuid: str | None = None
    parent_tool_use_id: str | None = None


@dataclass
class SystemMessage:
    subtype: str
    data: dict[str, Any] = field(default_factory=dict)


@dataclass
class ResultMessage:
    subtype: str = "result"
    duration_ms: int = 1000
    duration_api_ms: int = 800
    is_error: bool = False
    num_turns: int = 5
    session_id: str = "mock-session"
    total_cost_usd: float | None = 0.50
    usage: dict[str, Any] | None = None
    result: str | None = None
    structured_output: Any = None


@dataclass
class ClaudeAgentOptions:
    system_prompt: str | None = None
    cwd: str | None = None
    permission_mode: str | None = None
    model: str | None = None
    max_turns: int | None = None
    max_budget_usd: float | None = None
    output_format: dict[str, Any] | None = None
    tools: list[str] | None = None
    allowed_tools: list[str] | None = None
    mcp_servers: Any = None
    continue_conversation: bool = False
    resume: str | None = None
    disallowed_tools: list[str] | None = None
    fallback_model: str | None = None
    betas: list[str] | None = None
    env: dict[str, str] | None = None
    extra_args: dict[str, str | None] | None = None


# ---------------------------------------------------------------------------
# Scenario engine
# ---------------------------------------------------------------------------


@dataclass
class Scenario:
    match: dict[str, str]  # matching criteria
    responses: list[dict]  # sequence of responses to yield


# Module-level state
call_log: list[dict] = []
_scenarios: list[Scenario] = []
_default_response_text: str = "OK - mock response"


def load_scenarios(directory: Path):
    """Load scenario files from a directory."""
    global _scenarios
    if not directory.exists():
        return
    for f in sorted(directory.glob("*.json")):
        data = json.loads(f.read_text())
        # Support both single scenario and list of scenarios
        if isinstance(data, list):
            for item in data:
                _scenarios.append(
                    Scenario(match=item["match"], responses=item["responses"])
                )
        else:
            _scenarios.append(
                Scenario(match=data["match"], responses=data["responses"])
            )


def load_scenario(scenario_data: dict):
    """Load a single scenario from a dict."""
    _scenarios.append(
        Scenario(match=scenario_data["match"], responses=scenario_data["responses"])
    )


def set_default_response(text: str):
    """Set the default response text when no scenario matches."""
    global _default_response_text
    _default_response_text = text


def reset():
    """Clear call_log and scenarios."""
    global call_log, _scenarios, _default_response_text
    call_log = []
    _scenarios = []
    _default_response_text = "OK - mock response"


def _find_matching_scenario(
    options: ClaudeAgentOptions | None, prompt: str
) -> Scenario | None:
    """Find a scenario whose match criteria fit the current call."""
    for scenario in _scenarios:
        match_criteria = scenario.match
        matched = True

        if "system_prompt_contains" in match_criteria:
            sp = (options.system_prompt or "") if options else ""
            if match_criteria["system_prompt_contains"].lower() not in sp.lower():
                matched = False

        if "prompt_contains" in match_criteria:
            if match_criteria["prompt_contains"].lower() not in prompt.lower():
                matched = False

        if "prompt_regex" in match_criteria:
            if not re.search(match_criteria["prompt_regex"], prompt, re.IGNORECASE):
                matched = False

        if matched:
            return scenario

    return None


def _build_responses(scenario: Scenario | None) -> list:
    """Build response objects from a scenario (or defaults)."""
    if scenario:
        result = []
        for response in scenario.responses:
            rtype = response.get("type", "assistant")
            if rtype == "assistant":
                text = response.get("text", _default_response_text)
                result.append(AssistantMessage(content=[TextBlock(text=text)]))
            elif rtype == "result":
                result.append(
                    ResultMessage(
                        total_cost_usd=response.get("total_cost_usd", 0.50),
                        duration_ms=response.get("duration_ms", 1000),
                        num_turns=response.get("num_turns", 5),
                        session_id=response.get("session_id", "mock-session"),
                    )
                )
        return result
    else:
        return [
            AssistantMessage(content=[TextBlock(text=_default_response_text)]),
            ResultMessage(total_cost_usd=0.10, duration_ms=500, num_turns=1),
        ]


async def query(
    *,
    prompt: str,
    options: ClaudeAgentOptions | None = None,
    transport: Any = None,
) -> AsyncIterator:
    """Mock query that records calls and yields scenario-driven responses."""
    call_log.append(
        {
            "prompt": prompt,
            "options": options,
        }
    )

    scenario = _find_matching_scenario(options, prompt)
    for msg in _build_responses(scenario):
        yield msg


# ---------------------------------------------------------------------------
# ClaudeSDKClient (persistent session mock)
# ---------------------------------------------------------------------------


class ClaudeSDKClient:
    """Mock ClaudeSDKClient matching the real SDK interface.

    Records queries to the module-level call_log, just like query().
    Responses come from the same scenario-matching engine.
    """

    def __init__(
        self,
        options: ClaudeAgentOptions | None = None,
        transport: Any = None,
    ):
        self.options = options or ClaudeAgentOptions()
        self._responses: list = []

    async def connect(self, prompt: str | None = None) -> None:
        pass

    async def query(self, prompt: str, session_id: str = "default") -> None:
        call_log.append(
            {
                "prompt": prompt,
                "options": self.options,
            }
        )
        scenario = _find_matching_scenario(self.options, prompt)
        self._responses = _build_responses(scenario)

    async def receive_response(self) -> AsyncIterator:
        for msg in self._responses:
            yield msg

    async def disconnect(self) -> None:
        pass

    async def __aenter__(self) -> "ClaudeSDKClient":
        await self.connect()
        return self

    async def __aexit__(self, *args) -> bool:
        await self.disconnect()
        return False
