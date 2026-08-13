"""Native Runtime acceptance runner for authored Copilot normal cases."""

from __future__ import annotations

import json
import os
import subprocess
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from .app_server_client import AppServerClient, AppServerClientError
from .copilot_manifest import CopilotTestCase, load_copilot_test_cases
from .dev_profile import (
    CopilotDevError,
    DevComposition,
    PreparedDevProfile,
    load_dev_composition,
    prepare_dev_profile,
    resolve_tool_setup,
    validate_workspace,
)
from .mock_responses import MockResponsesFixture


class CopilotTestError(RuntimeError):
    def __init__(
        self,
        code: str,
        stage: str,
        message: str,
        diagnostics: dict[str, Any] | None = None,
    ) -> None:
        self.code = code
        self.stage = stage
        self.message = message
        self.diagnostics = diagnostics or {}
        super().__init__(f"{code}: {stage}: {message}")

    @property
    def public_message(self) -> str:
        if self.code == "TestTimedOut":
            return "native acceptance did not reach a terminal result before timeout"
        return "native acceptance failed; inspect local diagnostics"


@dataclass(frozen=True)
class TestEvidence:
    supervisor_skill: str
    agent: str
    tool: str
    tool_name: str
    mcp_completed: bool
    arguments_matched: bool
    structured_content_matched: bool
    child_turn_completed: bool
    root_final_after_child: bool
    root_turn_completed: bool


def run_copilot_tests(
    source_root: Path,
    manifest_path: Path,
    workspace: Path,
    codex_bin: Path,
    *,
    timeout_seconds: float = 45.0,
) -> dict[str, Any]:
    try:
        composition = load_dev_composition(source_root, manifest_path)
    except CopilotDevError as error:
        raise _test_error_from_dev(error) from error
    tests = load_copilot_test_cases(source_root, manifest_path)
    if not tests:
        raise CopilotTestError(
            "TestDefinitionInvalid", "manifest", "manifest must declare at least one [[tests]] case"
        )
    if len(tests) != 1:
        raise CopilotTestError(
            "TestDefinitionInvalid", "manifest", "the current acceptance runner supports exactly one test case"
        )
    try:
        canonical_workspace = validate_workspace(workspace)
        prepared = prepare_dev_profile(composition)
    except CopilotDevError as error:
        raise _test_error_from_dev(error) from error
    started = time.monotonic()
    try:
        return _run_case(
            composition,
            tests[0],
            prepared,
            canonical_workspace,
            codex_bin,
            timeout_seconds=timeout_seconds,
            started=started,
        )
    finally:
        prepared.cleanup()


