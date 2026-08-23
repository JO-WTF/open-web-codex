from __future__ import annotations

import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import MagicMock, patch

from copilot_sdk.copilot_manifest import CopilotPackageSummary, CopilotTestCase
from copilot_sdk.dev_profile import DevComposition, PreparedDevProfile
from copilot_sdk.test_runner import (
    AcceptanceEvidence,
    CopilotTestError,
    _evidence_from_history,
    _public_evidence,
    _require_skill,
    _run_case,
    _runtime_deadline,
    run_copilot_tests,
)
from copilot_sdk.tool_environment import (
    MaterializedCapabilityRoot,
    MaterializedToolComposition,
)


class CopilotTestRunnerTests(unittest.TestCase):
    def test_empty_status_inventory_fails_before_turn_start(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            profile = root / "profile"
            skill = profile / "skills/supervisor"
            skill.mkdir(parents=True)
            (skill / "SKILL.md").write_text("supervisor body", encoding="utf-8")
            for name in ("home", "cwd", "data"):
                (root / name).mkdir()
            summary = CopilotPackageSummary(
                "demo", "Demo", ("supervisor",), ("worker",), ("routes",),
                ("health",), "a" * 64,
            )
            composition = DevComposition(
                root,
                Path("copilot.toml"),
                summary,
                "supervisor",
                None,
                (),
                (),
                (),
                ("routing",),
            )
            prepared = PreparedDevProfile(
                composition, profile, root, root / "home", root / "cwd", root / "data",
                True, False,
            )
            case = CopilotTestCase(
                "health", "check", "agent", "worker", "routes", "routing", "health", {}, {"ok": True}
            )
            projection = root / "projection"
            projection.mkdir()
            prepared_tools = MaterializedToolComposition(
                (MaterializedCapabilityRoot("routes", projection, ()),)
            )

            class Fixture:
                base_url = "http://127.0.0.1:1/v1"

                def __enter__(self):
                    return self

                def __exit__(self, *args):
                    return False

            class Client:
                def __init__(self):
                    self.methods: list[str] = []
                    self.stderr = ""

                def initialize(self):
                    self.methods.append("initialize")

                def request(self, method, params, **kwargs):
                    self.methods.append(method)
                    if method == "skills/list":
                        return {"data": [{"skills": [{"name": "supervisor"}], "errors": []}]}
                    if method == "thread/start":
                        return {"thread": {"id": "root"}}
                    if method == "mcpServerStatus/list":
                        return {"data": [{"name": "routing", "tools": {}}]}
                    raise AssertionError(f"unexpected request {method}")

                def close(self):
                    pass

            client = Client()
            with (
                patch("copilot_sdk.test_runner.MockResponsesFixture", return_value=Fixture()),
                patch("copilot_sdk.test_runner.AppServerClient.launch", return_value=client),
            ):
                with self.assertRaises(CopilotTestError) as caught:
                    _run_case(
                        composition, case, prepared, root, root / "codex", prepared_tools,
                        timeout_seconds=5,
                    )

            self.assertEqual(caught.exception.stage, "mcpServerStatus/list")
            self.assertEqual(caught.exception.code, "tool_not_available")
            self.assertEqual(
                client.methods,
                ["initialize", "skills/list", "thread/start", "mcpServerStatus/list"],
            )

    def test_runtime_deadline_starts_from_current_runtime_clock(self) -> None:
        with patch("copilot_sdk.test_runner.time.monotonic", return_value=150.0):
            self.assertEqual(_runtime_deadline(90.0), 240.0)

    def test_public_evidence_is_bounded_and_has_no_runtime_ids_or_result_payload(self) -> None:
        evidence = AcceptanceEvidence(
            supervisor_skill="supervisor",
            target_kind="agent",
            target_agent="worker",
            capability_root="route-plugin",
            server="routes",
            tool_name="health",
            mcp_completed=True,
            arguments_matched=True,
            structured_content_matched=True,
            target_turn_completed=True,
            root_final_after_target=True,
            root_turn_completed=True,
        )

        public = _public_evidence(evidence)

        self.assertEqual(
            {key: public["mcp"][key] for key in ("capabilityRoot", "server", "toolName")},
            {
                "capabilityRoot": "route-plugin",
                "server": "routes",
                "toolName": "health",
            },
        )
        self.assertNotIn("ThreadId", repr(public))
        self.assertNotIn("arguments", public["mcp"])
        self.assertNotIn("structuredContent", public["mcp"])
        self.assertEqual(public["target"], {"kind": "agent", "agent": "worker"})

    def test_manifest_without_tests_is_a_typed_failure_before_profile_setup(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with (
                patch("copilot_sdk.test_runner.load_dev_composition", return_value=object()),
                patch("copilot_sdk.test_runner.load_copilot_test_cases", return_value=[]),
            ):
                with self.assertRaises(CopilotTestError) as caught:
                    run_copilot_tests(root, Path("copilot.toml"), root, root / "codex")

        self.assertEqual(caught.exception.code, "manifest_invalid")
        self.assertEqual(caught.exception.stage, "manifest")

    def test_skill_discovery_errors_fail_even_when_name_is_present(self) -> None:
        with self.assertRaises(CopilotTestError) as caught:
            _require_skill(
                {
                    "data": [
                        {
                            "skills": [{"name": "supervisor"}],
                            "errors": [{"message": "could not read another skill"}],
                        }
                    ]
                },
                "supervisor",
            )

        self.assertEqual(caught.exception.stage, "skills/list")
        self.assertEqual(caught.exception.code, "skill_not_discovered")

    def test_multiple_cases_use_independent_profiles_and_shared_suite_result(self) -> None:
        cases = (
            CopilotTestCase(
                "one", "first", "root", None, "routes", "routing", "health", {}, {"ok": True}
            ),
            CopilotTestCase(
                "two", "second", "root", None, "routes", "routing", "health", {}, {"ok": True}
            ),
        )
        composition = SimpleNamespace(
            summary=SimpleNamespace(id="demo", composition_descriptor_sha256="a" * 64)
        )
        profiles = (MagicMock(), MagicMock())
        evidence = AcceptanceEvidence(
            supervisor_skill="supervisor",
            target_kind="root",
            target_agent=None,
            capability_root="routes",
            server="routing",
            tool_name="health",
            mcp_completed=True,
            arguments_matched=True,
            structured_content_matched=True,
            target_turn_completed=True,
            root_final_after_target=True,
            root_turn_completed=True,
        )
        with (
            patch("copilot_sdk.test_runner.load_dev_composition", return_value=composition),
            patch("copilot_sdk.test_runner.load_copilot_test_cases", return_value=cases),
            patch("copilot_sdk.test_runner.validate_workspace", return_value=Path("/workspace")),
            patch("copilot_sdk.test_runner.prepare_dev_profile", side_effect=profiles),
            patch("copilot_sdk.test_runner.prepare_dev_tool_composition", return_value=MagicMock()),
            patch("copilot_sdk.test_runner._run_case", return_value=evidence) as run_case,
        ):
            result = run_copilot_tests(
                Path("source"), Path("copilot.toml"), Path("/workspace"), Path("codex")
            )

        self.assertEqual([case["id"] for case in result["tests"]], ["one", "two"])
        self.assertEqual(run_case.call_count, 2)
        profiles[0].cleanup.assert_called_once_with()
        profiles[1].cleanup.assert_called_once_with()

    def test_child_mcp_is_proved_from_canonical_history_without_live_mcp_event(self) -> None:
        case = CopilotTestCase(
            "health",
            "check",
            "agent",
            "worker",
            "routes",
            "routing",
            "health",
            {"region": "east"},
            {"ok": True},
        )
        root_read = {
            "thread": {
                "id": "root",
                "turns": [
                    {
                        "id": "root-turn",
                        "status": "completed",
                        "completedAt": 20,
                        "items": [
                            {
                                "id": "spawn",
                                "type": "collabAgentToolCall",
                                "tool": "spawnAgent",
                                "status": "completed",
                                "receiverThreadIds": ["child"],
                            },
                            {
                                "id": "wait",
                                "type": "collabAgentToolCall",
                                "tool": "wait",
                                "status": "completed",
                                "receiverThreadIds": ["child"],
                            },
                            {"id": "root-final", "type": "agentMessage", "text": "done"},
                        ],
                    }
                ],
            }
        }
        child_read = {
            "thread": {
                "id": "child",
                "parentThreadId": "root",
                "agentRole": "worker",
                "turns": [
                    {
                        "id": "child-turn",
                        "status": "completed",
                        "completedAt": 10,
                        "items": [
                            {
                                "id": "mcp",
                                "type": "mcpToolCall",
                                "server": "routing",
                                "tool": "health",
                                "status": "completed",
                                "arguments": {"region": "east"},
                                "result": {"structuredContent": {"ok": True}},
                            },
                            {"id": "child-final", "type": "agentMessage", "text": "done"},
                        ],
                    }
                ],
            }
        }

        evidence = _evidence_from_history(
            case,
            root_read=root_read,
            child_read=child_read,
            root_thread_id="root",
            root_turn_id="root-turn",
            supervisor_skill="supervisor",
        )

        self.assertTrue(evidence.mcp_completed)
        self.assertTrue(evidence.root_final_after_target)

    def test_root_target_uses_root_history_without_spawning_child(self) -> None:
        case = CopilotTestCase(
            "health", "check", "root", None, "routes", "routing", "health", {}, {"ok": True}
        )
        root_read = {
            "thread": {
                "id": "root",
                "turns": [
                    {
                        "id": "root-turn",
                        "status": "completed",
                        "items": [
                            {
                                "id": "mcp",
                                "type": "mcpToolCall",
                                "server": "routing",
                                "tool": "health",
                                "status": "completed",
                                "arguments": {},
                                "result": {"structuredContent": {"ok": True}},
                            },
                            {"id": "final", "type": "agentMessage", "text": "done"},
                        ],
                    }
                ],
            }
        }

        evidence = _evidence_from_history(
            case,
            root_read=root_read,
            child_read=None,
            root_thread_id="root",
            root_turn_id="root-turn",
            supervisor_skill="supervisor",
        )

        self.assertEqual(evidence.target_kind, "root")
        self.assertIsNone(evidence.target_agent)


if __name__ == "__main__":
    unittest.main()
