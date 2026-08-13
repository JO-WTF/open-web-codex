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
    run_copilot_tests,
)


class CopilotTestRunnerTests(unittest.TestCase):
    def test_public_evidence_is_bounded_and_has_no_runtime_ids_or_result_payload(self) -> None:
        evidence = TestEvidence(
            supervisor_skill="supervisor",
            agent="worker",
            tool="routes",
            tool_name="health",
            mcp_completed=True,
            arguments_matched=True,
            structured_content_matched=True,
            child_turn_completed=True,
            root_final_after_child=True,
            root_turn_completed=True,
        )

        public = _public_evidence(evidence)

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
