"""Native Runtime acceptance runner for authored Copilot normal cases."""

from __future__ import annotations

import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import tomllib

from .app_server_client import AppServerClient, AppServerClientError
from .copilot_manifest import CopilotTestCase, load_copilot_test_cases
from .dev_profile import (
    CopilotDevError,
    DevComposition,
    PreparedDevProfile,
    load_dev_composition,
    prepare_dev_profile,
    prepare_dev_tool_composition,
    validate_workspace,
)
from .mock_responses import MockResponsesFixture
from .tool_environment import MaterializedToolComposition

_PUBLIC_ERRORS: dict[str, tuple[str, str]] = {
    "manifest_invalid": (
        "The acceptance test definition is invalid.",
        "Fix the declared [[tests]] case and run validation again.",
    ),
    "runtime_unavailable": (
        "The Codex Runtime acceptance environment is unavailable.",
        "Check the Codex executable and prepared Tool environment, then retry.",
    ),
    "skill_not_discovered": (
        "The declared Root Skill was not discovered.",
        "Run copilot validate and copilot dev, then fix the Skill declaration.",
    ),
    "agent_not_spawned": (
        "The declared Agent target was not started with the expected Role.",
        "Check the target Role and Root delegation instructions.",
    ),
    "tool_not_available": (
        "The declared MCP Tool is not available to the test target.",
        "Check the Tool runtime and the target Role MCP policy.",
    ),
    "tool_not_called": (
        "The declared MCP Tool did not complete exactly once.",
        "Check the prompt, target Skill, and Tool availability.",
    ),
    "arguments_mismatch": (
        "The MCP Tool call arguments did not match the test case.",
        "Align the test arguments with the target instructions.",
    ),
    "result_mismatch": (
        "The MCP Tool structured result did not match the expectation.",
        "Update the Tool or the expected structured_content.",
    ),
    "terminal_missing": (
        "The native Agent or Root turn did not reach the required terminal order.",
        "Check Agent completion, Root wait, and final-answer behavior.",
    ),
    "timed_out": (
        "The native acceptance case did not finish before its timeout.",
        "Inspect the target Tool and Agent terminal behavior before increasing the timeout.",
    ),
}


class CopilotTestError(RuntimeError):
    def __init__(
        self,
        code: str,
        stage: str,
        message: str,
        *,
        test_id: str | None = None,
        diagnostics: dict[str, Any] | None = None,
    ) -> None:
        self.code = code
        self.stage = stage
        self.message = message
        self.test_id = test_id
        self.diagnostics = diagnostics or {}
        super().__init__(f"{code}: {stage}: {message}")

    @property
    def public_message(self) -> str:
        return _PUBLIC_ERRORS.get(
            self.code,
            (
                "The native acceptance case failed.",
                "Run the failing case directly and use its typed stage to locate the owner.",
            ),
        )[0]

    @property
    def next_action(self) -> str:
        return _PUBLIC_ERRORS.get(
            self.code,
            (
                "The native acceptance case failed.",
                "Run the failing case directly and use its typed stage to locate the owner.",
            ),
        )[1]


@dataclass(frozen=True)
class AcceptanceEvidence:
    supervisor_skill: str
    target_kind: str
    target_agent: str | None
    capability_root: str
    server: str
    tool_name: str
    mcp_completed: bool
    arguments_matched: bool
    structured_content_matched: bool
    target_turn_completed: bool
    root_final_after_target: bool
    root_turn_completed: bool