def _run_case(
    composition: DevComposition,
    case: CopilotTestCase,
    prepared: PreparedDevProfile,
    workspace: Path,
    codex_bin: Path,
    *,
    timeout_seconds: float,
    started: float,
) -> dict[str, Any]:
    environment = {"OPEN_WEB_CODEX_DATA_DIR": str(prepared.process_data)}
    for tool in composition.tools:
        try:
            setup = resolve_tool_setup(composition, tool)
        except CopilotDevError as error:
            raise _test_error_from_dev(error) from error
        try:
            completed = subprocess.run(
                [str(setup)],
                cwd=tool.source,
                env={**os.environ, **environment},
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
                timeout=120,
            )
        except (OSError, subprocess.TimeoutExpired) as error:
            raise CopilotTestError(
                "EnvironmentUnavailable", "tool-setup", "Tool environment setup could not complete"
            ) from error
        if completed.returncode != 0:
            raise CopilotTestError(
                "EnvironmentUnavailable",
                "tool-setup",
                f"Tool environment setup exited with status {completed.returncode}",
            )

    supervisor_path = (
        prepared.profile_root / "skills" / composition.supervisor_skill / "SKILL.md"
    ).resolve(strict=True)
    supervisor_marker = supervisor_path.read_text(encoding="utf-8")
    child_prompt = "Run the declared MCP tool once and return its exact structured result."
    with MockResponsesFixture(
        agent=case.agent,
        tool=case.tool,
        tool_name=case.tool_name,
        arguments=case.arguments,
        supervisor_marker=supervisor_marker,
        root_prompt=case.prompt,
        child_prompt=child_prompt,
        child_task_name="acceptance_worker",
    ) as mock:
        _write_test_config(prepared.profile_root, mock.base_url)
        client: AppServerClient | None = None
        try:
            client = AppServerClient.launch(
                codex_bin,
                profile_root=prepared.profile_root,
                process_home=prepared.process_home,
                process_cwd=prepared.process_cwd,
                environment=environment,
                timeout_seconds=timeout_seconds,
            )
            client.initialize()
            skills = client.request("skills/list", {"cwds": [str(workspace)], "forceReload": True})
            _require_skill(skills, composition.supervisor_skill)
            selected_roots = [
                {
                    "id": tool.id,
                    "location": {
                        "type": "environment",
                        "environmentId": "local",
                        "path": str(tool.source),
                    },
                }
                for tool in composition.tools
            ]
            thread = client.request(
                "thread/start",
                {
                    "model": "copilot-test-model",
                    "modelProvider": "copilot_test",
                    "cwd": str(workspace),
                    "ephemeral": True,
                    "approvalPolicy": "never",
                    "sandbox": "read-only",
                    "environments": [
                        {
                            "environmentId": "local",
                            "cwd": str(workspace),
                            "runtimeWorkspaceRoots": [str(workspace)],
                        }
                    ],
                    "selectedCapabilityRoots": selected_roots,
                },
            )
            root_thread_id = _thread_id(thread)
            inventory = client.request(
                "mcpServerStatus/list",
                {"threadId": root_thread_id, "detail": "toolsAndAuthOnly", "limit": 100},
            )
            _require_tool(inventory, case.tool, case.tool_name)
            turn = client.request(
                "turn/start",
                {
                    "threadId": root_thread_id,
                    "input": [
                        {"type": "text", "text": case.prompt, "textElements": []},
                        {
                            "type": "skill",
                            "name": composition.supervisor_skill,
                            "path": str(supervisor_path),
                        },
                    ],
                },
            )
            root_turn = turn.get("turn")
            root_turn_id = root_turn.get("id") if isinstance(root_turn, dict) else None
            if not isinstance(root_turn_id, str):
                raise CopilotTestError("RuntimeTestFailed", "turn/start", "Runtime omitted root turn identity")
            try:
                evidence = _collect_evidence(
                    client,
                    case,
                    mock,
                    root_thread_id=root_thread_id,
                    root_turn_id=root_turn_id,
                    supervisor_skill=composition.supervisor_skill,
                    deadline=started + timeout_seconds,
                )
            except CopilotTestError as error:
                role_warnings = [
                    line[:300]
                    for line in client.stderr.splitlines()
                    if "agent role" in line.lower() or "malformed" in line.lower()
                ][:8]
                raise CopilotTestError(
                    error.code,
                    error.stage,
                    error.message,
                    {
                        "requests": mock.request_classifications[:8],
                        "roleWarnings": role_warnings,
                        **error.diagnostics,
                    },
                ) from error
        except AppServerClientError as error:
            code = "TestTimedOut" if "timed out" in error.message else "RuntimeTestFailed"
            raise CopilotTestError(code, "app-server", error.message) from error
        finally:
            if client is not None:
                client.close()

        if mock.first_root_request_has_supervisor_marker is not True:
            raise CopilotTestError(
                "RuntimeTestFailed",
                "supervisor",
                "Supervisor Skill body was absent from the first root model request",
            )
        duration_ms = round((time.monotonic() - started) * 1000)
        return {
            "ok": True,
            "state": "test_passed",
            "copilot": {
                "id": composition.summary.id,
                "compositionDescriptorSha256": (
                    composition.summary.composition_descriptor_sha256
                ),
            },
            "provider": {
                "id": "copilot_test",
                "model": "copilot-test-model",
                "mode": "local_responses_fixture",
            },
            "durationMs": duration_ms,
            "tests": [
                {
                    "id": case.id,
                    "state": "passed",
                    "evidence": _public_evidence(evidence),
                }
            ],
        }


