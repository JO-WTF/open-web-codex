import io
import json
import os
import subprocess
import sys
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

from copilot_sdk.cli import main
from copilot_sdk.dev_profile import CopilotDevError
from copilot_sdk.tool_environment import ToolEnvironmentError
from copilot_sdk.tool_runtime_manifest import load_tool_runtime_manifest


class CopilotCliTests(unittest.TestCase):
    def invoke(self, *arguments: str) -> tuple[int, str, str]:
        stdout = io.StringIO()
        stderr = io.StringIO()
        with redirect_stdout(stdout), redirect_stderr(stderr):
            result = main(list(arguments))
        return result, stdout.getvalue(), stderr.getvalue()

    def test_init_defaults_to_single_agent_root_with_direct_tool(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "order-review"
            result, stdout, stderr = self.invoke(
                "init", str(root), "--name", "order-review"
            )

            self.assertEqual(result, 0)
            self.assertEqual(stderr, "")
            self.assertIn("Copilot package 'order-review' is valid.", stdout)
            self.assertTrue((root / "copilot.toml").is_file())
            self.assertTrue((root / "skills/order-review-root/SKILL.md").is_file())
            root_skill = (root / "skills/order-review-root/SKILL.md").read_text(
                encoding="utf-8"
            )
            self.assertIn("native `tool_search`", root_skill)
            self.assertIn(
                "completed client ToolSearchOutput preserved in canonical Thread history",
                root_skill,
            )
            self.assertIn("Do not create or contact child Agents", root_skill)
            self.assertTrue((root / "agents/order-review-root.toml").is_file())
            manifest = (root / "copilot.toml").read_text(encoding="utf-8")
            self.assertIn("[[tests]]", manifest)
            self.assertIn('server = "order_review_tools"', manifest)
            self.assertIn('target = { kind = "root" }', manifest)
            role = (root / "agents/order-review-root.toml").read_text(encoding="utf-8")
            self.assertIn("[plugins.order_review_tools]", role)
            self.assertIn("multi_agent = false", role)
            self.assertNotIn("[mcp_servers", role)
            tool_root = root / "tools/order-review-tools"
            self.assertTrue((tool_root / "pyproject.toml").is_file())
            self.assertTrue((tool_root / "requirements.lock").is_file())
            self.assertTrue((tool_root / "runtime.toml").is_file())
            self.assertFalse((tool_root / ".mcp.json").exists())
            self.assertFalse((tool_root / ".codex-plugin").exists())
            self.assertFalse((tool_root / "bin").exists())
            self.assertIn("--hash=sha256:", (tool_root / "requirements.lock").read_text())
            self.assertTrue((tool_root / "src/order_review_tools/server.py").is_file())

    def test_init_multi_agent_generates_root_to_worker_topology(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "order-review"

            result, stdout, stderr = self.invoke(
                "init",
                str(root),
                "--name",
                "order-review",
                "--template",
                "multi-agent",
                "--json",
            )

            self.assertEqual(result, 0, stderr)
            payload = json.loads(stdout)
            self.assertEqual(payload["copilot"]["agent_ids"], ["order-review-worker"])
            self.assertTrue((root / "skills/order-review-root/SKILL.md").is_file())
            worker_skill = root / "skills/order-review-worker/SKILL.md"
            self.assertTrue(worker_skill.is_file())
            self.assertIn("native `tool_search`", worker_skill.read_text(encoding="utf-8"))
            manifest = (root / "copilot.toml").read_text(encoding="utf-8")
            self.assertIn(
                'target = { kind = "agent", agent = "order-review-worker" }',
                manifest,
            )
            self.assertNotIn("agent = \"order-review-root\"", manifest)

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

    def test_init_rejects_symlink_destination_without_touching_target(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "target"
            target.mkdir()
            destination = Path(directory) / "source"
            destination.symlink_to(target, target_is_directory=True)

            result, stdout, stderr = self.invoke(
                "init", str(destination), "--name", "order-review"
            )

            self.assertEqual(result, 2)
            self.assertEqual(stdout, "")
            self.assertIn("unsafe_symlink", stderr)
            self.assertEqual(list(target.iterdir()), [])

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
            self.assertEqual(payload["copilot"]["agent_ids"], ["order-review-root"])

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
            self.assertEqual(payload["error"]["code"], "runtime_unavailable")
            self.assertEqual(payload["error"]["stage"], "workspace")
            self.assertIn("nextAction", payload["error"])
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

    def test_test_case_selects_one_declared_case(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "source"
            result, _, _ = self.invoke("init", str(root), "--name", "order-review")
            self.assertEqual(result, 0)
            payload = {
                "ok": True,
                "state": "test_passed",
                "copilot": {"id": "order-review"},
                "durationMs": 1,
                "tests": [{"id": "native-root-analysis", "state": "passed"}],
            }
            with patch("copilot_sdk.cli.run_copilot_tests", return_value=payload) as run:
                result, stdout, stderr = self.invoke(
                    "test",
                    str(root),
                    "--workspace",
                    str(Path(directory).resolve()),
                    "--codex-bin",
                    sys.executable,
                    "--case",
                    "native-root-analysis",
                    "--json",
                )

            self.assertEqual(result, 0, stderr)
            self.assertEqual(json.loads(stdout)["tests"][0]["id"], "native-root-analysis")
            self.assertEqual(run.call_args.kwargs["case_id"], "native-root-analysis")

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

    def test_tool_init_generates_shared_runtime_and_runnable_core_tests(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "shared-analysis"

            result, stdout, stderr = self.invoke(
                "tool", "init", str(root), "--name", "shared-analysis", "--json"
            )

            self.assertEqual(result, 0, stderr)
            self.assertEqual(json.loads(stdout)["tool"]["serverIds"], ["shared_analysis"])
            self.assertEqual(
                (root / "tool.toml").read_text(encoding="utf-8"),
                'schema_version = 1\nid = "shared-analysis"\nruntime = "runtime.toml"\n',
            )
            runtime = load_tool_runtime_manifest(root, root, Path("runtime.toml"))
            self.assertEqual(runtime.servers[0].entry.module, "shared_analysis.server")
            server = (root / "src/shared_analysis/server.py").read_text(encoding="utf-8")
            self.assertIn("structured_output=True", server)
            self.assertIn("readOnlyHint=True", server)
            self.assertIn("idempotentHint=True", server)
            self.assertIn("openWorldHint=False", server)
            self.assertNotIn(
                "open-web-codex-provider-sdk",
                (root / "pyproject.toml").read_text(encoding="utf-8"),
            )
            environment = {**os.environ, "PYTHONPATH": str(root / "src")}
            completed = subprocess.run(
                [sys.executable, "-m", "unittest", "discover", "-s", str(root / "tests")],
                cwd=root,
                env=environment,
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(completed.returncode, 0, completed.stderr)

            consumer = Path(directory) / "consumer"
            result, _, stderr = self.invoke(
                "init", str(consumer), "--name", "consumer"
            )
            self.assertEqual(result, 0, stderr)
            manifest = consumer / "copilot.toml"
            manifest.write_text(
                manifest.read_text(encoding="utf-8")
                .replace(
                    'id = "consumer_tools"\nroot = "tools/consumer-tools"\n'
                    'runtime = "tools/consumer-tools/runtime.toml"',
                    'id = "consumer_tools"\npackage = "shared-analysis"',
                )
                .replace('server = "consumer_tools"', 'server = "shared_analysis"'),
                encoding="utf-8",
            )
            role = consumer / "agents/consumer-root.toml"
            role.write_text(
                role.read_text(encoding="utf-8").replace(
                    "mcp_servers.consumer_tools", "mcp_servers.shared_analysis"
                ),
                encoding="utf-8",
            )
            result, stdout, stderr = self.invoke(
                "validate",
                str(consumer),
                "--tool-registry-root",
                str(Path(directory)),
                "--json",
            )
            self.assertEqual(result, 0, stderr)
            self.assertEqual(json.loads(stdout)["copilot"]["tool_ids"], ["consumer_tools"])

    def test_tool_init_refuses_non_empty_destination(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "keep.txt").write_text("keep", encoding="utf-8")

            result, stdout, stderr = self.invoke(
                "tool", "init", str(root), "--name", "shared-analysis"
            )

            self.assertEqual(result, 2)
            self.assertEqual(stdout, "")
            self.assertIn("destination_not_empty", stderr)
            self.assertEqual((root / "keep.txt").read_text(encoding="utf-8"), "keep")

    def test_check_runs_bounded_phases_and_cleans_temporary_roots(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "source"
            result, _, _ = self.invoke("init", str(source), "--name", "order-review")
            self.assertEqual(result, 0)
            observed: dict[str, Path] = {}

            def prepare(args):
                observed["tool_env"] = args.output_root
                observed["build_store"] = args.build_store_root
                return {"ok": True, "state": "environment_prepared"}

            def dev(args):
                observed["workspace"] = args.workspace
                self.assertEqual(args.tool_environment_root, observed["tool_env"])
                return {"ok": True, "state": "discovery_ready"}

            with (
                patch("copilot_sdk.cli._run_prepare", side_effect=prepare),
                patch("copilot_sdk.cli._run_dev_probe", side_effect=dev),
                patch(
                    "copilot_sdk.cli.run_copilot_tests",
                    return_value={"ok": True, "state": "test_passed", "tests": [{}]},
                ),
                patch("copilot_sdk.cli._resolve_codex_bin", return_value=Path(sys.executable)),
            ):
                result, stdout, stderr = self.invoke(
                    "check", str(source), "--codex-bin", sys.executable, "--json"
                )

            self.assertEqual(result, 0, stderr)
            payload = json.loads(stdout)
            self.assertEqual(payload["state"], "check_passed")
            self.assertEqual(
                [phase["name"] for phase in payload["phases"]],
                ["validate", "prepare", "dev", "test"],
            )
            self.assertEqual(payload["tests"], {"passed": 1})
            self.assertFalse(observed["workspace"].exists())
            self.assertFalse(observed["tool_env"].exists())
            self.assertTrue(observed["build_store"].is_absolute())
            self.assertNotIn(str(source), stdout)

    def test_check_stops_at_first_failure(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "source"
            result, _, _ = self.invoke("init", str(source), "--name", "order-review")
            self.assertEqual(result, 0)

            with (
                patch(
                    "copilot_sdk.cli._run_prepare",
                    side_effect=CopilotDevError(
                        "EnvironmentUnavailable", "tool-environment", "private", "failed"
                    ),
                ),
                patch("copilot_sdk.cli._run_dev_probe") as dev,
                patch("copilot_sdk.cli.run_copilot_tests") as test,
            ):
                result, stdout, stderr = self.invoke(
                    "check", str(source), "--codex-bin", sys.executable, "--json"
                )

            self.assertEqual(result, 2)
            self.assertEqual(stderr, "")
            payload = json.loads(stdout)
            self.assertEqual(payload["failedPhase"], "prepare")
            self.assertEqual(
                [(phase["name"], phase["state"]) for phase in payload["phases"]],
                [("validate", "passed"), ("prepare", "failed")],
            )
            dev.assert_not_called()
            test.assert_not_called()


if __name__ == "__main__":
    unittest.main()