def run_copilot_tests(
    source_root: Path,
    manifest_path: Path,
    workspace: Path,
    codex_bin: Path,
    *,
    case_id: str | None = None,
    timeout_seconds: float = 45.0,
    tool_environment_root: Path | None = None,
    build_store_root: Path | None = None,
    tool_registry_root: Path | None = None,
) -> dict[str, Any]:
    if timeout_seconds <= 0 or timeout_seconds > 300:
        raise CopilotTestError(
            "manifest_invalid",
            "arguments",
            "timeout-seconds must be greater than 0 and at most 300",
            test_id=case_id,
        )
    try:
        composition = load_dev_composition(
            source_root,
            manifest_path,
            tool_registry_root=tool_registry_root,
        )
        tests = load_copilot_test_cases(
            source_root,
            manifest_path,
            tool_registry_root=tool_registry_root,
        )
        canonical_workspace = validate_workspace(workspace)
    except CopilotDevError as error:
        raise _test_error_from_dev(error, test_id=case_id) from error
    if not tests:
        raise CopilotTestError(
            "manifest_invalid",
            "manifest",
            "manifest must declare at least one [[tests]] case",
            test_id=case_id,
        )
    selected = tuple(case for case in tests if case_id is None or case.id == case_id)
    if case_id is not None and not selected:
        raise CopilotTestError(
            "manifest_invalid",
            "manifest",
            "selected acceptance case is not declared",
            test_id=case_id,
        )

    suite_started = time.monotonic()
    results: list[dict[str, Any]] = []
    for case in selected:
        case_started = time.monotonic()
        prepared: PreparedDevProfile | None = None
        try:
            try:
                prepared = prepare_dev_profile(composition)
                prepared_tools = prepare_dev_tool_composition(
                    prepared,
                    output_root=tool_environment_root,
                    build_store_root=build_store_root,
                )
            except CopilotDevError as error:
                raise _test_error_from_dev(error, test_id=case.id) from error
            evidence = _run_case(
                composition,
                case,
                prepared,
                canonical_workspace,
                codex_bin,
                prepared_tools,
                timeout_seconds=timeout_seconds,
            )
            results.append(
                {
                    "id": case.id,
                    "state": "passed",
                    "durationMs": round((time.monotonic() - case_started) * 1000),
                    "evidence": _public_evidence(evidence),
                }
            )
        except CopilotTestError as error:
            if error.test_id is not None:
                raise
            raise CopilotTestError(
                error.code,
                error.stage,
                error.message,
                test_id=case.id,
                diagnostics=error.diagnostics,
            ) from error
        finally:
            if prepared is not None:
                prepared.cleanup()

    return {
        "ok": True,
        "state": "test_passed",
        "copilot": {
            "id": composition.summary.id,
            "compositionDescriptorSha256": composition.summary.composition_descriptor_sha256,
        },
        "provider": {
            "id": "copilot_test",
            "model": "copilot-test-model",
            "mode": "local_responses_fixture",
        },
        "durationMs": round((time.monotonic() - suite_started) * 1000),
        "tests": results,
    }


def _run_case(
    composition: DevComposition,
    case: CopilotTestCase,
    prepared: PreparedDevProfile,
    workspace: Path,
    codex_bin: Path,
    prepared_tools: MaterializedToolComposition,
    *,
    timeout_seconds: float,
) -> AcceptanceEvidence:
    supervisor_path = (
        prepared.profile_root / "skills" / composition.root_skill / "SKILL.md"
    ).resolve(strict=True)
    root_config = _root_config_overrides(composition, prepared)
    child_prompt = "Run the declared MCP tool once and return its exact structured result."
    runtime_deadline = _runtime_deadline(timeout_seconds)
    with MockResponsesFixture(
        target_kind=case.target_kind,
        agent=case.target_agent,
        server=case.server,
        tool_name=case.tool_name,
        arguments=case.arguments,
        root_prompt=case.prompt,
        child_prompt=child_prompt,
        child_task_name="acceptance_worker",
        collaboration_namespace=(root_config.get("features.multi_agent_v2") is not False),
    ) as mock:
        _write_test_config(prepared.profile_root, mock.base_url)
        client: AppServerClient | None = None
        try:
            client = AppServerClient.launch(
                codex_bin,
                profile_root=prepared.profile_root,
                process_home=prepared.process_home,
                process_cwd=prepared.process_cwd,
                environment={},
                timeout_seconds=timeout_seconds,
            )
            client.initialize()
            skills = client.request(
                "skills/list", {"cwds": [str(workspace)], "forceReload": True}
            )
            _require_skill(skills, composition.root_skill, test_id=case.id)
            selected_roots = [
                {
                    "id": tool.id,
                    "location": {
                        "type": "environment",
                        "environmentId": "local",
                        "path": str(tool.projection_root),
                    },
                }
                for tool in prepared_tools.capability_roots
            ]
            start_params: dict[str, Any] = {
                "model": "copilot-test-model",
                "modelProvider": "copilot_test",
                "cwd": str(workspace),
                "ephemeral": False,
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
            }
            if root_config:
                start_params["config"] = root_config
            thread = client.request("thread/start", start_params)
            root_thread_id = _thread_id(thread, test_id=case.id)
            inventory = client.request(
                "mcpServerStatus/list",
                {"threadId": root_thread_id, "detail": "toolsAndAuthOnly", "limit": 100},
                timeout_seconds=max(0.1, runtime_deadline - time.monotonic()),
            )
            _require_tool(inventory, case.server, case.tool_name, test_id=case.id)
            turn = client.request(
                "turn/start",
                {
                    "threadId": root_thread_id,
                    "input": [
                        {"type": "text", "text": case.prompt, "textElements": []},
                        {
                            "type": "skill",
                            "name": composition.root_skill,
                            "path": str(supervisor_path),
                        },
                    ],
                },
            )
            root_turn = turn.get("turn")
            root_turn_id = root_turn.get("id") if isinstance(root_turn, dict) else None
            if not isinstance(root_turn_id, str) or not root_turn_id:
                raise CopilotTestError(
                    "terminal_missing",
                    "turn/start",
                    "Runtime omitted the Root turn identity",
                    test_id=case.id,
                )
            child_thread_id = _wait_for_root_terminal(
                client,
                case,
                root_thread_id=root_thread_id,
                root_turn_id=root_turn_id,
                deadline=runtime_deadline,
            )
            root_read = client.request(
                "thread/read",
                {"threadId": root_thread_id, "includeTurns": True},
                timeout_seconds=max(0.1, runtime_deadline - time.monotonic()),
            )
            if case.target_kind == "agent" and child_thread_id is None:
                child_thread_id = _child_id_from_root_history(root_read)
            child_read: dict[str, Any] | None = None
            if case.target_kind == "agent":
                if child_thread_id is None:
                    raise CopilotTestError(
                        "agent_not_spawned",
                        "agent",
                        "canonical Root history omitted the Agent target identity",
                        test_id=case.id,
                    )
                child_read = client.request(
                    "thread/read",
                    {"threadId": child_thread_id, "includeTurns": True},
                    timeout_seconds=max(0.1, runtime_deadline - time.monotonic()),
                )
            return _evidence_from_history(
                case,
                root_read=root_read,
                child_read=child_read,
                root_thread_id=root_thread_id,
                root_turn_id=root_turn_id,
                supervisor_skill=composition.root_skill,
            )
        except AppServerClientError as error:
            code = "timed_out" if "timed out" in error.message else "runtime_unavailable"
            raise CopilotTestError(
                code,
                "app-server",
                error.message,
                test_id=case.id,
            ) from error
        finally:
            if client is not None:
                client.close()