def _public_evidence(evidence: TestEvidence) -> dict[str, Any]:
    return {
        "supervisorSkill": evidence.supervisor_skill,
        "agent": evidence.agent,
        "mcp": {
            "server": evidence.tool,
            "tool": evidence.tool_name,
            "completed": evidence.mcp_completed,
            "argumentsMatched": evidence.arguments_matched,
            "structuredContentMatched": evidence.structured_content_matched,
        },
        "childTurnCompleted": evidence.child_turn_completed,
        "rootFinalAfterChild": evidence.root_final_after_child,
        "rootTurnCompleted": evidence.root_turn_completed,
    }


def _test_error_from_dev(error: CopilotDevError) -> CopilotTestError:
    return CopilotTestError(
        error.code,
        error.stage,
        "native acceptance environment preparation failed",
        {"path": error.path, "cause": error.cause},
    )


def _write_test_config(profile: Path, base_url: str) -> None:
    (profile / "config.toml").write_text(
        f'''model = "copilot-test-model"
model_provider = "copilot_test"
approval_policy = "never"
sandbox_mode = "read-only"

[features]
multi_agent_v2 = true
plugins = true

[skills]
include_instructions = true

[model_providers.copilot_test]
name = "Copilot acceptance fixture"
base_url = "{base_url}"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0
''',
        encoding="utf-8",
    )


def _collect_evidence(
    client: AppServerClient,
    case: CopilotTestCase,
    mock: MockResponsesFixture,
    *,
    root_thread_id: str,
    root_turn_id: str,
    supervisor_skill: str,
    deadline: float,
) -> TestEvidence:
    child_thread_id: str | None = None
    spawn_activities: list[dict[str, Any]] = []
    mcp_items: list[dict[str, Any]] = []
    child_completed_index: int | None = None
    root_final_index: int | None = None
    root_completed = False
    sequence = 0
    observed: list[str] = []
    while time.monotonic() < deadline and not (root_completed and root_final_index is not None):
        sequence += 1
        try:
            notification = client.next_notification(
                timeout_seconds=max(0.1, deadline - time.monotonic())
            )
        except AppServerClientError as error:
            classifications = mock.request_classifications[:8]
            raise CopilotTestError(
                "TestTimedOut",
                "app-server",
                f"native acceptance timed out; requests={classifications}; notifications={observed[:24]}",
                {"requests": classifications, "notifications": observed[:24]},
            ) from error
        method = notification.get("method")
        params = notification.get("params")
        if not isinstance(params, dict):
            continue
        if len(observed) < 80:
            item = params.get("item")
            item_type = item.get("type") if isinstance(item, dict) else None
            item_tool = item.get("tool") if isinstance(item, dict) else None
            item_status = item.get("status") if isinstance(item, dict) else None
            turn = params.get("turn")
            turn_status = turn.get("status") if isinstance(turn, dict) else None
            observed.append(
                f"{method}:{item_type or '-'}:{item_tool or '-'}:{item_status or turn_status or '-'}"
            )
        if method == "thread/started":
            thread = params.get("thread")
            if isinstance(thread, dict) and thread.get("agentRole") == case.agent:
                child_thread_id = thread.get("id")
        elif method == "thread/status/changed":
            # Native sub-agent execution is intentionally not subscribed as a normal user
            # thread. Its canonical identity arrives on spawn completion.
            pass
        elif method == "item/completed":
            item = params.get("item")
            if not isinstance(item, dict):
                continue
            if item.get("type") == "subAgentActivity" and item.get("kind") == "started":
                candidate = item.get("agentThreadId")
                if isinstance(candidate, str):
                    child_thread_id = candidate
                    spawn_activities.append(item)
            elif item.get("type") == "mcpToolCall":
                if child_thread_id is not None and params.get("threadId") == child_thread_id:
                    mcp_items.append(item)
            elif (
                item.get("type") == "agentMessage"
                and params.get("threadId") == root_thread_id
                and params.get("turnId") == root_turn_id
            ):
                root_final_index = sequence
        elif method == "turn/completed":
            thread_id = params.get("threadId")
            turn = params.get("turn")
            turn_id = turn.get("id") if isinstance(turn, dict) else None
            turn_status = turn.get("status") if isinstance(turn, dict) else None
            if (
                child_thread_id is not None
                and thread_id == child_thread_id
                and turn_status == "completed"
            ):
                child_completed_index = sequence
            if (
                thread_id == root_thread_id
                and turn_id == root_turn_id
                and turn_status == "completed"
            ):
                root_completed = True

    if child_thread_id is None:
        raise CopilotTestError(
            "RuntimeTestFailed",
            "agent",
            f"declared child was not observed; notifications={observed}",
        )
    if len(spawn_activities) != 1:
        raise CopilotTestError(
            "RuntimeTestFailed",
            "agent",
            f"expected exactly one native child start; activities={spawn_activities}; notifications={observed}",
        )
    child_read = client.request(
        "thread/read", {"threadId": child_thread_id, "includeTurns": False}
    )
    child_thread = child_read.get("thread")
    if (
        not isinstance(child_thread, dict)
        or child_thread.get("agentRole") != case.agent
        or child_thread.get("parentThreadId") != root_thread_id
    ):
        raise CopilotTestError("RuntimeTestFailed", "agent", "spawned child did not use the declared Role")
    matching = [
        item
        for item in mcp_items
        if item.get("server") == case.tool and item.get("tool") == case.tool_name
    ]
    if len(matching) != 1:
        raise CopilotTestError(
            "RuntimeTestFailed",
            "mcp",
            f"expected exactly one declared MCP Tool call; mcp_items={mcp_items}; notifications={observed}",
        )
    item = matching[0]
    result = item.get("result")
    structured = result.get("structuredContent") if isinstance(result, dict) else None
    if item.get("status") != "completed" or item.get("arguments") != case.arguments:
        raise CopilotTestError("RuntimeTestFailed", "mcp", "MCP Tool call did not complete with exact arguments")
    if structured != case.expected_structured_content:
        raise CopilotTestError("RuntimeTestFailed", "mcp", "MCP structured content did not match expectation")
    if child_completed_index is None:
        raise CopilotTestError("RuntimeTestFailed", "agent", "child turn did not complete")
    if root_final_index is None or root_final_index <= child_completed_index:
        raise CopilotTestError("RuntimeTestFailed", "root", "Root final message did not follow child completion")
    if not root_completed:
        raise CopilotTestError("RuntimeTestFailed", "root", "Root turn did not complete")
    return TestEvidence(
        supervisor_skill=supervisor_skill,
        agent=case.agent,
        tool=case.tool,
        tool_name=case.tool_name,
        mcp_completed=True,
        arguments_matched=True,
        structured_content_matched=True,
        child_turn_completed=True,
        root_final_after_child=True,
        root_turn_completed=True,
    )


