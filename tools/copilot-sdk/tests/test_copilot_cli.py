import io
import json
import sys
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

from copilot_sdk.cli import main
from copilot_sdk.tool_environment import ToolEnvironmentError


class CopilotCliTests(unittest.TestCase):
    def invoke(self, *arguments: str) -> tuple[int, str, str]:
        stdout = io.StringIO()
        stderr = io.StringIO()
        with redirect_stdout(stdout), redirect_stderr(stderr):
            result = main(list(arguments))
        return result, stdout.getvalue(), stderr.getvalue()

    def test_init_generates_valid_native_author_sources(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "order-review"
            result, stdout, stderr = self.invoke(
                "init", str(root), "--name", "order-review"
            )

            self.assertEqual(result, 0)
            self.assertEqual(stderr, "")
            self.assertIn("Copilot package 'order-review' is valid.", stdout)
            self.assertTrue((root / "copilot.toml").is_file())
            self.assertTrue(
                (root / "skills/order-review-supervisor/SKILL.md").is_file()
            )
            root_skill = (root / "skills/order-review-supervisor/SKILL.md").read_text(
                encoding="utf-8"
            )
            child_skill = (root / "skills/order-review-worker/SKILL.md").read_text(
                encoding="utf-8"
            )
            self.assertIn("native `tool_search`", root_skill)
            self.assertIn("native `tool_search`", child_skill)
            self.assertTrue((root / "agents/order-review-worker.toml").is_file())
            manifest = (root / "copilot.toml").read_text(encoding="utf-8")
            self.assertIn("[[tests]]", manifest)
            self.assertIn('server = "order_review_tools"', manifest)
            role = (root / "agents/order-review-worker.toml").read_text(encoding="utf-8")
            self.assertIn("[plugins.order_review_tools]", role)
            self.assertNotIn("[mcp_servers", role)
            tool_root = root / "tools/order-review-tools"
            self.assertTrue((tool_root / "pyproject.toml").is_file())
            self.assertTrue((tool_root / "requirements.lock").is_file())
            self.assertTrue((tool_root / "runtime.toml").is_file())
            self.assertFalse((tool_root / ".mcp.json").exists())
            self.assertFalse((tool_root / ".codex-plugin").exists())
            self.assertFalse((tool_root / "bin").exists())
            self.assertIn("--hash=sha256:", (tool_root / "requirements.lock").read_text())

    def test_init_refuses_to_overwrite_non_empty_destination(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "keep.txt").write_text("user owned", encoding="utf-8")

            result, stdout, stderr = self.invoke(
                "init", str(root), "--name", "order-review"
            )

            self.assertEqual(result, 2)
            self.assertEqual(stdout, "")
            self.assertIn("destination_not_empty: .:", stderr)
            self.assertEqual((root / "keep.txt").read_text(encoding="utf-8"), "user owned")

    def test_validate_defaults_to_human_output_and_accepts_relative_manifest(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "source"
            result, _, _ = self.invoke("init", str(root), "--name", "order-review")
            self.assertEqual(result, 0)
            (root / "manifests").mkdir()
            (root / "copilot.toml").rename(root / "manifests/development.toml")

            result, stdout, stderr = self.invoke(
                "validate",
                str(root),
                "--manifest",
                "manifests/development.toml",
            )

            self.assertEqual(result, 0)
            self.assertEqual(stderr, "")
            self.assertIn("manifest: manifests/development.toml", stdout)
            self.assertNotIn('"ok"', stdout)

    def test_validate_json_has_stable_success_shape(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "source"
            result, _, _ = self.invoke("init", str(root), "--name", "order-review")
            self.assertEqual(result, 0)

            result, stdout, stderr = self.invoke("validate", str(root), "--json")

            self.assertEqual(result, 0)
            self.assertEqual(stderr, "")
            payload = json.loads(stdout)
            self.assertTrue(payload["ok"])
            self.assertEqual(payload["copilot"]["id"], "order-review")
            self.assertEqual(payload["copilot"]["agent_ids"], ["order-review-worker"])

    def test_validate_json_reports_stable_error_fields(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)

            result, stdout, stderr = self.invoke("validate", str(root), "--json")

            self.assertEqual(result, 2)
            self.assertEqual(stderr, "")
            self.assertEqual(
                json.loads(stdout),
                {
                    "ok": False,
                    "error": {
                        "code": "missing_file",
                        "path": "copilot.toml",
                        "message": "file does not exist",
                    },
                },
            )

    def test_prepare_json_is_bounded_and_passes_explicit_output_root(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "source"
            output = Path(directory) / "prepared"
            result, _, _ = self.invoke("init", str(root), "--name", "order-review")
            self.assertEqual(result, 0)
            prepared = SimpleNamespace(
                state="built",
                capability_roots=(
                    SimpleNamespace(
                        id="order_review_tools",
                        servers=(SimpleNamespace(id="order_review_tools"),),
                    ),
                )
            )
            with patch(
                "copilot_sdk.cli.prepare_tool_composition", return_value=prepared
            ) as prepare:
                result, stdout, stderr = self.invoke(
                    "prepare",
                    str(root),
                    "--output-root",
                    str(output),
                    "--build-store-root",
                    str(Path(directory) / "build-store"),
                    "--json",
                )

            self.assertEqual(result, 0, stderr)
            payload = json.loads(stdout)
            self.assertEqual(payload["state"], "environment_prepared")
            self.assertEqual(payload["capabilityRoots"][0]["servers"], ["order_review_tools"])
            self.assertNotIn(str(output), stdout)
            self.assertNotIn(str(root), stdout)
            self.assertEqual(prepare.call_args.kwargs["output_root"], output)
            self.assertEqual(
                prepare.call_args.kwargs["build_store_root"],
                Path(directory) / "build-store",
            )

    def test_prepare_error_does_not_expose_internal_path_or_cause(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "source"
            output = Path(directory) / "private-output"
            result, _, _ = self.invoke("init", str(root), "--name", "order-review")
            self.assertEqual(result, 0)
            with patch(
                "copilot_sdk.cli.prepare_tool_composition",
                side_effect=ToolEnvironmentError(
                    "OutputConflict", str(output), "output root is not owned", "secret"
                ),
            ):
                result, stdout, stderr = self.invoke(
                    "prepare",
                    str(root),
                    "--output-root",
                    str(output),
                    "--build-store-root",
                    str(Path(directory) / "build-store"),
                    "--json",
                )

            self.assertEqual(result, 2)
            self.assertEqual(stderr, "")
            payload = json.loads(stdout)
            self.assertEqual(payload["error"]["code"], "OutputConflict")
            self.assertNotIn("path", payload["error"])
            self.assertNotIn(str(output), stdout)
            self.assertNotIn("secret", stdout)

    def test_test_json_does_not_expose_workspace_path_from_dev_error(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "source"
            result, _, _ = self.invoke("init", str(root), "--name", "order-review")
            self.assertEqual(result, 0)

            result, stdout, stderr = self.invoke(
                "test",
                str(root),
                "--workspace",
                "relative-private-workspace",
                "--codex-bin",
                sys.executable,
                "--json",
            )

            self.assertEqual(result, 2)
            self.assertEqual(stderr, "")
            payload = json.loads(stdout)
            self.assertEqual(payload["error"]["code"], "WorkspaceInvalid")
            self.assertEqual(payload["error"]["stage"], "workspace")
            self.assertNotIn("relative-private-workspace", stdout)
            self.assertNotIn("path", payload["error"])

    def test_test_json_does_not_expose_missing_codex_path(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "source"
            result, _, _ = self.invoke("init", str(root), "--name", "order-review")
            self.assertEqual(result, 0)
            missing = Path(directory) / "private-runtime" / "codex"

            result, stdout, stderr = self.invoke(
                "test",
                str(root),
                "--workspace",
                str(Path(directory).resolve()),
                "--codex-bin",
                str(missing),
                "--json",
            )

            self.assertEqual(result, 2)
            self.assertEqual(stderr, "")
            self.assertNotIn(str(missing), stdout)
            self.assertNotIn("path", json.loads(stdout)["error"])

    def test_init_rejects_id_that_could_escape_destination(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "source"
            result, stdout, stderr = self.invoke(
                "init", str(root), "--name", "../outside"
            )

            self.assertEqual(result, 2)
            self.assertEqual(stdout, "")
            self.assertIn("invalid_id: id:", stderr)
            self.assertFalse(root.exists())

    def test_legacy_tool_command_is_not_a_cli_surface(self):
        stderr = io.StringIO()
        with redirect_stderr(stderr), self.assertRaises(SystemExit) as caught:
            main(["tool", "validate", "."])

        self.assertEqual(caught.exception.code, 2)
        self.assertIn("invalid choice: 'tool'", stderr.getvalue())


if __name__ == "__main__":
    unittest.main()
