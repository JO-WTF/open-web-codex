import io
import json
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path

from copilot_sdk.cli import main


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
            self.assertTrue((root / "agents/order-review-worker.toml").is_file())
            self.assertTrue(
                (root / "tools/order-review-tools/.codex-plugin/plugin.json").is_file()
            )
            launcher = root / "tools/order-review-tools/bin/order_review_tools-launcher"
            self.assertTrue(launcher.stat().st_mode & 0o111)

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

    def test_legacy_tool_commands_complete_the_authoring_flow(self):
        with tempfile.TemporaryDirectory() as directory:
            temporary_root = Path(directory)
            package = temporary_root / "route-audit"
            archive = temporary_root / "route-audit.zip"

            result, stdout, stderr = self.invoke(
                "tool",
                "init",
                str(package),
                "--name",
                "route-audit",
                "--description",
                "Audit route inputs",
            )
            self.assertEqual(result, 0)
            self.assertEqual(stderr, "")
            initialized = json.loads(stdout)
            self.assertEqual(initialized["name"], "route-audit")

            result, stdout, stderr = self.invoke("tool", "validate", str(package))
            self.assertEqual(result, 0)
            self.assertEqual(stderr, "")
            validated = json.loads(stdout)
            self.assertEqual(validated["contentSha256"], initialized["contentSha256"])

            result, stdout, stderr = self.invoke("tool", "test", str(package))
            self.assertEqual(result, 0)
            self.assertEqual(stderr, "")
            self.assertEqual(
                stdout,
                "No tests directory; package validation still passed.\n",
            )

            result, stdout, stderr = self.invoke(
                "tool", "pack", str(package), "--output", str(archive)
            )
            self.assertEqual(result, 0)
            self.assertEqual(stderr, "")
            packed = json.loads(stdout)
            self.assertEqual(packed["contentSha256"], validated["contentSha256"])
            self.assertEqual(Path(packed["archive"]), archive.resolve())
            self.assertTrue(archive.is_file())
            self.assertRegex(packed["archiveSha256"], r"^[0-9a-f]{64}$")


if __name__ == "__main__":
    unittest.main()