def _public_evidence(evidence: AcceptanceEvidence) -> dict[str, Any]:
    target: dict[str, Any] = {"kind": evidence.target_kind}
    if evidence.target_agent is not None:
        target["agent"] = evidence.target_agent
    return {
        "supervisorSkill": evidence.supervisor_skill,
        "target": target,
        "mcp": {
            "capabilityRoot": evidence.capability_root,
            "server": evidence.server,
            "toolName": evidence.tool_name,
            "completed": evidence.mcp_completed,
            "argumentsMatched": evidence.arguments_matched,
            "structuredContentMatched": evidence.structured_content_matched,
        },
        "targetTurnCompleted": evidence.target_turn_completed,
        "rootFinalAfterTarget": evidence.root_final_after_target,
        "rootTurnCompleted": evidence.root_turn_completed,
    }


def _runtime_deadline(timeout_seconds: float) -> float:
    """Start the Runtime deadline after bounded Tool setup has completed."""

    return time.monotonic() + timeout_seconds


def _test_error_from_dev(
    error: CopilotDevError, *, test_id: str | None = None
) -> CopilotTestError:
    return CopilotTestError(
        "runtime_unavailable",
        error.stage,
        "native acceptance environment preparation failed",
        test_id=test_id,
    )


def _write_test_config(profile: Path, base_url: str) -> None:
    (profile / "config.toml").write_text(
        f'''model = "copilot-test-model"
model_provider = "copilot_test"
approval_policy = "never"
sandbox_mode = "read-only"

[features]
multi_agent = true
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


def _root_config_overrides(
    composition: DevComposition, prepared: PreparedDevProfile
) -> dict[str, Any]:
    if composition.root_agent is None:
        return {}
    role_path = prepared.profile_root / "agents" / f"{composition.root_agent}.toml"
    with role_path.open("rb") as handle:
        role = tomllib.load(handle)
    for metadata in ("name", "description", "nickname_candidates"):
        role.pop(metadata, None)
    flattened: dict[str, Any] = {}

    def visit(prefix: str | None, value: Any) -> None:
        if isinstance(value, dict):
            for key, child in value.items():
                visit(key if prefix is None else f"{prefix}.{key}", child)
            return
        if prefix is None:
            raise CopilotTestError(
                "manifest_invalid",
                "root-config",
                "Root Agent config must be a table",
            )
        flattened[prefix] = value

    visit(None, role)
    return flattened


def _wait_for_root_terminal(
    client: AppServerClient,
    case: CopilotTestCase,
    *,
    root_thread_id: str,
    root_turn_id: str,
    deadline: float,
) -> str | None:
    child_thread_id: str | None = None
    observed: list[str] = []
    while time.monotonic() < deadline:
        try:
            notification = client.next_notification(
                timeout_seconds=max(0.1, deadline - time.monotonic())
            )
        except AppServerClientError as error:
            raise CopilotTestError(
                "timed_out" if error.code == "NotificationTimedOut" else "runtime_unavailable",
                "terminal",
                "notification stream ended before the Root terminal event",
                test_id=case.id,
                diagnostics={"notifications": observed[:24]},
            ) from error
        method = notification.get("method")
        params = notification.get("params")
        if not isinstance(params, dict):
            continue
        item = params.get("item")
        item_type = item.get("type") if isinstance(item, dict) else None
        turn = params.get("turn")
        turn_status = turn.get("status") if isinstance(turn, dict) else None
        if len(observed) < 24:
            observed.append(f"{method}:{item_type or '-'}:{turn_status or '-'}")
        if method == "thread/started" and case.target_kind == "agent":
            thread = params.get("thread")
            if (
                isinstance(thread, dict)
                and thread.get("parentThreadId") == root_thread_id
                and thread.get("agentRole") == case.target_agent
            ):
                candidate = thread.get("id")
                if isinstance(candidate, str):
                    child_thread_id = _bind_child_id(child_thread_id, candidate, case.id)
        elif method in ("item/started", "item/completed") and isinstance(item, dict):
            if (
                case.target_kind == "agent"
                and params.get("threadId") == root_thread_id
                and item.get("type") == "subAgentActivity"
                and item.get("kind") == "started"
            ):
                candidate = item.get("agentThreadId")
                if isinstance(candidate, str):
                    child_thread_id = _bind_child_id(child_thread_id, candidate, case.id)
        if method != "turn/completed" or params.get("threadId") != root_thread_id:
            continue
        if not isinstance(turn, dict) or turn.get("id") != root_turn_id:
            continue
        if turn.get("status") != "completed":
            raise CopilotTestError(
                "terminal_missing",
                "root",
                "Root turn reached a non-completed terminal state",
                test_id=case.id,
            )
        return child_thread_id
    raise CopilotTestError(
        "timed_out",
        "terminal",
        "Root turn did not finish before the deadline",
        test_id=case.id,
    )


def _bind_child_id(existing: str | None, candidate: str, test_id: str) -> str:
    if existing is not None and existing != candidate:
        raise CopilotTestError(
            "agent_not_spawned",
            "agent",
            "multiple Agent target identities were observed",
            test_id=test_id,
        )
    return candidate


def _child_id_from_root_history(root_read: dict[str, Any]) -> str | None:
    thread = root_read.get("thread")
    if not isinstance(thread, dict):
        return None
    candidates: set[str] = set()
    for turn in thread.get("turns", []):
        if not isinstance(turn, dict):
            continue
        for item in turn.get("items", []):
            if not isinstance(item, dict):
                continue
            if item.get("type") == "subAgentActivity" and item.get("kind") == "started":
                value = item.get("agentThreadId")
                if isinstance(value, str):
                    candidates.add(value)
            if item.get("type") == "collabAgentToolCall" and item.get("tool") == "spawnAgent":
                values = item.get("receiverThreadIds")
                if isinstance(values, list):
                    candidates.update(value for value in values if isinstance(value, str))
    return next(iter(candidates)) if len(candidates) == 1 else None


def _evidence_from_history(
    case: CopilotTestCase,
    *,
    root_read: dict[str, Any],
    child_read: dict[str, Any] | None,
    root_thread_id: str,
    root_turn_id: str,
    supervisor_skill: str,
) -> AcceptanceEvidence:
    root_thread = _canonical_thread(root_read, test_id=case.id)
    if root_thread.get("id") != root_thread_id:
        raise CopilotTestError(
            "terminal_missing", "history", "Root history identity changed", test_id=case.id
        )
    root_turn = _canonical_turn(root_thread, root_turn_id, test_id=case.id)
    if root_turn.get("status") != "completed":
        raise CopilotTestError(
            "terminal_missing", "root", "Root history is not completed", test_id=case.id
        )
    root_items = _unique_items(root_turn.get("items"), test_id=case.id)
    root_final_index = _last_item_index(root_items, "agentMessage")
    if root_final_index is None:
        raise CopilotTestError(
            "terminal_missing", "root", "Root history omitted the final message", test_id=case.id
        )

    if case.target_kind == "root":
        target_turn = root_turn
        target_items = root_items
        target_anchor = _matching_mcp_index(target_items, case)
    else:
        child_thread = _canonical_thread(child_read, test_id=case.id)
        if (
            child_thread.get("parentThreadId") != root_thread_id
            or child_thread.get("agentRole") != case.target_agent
        ):
            raise CopilotTestError(
                "agent_not_spawned",
                "agent",
                "canonical child history did not use the declared parent and Role",
                test_id=case.id,
            )
        target_turn, target_items = _turn_with_declared_mcp(child_thread, case)
        target_anchor = _matching_mcp_index(target_items, case)
        child_completed_at = target_turn.get("completedAt")
        root_completed_at = root_turn.get("completedAt")
        if (
            isinstance(child_completed_at, int)
            and isinstance(root_completed_at, int)
            and child_completed_at > root_completed_at
        ):
            raise CopilotTestError(
                "terminal_missing",
                "root",
                "Root completed before the Agent target",
                test_id=case.id,
            )
        child_id = child_thread.get("id")
        spawn_index = (
            _root_spawn_index(root_items, child_id)
            if isinstance(child_id, str)
            else None
        )
        if spawn_index is None:
            raise CopilotTestError(
                "agent_not_spawned",
                "agent",
                "canonical Root history omitted the declared Agent spawn",
                test_id=case.id,
            )
        target_anchor = spawn_index

    matching = [
        item
        for item in target_items
        if item.get("type") == "mcpToolCall"
        and item.get("server") == case.server
        and item.get("tool") == case.tool_name
    ]
    if len(matching) != 1 or matching[0].get("status") != "completed":
        raise CopilotTestError(
            "tool_not_called",
            "tool",
            "canonical target history did not contain one completed declared Tool call",
            test_id=case.id,
            diagnostics={"matchingToolCalls": len(matching)},
        )
    tool_item = matching[0]
    if tool_item.get("arguments") != case.arguments:
        raise CopilotTestError(
            "arguments_mismatch",
            "tool",
            "canonical Tool arguments differed from the declaration",
            test_id=case.id,
        )
    result = tool_item.get("result")
    structured = result.get("structuredContent") if isinstance(result, dict) else None
    if structured != case.expected_structured_content:
        raise CopilotTestError(
            "result_mismatch",
            "tool",
            "canonical Tool structured content differed from the expectation",
            test_id=case.id,
        )
    if target_turn.get("status") != "completed":
        raise CopilotTestError(
            "terminal_missing",
            "target",
            "target turn did not complete",
            test_id=case.id,
        )
    if target_anchor is None or root_final_index <= target_anchor:
        raise CopilotTestError(
            "terminal_missing",
            "root",
            "Root final message did not follow the target completion",
            test_id=case.id,
        )
    return AcceptanceEvidence(
        supervisor_skill=supervisor_skill,
        target_kind=case.target_kind,
        target_agent=case.target_agent,
        capability_root=case.tool,
        server=case.server,
        tool_name=case.tool_name,
        mcp_completed=True,
        arguments_matched=True,
        structured_content_matched=True,
        target_turn_completed=True,
        root_final_after_target=True,
        root_turn_completed=True,
    )


def _canonical_thread(value: dict[str, Any] | None, *, test_id: str) -> dict[str, Any]:
    thread = value.get("thread") if isinstance(value, dict) else None
    if not isinstance(thread, dict) or not isinstance(thread.get("turns"), list):
        raise CopilotTestError(
            "terminal_missing",
            "history",
            "thread/read omitted canonical turns",
            test_id=test_id,
        )
    return thread


def _canonical_turn(thread: dict[str, Any], turn_id: str, *, test_id: str) -> dict[str, Any]:
    matches = [
        turn
        for turn in thread.get("turns", [])
        if isinstance(turn, dict) and turn.get("id") == turn_id
    ]
    if len(matches) != 1:
        raise CopilotTestError(
            "terminal_missing",
            "history",
            "thread/read did not return the exact Root turn",
            test_id=test_id,
        )
    return matches[0]


def _turn_with_declared_mcp(
    thread: dict[str, Any], case: CopilotTestCase
) -> tuple[dict[str, Any], list[dict[str, Any]]]:
    matches: list[tuple[dict[str, Any], list[dict[str, Any]]]] = []
    for turn in thread.get("turns", []):
        if not isinstance(turn, dict):
            continue
        items = _unique_items(turn.get("items"), test_id=case.id)
        if any(
            item.get("type") == "mcpToolCall"
            and item.get("server") == case.server
            and item.get("tool") == case.tool_name
            for item in items
        ):
            matches.append((turn, items))
    if len(matches) != 1:
        raise CopilotTestError(
            "tool_not_called",
            "tool",
            "canonical Agent history did not identify one Tool turn",
            test_id=case.id,
            diagnostics={"matchingTurns": len(matches)},
        )
    return matches[0]


def _unique_items(value: Any, *, test_id: str) -> list[dict[str, Any]]:
    if not isinstance(value, list):
        raise CopilotTestError(
            "terminal_missing", "history", "canonical turn omitted items", test_id=test_id
        )
    items: list[dict[str, Any]] = []
    seen: dict[str, dict[str, Any]] = {}
    for item in value:
        if not isinstance(item, dict):
            continue
        item_id = item.get("id")
        if not isinstance(item_id, str) or not item_id:
            raise CopilotTestError(
                "terminal_missing",
                "history",
                "canonical item omitted its identity",
                test_id=test_id,
            )
        previous = seen.get(item_id)
        if previous is not None:
            if previous != item:
                raise CopilotTestError(
                    "terminal_missing",
                    "history",
                    "canonical item identity had conflicting contents",
                    test_id=test_id,
                )
            continue
        seen[item_id] = item
        items.append(item)
    return items


def _matching_mcp_index(items: list[dict[str, Any]], case: CopilotTestCase) -> int | None:
    matches = [
        index
        for index, item in enumerate(items)
        if item.get("type") == "mcpToolCall"
        and item.get("server") == case.server
        and item.get("tool") == case.tool_name
    ]
    return matches[0] if len(matches) == 1 else None


def _last_item_index(items: list[dict[str, Any]], item_type: str) -> int | None:
    matches = [index for index, item in enumerate(items) if item.get("type") == item_type]
    return matches[-1] if matches else None


def _root_spawn_index(items: list[dict[str, Any]], child_id: str) -> int | None:
    for index, item in enumerate(items):
        if (
            item.get("type") == "subAgentActivity"
            and item.get("kind") == "started"
            and item.get("agentThreadId") == child_id
        ):
            return index
        if item.get("type") == "collabAgentToolCall" and item.get("tool") == "spawnAgent":
            values = item.get("receiverThreadIds")
            if isinstance(values, list) and child_id in values:
                return index
    return None


def _require_skill(result: dict[str, Any], name: str, *, test_id: str | None = None) -> None:
    entries = result.get("data")
    if isinstance(entries, list) and any(
        isinstance(entry, dict) and bool(entry.get("errors")) for entry in entries
    ):
        raise CopilotTestError(
            "skill_not_discovered",
            "skills/list",
            "Runtime reported Skill discovery errors",
            test_id=test_id,
        )
    if not isinstance(entries, list) or not any(
        isinstance(entry, dict)
        and isinstance(entry.get("skills"), list)
        and any(isinstance(skill, dict) and skill.get("name") == name for skill in entry["skills"])
        for entry in entries
    ):
        raise CopilotTestError(
            "skill_not_discovered",
            "skills/list",
            "Runtime did not discover the declared Root Skill",
            test_id=test_id,
        )


def _require_tool(
    result: dict[str, Any], server: str, tool: str, *, test_id: str | None = None
) -> None:
    entries = result.get("data")
    if not isinstance(entries, list) or not any(
        isinstance(entry, dict)
        and entry.get("name") == server
        and isinstance(entry.get("tools"), dict)
        and tool in entry["tools"]
        for entry in entries
    ):
        raise CopilotTestError(
            "tool_not_available",
            "mcpServerStatus/list",
            "Runtime did not expose the declared MCP Tool",
            test_id=test_id,
        )


def _thread_id(result: dict[str, Any], *, test_id: str | None = None) -> str:
    thread = result.get("thread")
    thread_id = thread.get("id") if isinstance(thread, dict) else None
    if not isinstance(thread_id, str) or not thread_id:
        raise CopilotTestError(
            "terminal_missing",
            "thread/start",
            "Runtime omitted the Root thread identity",
            test_id=test_id,
        )
    return thread_id