def _require_skill(result: dict[str, Any], name: str) -> None:
    entries = result.get("data")
    if isinstance(entries, list) and any(
        isinstance(entry, dict) and bool(entry.get("errors")) for entry in entries
    ):
        raise CopilotTestError(
            "RuntimeTestFailed", "skills/list", "Runtime reported Skill discovery errors"
        )
    if not isinstance(entries, list) or not any(
        isinstance(entry, dict)
        and isinstance(entry.get("skills"), list)
        and any(isinstance(skill, dict) and skill.get("name") == name for skill in entry["skills"])
        for entry in entries
    ):
        raise CopilotTestError("RuntimeTestFailed", "skills/list", f"Runtime did not discover {name!r}")


def _require_tool(result: dict[str, Any], server: str, tool: str) -> None:
    entries = result.get("data")
    if not isinstance(entries, list) or not any(
        isinstance(entry, dict)
        and entry.get("name") == server
        and isinstance(entry.get("tools"), dict)
        and tool in entry["tools"]
        for entry in entries
    ):
        raise CopilotTestError("RuntimeTestFailed", "mcpServerStatus/list", f"Runtime did not expose {server}.{tool}")


def _thread_id(result: dict[str, Any]) -> str:
    thread = result.get("thread")
    thread_id = thread.get("id") if isinstance(thread, dict) else None
    if not isinstance(thread_id, str) or not thread_id:
        raise CopilotTestError("RuntimeTestFailed", "thread/start", "Runtime omitted thread identity")
    return thread_id
