from __future__ import annotations

import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from copilot_sdk.test_runner import (
    CopilotTestError,
    TestEvidence,
    _public_evidence,
    _require_skill,
    _runtime_deadline,
    _run_case,
    run_copilot_tests,
)
from copilot_sdk.copilot_manifest import CopilotPackageSummary, CopilotTestCase
from copilot_sdk.dev_profile import DevComposition, PreparedDevProfile
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
                "health", "check", "worker", "routes", "routing", "health", {}, {"ok": True}
            )
            projection = root / "projection"
            projection.mkdir()
            prepared_tools = MaterializedToolComposition(
                (MaterializedCapabilityRoot("routes", projection, ()),)
            )

            class Fixture:
                base_url = "http://127.0.0.1:1/v1"
                first_root_request_has_supervisor_marker = True
                request_classifications: list[dict[str, object]] = []

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
                        timeout_seconds=5, started=0,
                    )

            self.assertEqual(caught.exception.stage, "mcpServerStatus/list")
            self.assertEqual(
                client.methods,
                ["initialize", "skills/list", "thread/start", "mcpServerStatus/list"],
            )

    def test_runtime_deadline_starts_from_current_runtime_clock(self) -> None:
        with patch("copilot_sdk.test_runner.time.monotonic", return_value=150.0):
            self.assertEqual(_runtime_deadline(90.0), 240.0)

    def test_public_evidence_is_bounded_and_has_no_runtime_ids_or_result_payload(self) -> None:
        evidence = TestEvidence(
            supervisor_skill="supervisor",
            agent="worker",
            capability_root="route-plugin",
            server="routes",
            tool_name="health",
            mcp_completed=True,
            arguments_matched=True,
            structured_content_matched=True,
            child_turn_completed=True,
            root_final_after_child=True,
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

    def test_manifest_without_tests_is_a_typed_failure_before_profile_setup(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with (
                patch("copilot_sdk.test_runner.load_dev_composition", return_value=object()),
                patch("copilot_sdk.test_runner.load_copilot_test_cases", return_value=[]),
            ):
                with self.assertRaises(CopilotTestError) as caught:
                    run_copilot_tests(root, Path("copilot.toml"), root, root / "codex")

        self.assertEqual(caught.exception.code, "TestDefinitionInvalid")
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


if __name__ == "__main__":
    unittest.main()
