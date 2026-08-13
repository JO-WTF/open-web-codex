from __future__ import annotations

import json
import io
import sys
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from unittest.mock import patch

from copilot_sdk.cli import main
from copilot_sdk.dev_profile import (
    CopilotDevError,
    OWNER_MARKER,
    load_dev_composition,
    prepare_dev_profile,
    validate_workspace,
)


class CopilotDevProfileTests(unittest.TestCase):
    def make_source(self, parent: Path) -> Path:
        root = parent / "source"
        (root / "skills" / "supervisor" / "references").mkdir(parents=True)
        (root / "agents").mkdir()
        (root / "tools" / "native" / ".codex-plugin").mkdir(parents=True)
        (root / "tools" / "native" / "bin").mkdir()
        (root / "skills" / "supervisor" / "SKILL.md").write_text(
            "---\nname: supervisor\ndescription: Coordinate work.\n---\n",
            encoding="utf-8",
        )
        (root / "skills" / "supervisor" / "references" / "guide.md").write_text(
            "complete tree\n", encoding="utf-8"
        )
        (root / "agents" / "worker.toml").write_text(
            'name = "worker"\n[[skills.config]]\nname = "supervisor"\nenabled = true\n'
            '[plugins.native]\nenabled = true\n'
            '[plugins.native.mcp_servers.native]\n'
            'enabled = true\ndefault_tools_approval_mode = "approve"\n'
            'enabled_tools = ["health"]\n',
            encoding="utf-8",
        )
        (root / "tools" / "native" / ".codex-plugin" / "plugin.json").write_text(
            '{"mcpServers":"./.mcp.json"}', encoding="utf-8"
        )
        (root / "tools" / "native" / ".mcp.json").write_text(
            '{"mcpServers":{"native":{"command":"./bin/native-launcher","cwd":".",'
            '"args":[],"env_vars":["OPEN_WEB_CODEX_DATA_DIR"],'
            '"enabled_tools":["transport-default"]}}}',
            encoding="utf-8",
        )
        launcher = root / "tools" / "native" / "bin/native-launcher"
        launcher.write_text("#!/usr/bin/env bash\nexit 0\n", encoding="utf-8")
        launcher.chmod(0o755)
        setup = root / "tools" / "native" / "bin/setup-env"
        setup.write_text(
            "#!/usr/bin/env bash\n"
            "set -euo pipefail\n"
            "mkdir -p \"$OPEN_WEB_CODEX_DATA_DIR/tool-envs/native\"\n"
            "touch \"$OPEN_WEB_CODEX_DATA_DIR/tool-envs/native/setup-invoked\"\n",
            encoding="utf-8",
        )
        setup.chmod(0o755)
        (root / "copilot.toml").write_text(
            'schema_version = 1\nid = "sample"\ndisplay_name = "Sample"\n'
            '[supervisor]\nskill = "supervisor"\n'
            '[[skills]]\nid = "supervisor"\npath = "skills/supervisor"\n'
            '[[agents]]\nid = "worker"\nrole = "agents/worker.toml"\n'
            '[[tools]]\nid = "native"\nroot = "tools/native"\n',
            encoding="utf-8",
        )
        return root

    def test_materializes_role_transport_projection_without_tools_or_config(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            temporary_root = Path(directory)
            source = self.make_source(temporary_root)
            profile = temporary_root / "profile"
            prepared = prepare_dev_profile(load_dev_composition(source), profile)

            self.assertTrue((profile / OWNER_MARKER).is_file())
            self.assertEqual(
                (profile / "skills/supervisor/references/guide.md").read_text(),
                "complete tree\n",
            )
            with (profile / "agents/worker.toml").open("rb") as handle:
                runtime_role = __import__("tomllib").load(handle)
            runtime_server = runtime_role["mcp_servers"]["native"]
            self.assertEqual(
                runtime_server["command"],
                str((source / "tools/native/bin/native-launcher").resolve()),
            )
            self.assertEqual(runtime_server["cwd"], str((source / "tools/native").resolve()))
            self.assertEqual(runtime_server["enabled_tools"], ["health"])
            self.assertEqual(runtime_server["default_tools_approval_mode"], "approve")
            self.assertEqual(runtime_server["env_vars"], ["OPEN_WEB_CODEX_DATA_DIR"])
            self.assertNotIn("plugins", runtime_role)
            self.assertFalse((profile / "tools").exists())
            self.assertFalse((profile / "config.toml").exists())
            self.assertFalse(prepared.cleanup_on_exit)

    def test_role_projection_rejects_absolute_transport_command(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = self.make_source(root)
            descriptor = source / "tools/native/.mcp.json"
            descriptor.write_text(
                '{"mcpServers":{"native":{"command":"/bin/echo","cwd":"."}}}',
                encoding="utf-8",
            )

            with self.assertRaises(CopilotDevError) as caught:
                prepare_dev_profile(load_dev_composition(source), root / "profile")

            self.assertEqual(caught.exception.code, "UnsafePath")
            self.assertEqual(caught.exception.stage, "role-projection")

    def test_explicit_profile_rejects_foreign_non_empty_directory(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            temporary_root = Path(directory)
            source = self.make_source(temporary_root)
            profile = temporary_root / "profile"
            profile.mkdir()
            (profile / "foreign.txt").write_text("keep", encoding="utf-8")
            with self.assertRaises(CopilotDevError) as caught:
                prepare_dev_profile(load_dev_composition(source), profile)
            self.assertEqual(caught.exception.code, "ProfileConflict")
            self.assertEqual((profile / "foreign.txt").read_text(), "keep")

    def test_explicit_profile_root_symlink_is_rejected_without_touching_target(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = self.make_source(root)
            external = root / "external"
            external.mkdir()
            sentinel = external / "sentinel.txt"
            sentinel.write_text("keep", encoding="utf-8")
            profile = root / "profile"
            profile.symlink_to(external, target_is_directory=True)

            with self.assertRaises(CopilotDevError) as caught:
                prepare_dev_profile(load_dev_composition(source), profile)

            self.assertEqual(caught.exception.code, "ProfileConflict")
            self.assertEqual(sentinel.read_text(encoding="utf-8"), "keep")
            self.assertTrue(profile.is_symlink())

    def test_owner_marker_symlink_is_rejected_without_touching_target(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = self.make_source(root)
            profile = root / "profile"
            profile.mkdir()
            external = root / "external-marker.json"
            external.write_text("external secret", encoding="utf-8")
            (profile / OWNER_MARKER).symlink_to(external)

            with self.assertRaises(CopilotDevError) as caught:
                prepare_dev_profile(load_dev_composition(source), profile)

            self.assertEqual(caught.exception.code, "ProfileConflict")
            self.assertEqual(external.read_text(encoding="utf-8"), "external secret")
            self.assertTrue((profile / OWNER_MARKER).is_symlink())

    def test_process_root_is_cleaned_when_explicit_profile_validation_fails(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = self.make_source(root)
            profile = root / "profile"
            profile.write_text("not a directory", encoding="utf-8")
            process_root = root / "bounded-process"

            def make_process_root(*, prefix: str) -> str:
                self.assertEqual(prefix, "copilot-dev-process-")
                process_root.mkdir()
                return str(process_root)

            with patch(
                "copilot_sdk.dev_profile.tempfile.mkdtemp",
                side_effect=make_process_root,
            ):
                with self.assertRaises(CopilotDevError):
                    prepare_dev_profile(load_dev_composition(source), profile)

            self.assertFalse(process_root.exists())
            self.assertEqual(profile.read_text(encoding="utf-8"), "not a directory")

    def test_temporary_profile_cleanup_is_owned_and_bounded(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            source = self.make_source(Path(directory))
            prepared = prepare_dev_profile(load_dev_composition(source))
            profile = prepared.profile_root
            process_root = prepared.process_root
            self.assertTrue(profile.is_dir())
            self.assertFalse(process_root.is_relative_to(profile))
            prepared.cleanup()
            self.assertFalse(profile.exists())
            self.assertFalse(process_root.exists())

    def test_workspace_must_be_absolute_existing_directory(self) -> None:
        with self.assertRaises(CopilotDevError) as caught:
            validate_workspace(Path("relative"))
        self.assertEqual(caught.exception.code, "WorkspaceInvalid")


class CopilotDevCliTests(unittest.TestCase):
    def make_source(self, parent: Path) -> Path:
        return CopilotDevProfileTests().make_source(parent)

    def make_fake_codex(
        self,
        parent: Path,
        *,
        skill_errors: list[dict[str, str]] | None = None,
        mcp_tools: dict[str, object] | None = None,
    ) -> tuple[Path, Path]:
        executable = parent / "fake-codex"
        transcript = parent / "transcript.json"
        skill_errors = [] if skill_errors is None else skill_errors
        mcp_tools = {"health": {"name": "health"}} if mcp_tools is None else mcp_tools
        executable.write_text(
            f"#!{sys.executable}\n"
            "import json, os, sys\n"
            f"transcript = {str(transcript)!r}\n"
            "messages = []\n"
            "for line in sys.stdin:\n"
            "    message = json.loads(line)\n"
            "    messages.append(message)\n"
            "    method = message.get('method')\n"
            "    if method == 'initialize': result = {'codexHome': os.environ['CODEX_HOME']}\n"
            f"    elif method == 'skills/list': result = {{'data': [{{'cwd': message['params']['cwds'][0], 'skills': [{{'name': 'supervisor'}}], 'errors': {skill_errors!r}}}]}}\n"
            "    elif method == 'thread/start': result = {'thread': {'id': 'thread-dev'}}\n"
            f"    elif method == 'mcpServerStatus/list': result = {{'data': [{{'name': 'native', 'tools': {mcp_tools!r}}}], 'nextCursor': None}}\n"
            "    else:\n"
            "        if 'id' not in message: continue\n"
            "        result = {}\n"
            "    if method == 'mcpServerStatus/list':\n"
            "        with open(transcript, 'w', encoding='utf-8') as handle:\n"
            "            data = os.environ['OPEN_WEB_CODEX_DATA_DIR']\n"
            "            json.dump({'messages': messages, 'mcpTools': result['data'][0]['tools'], 'cwd': os.getcwd(), 'profile': os.environ['CODEX_HOME'], 'home': os.environ['HOME'], 'data': data, 'setup': os.path.isfile(os.path.join(data, 'tool-envs/native/setup-invoked'))}, handle)\n"
            "    print(json.dumps({'id': message['id'], 'result': result}), flush=True)\n",
            encoding="utf-8",
        )
        executable.chmod(0o755)
        return executable, transcript

    def invoke(self, *arguments: str) -> tuple[int, str, str]:
        stdout = io.StringIO()
        stderr = io.StringIO()
        with redirect_stdout(stdout), redirect_stderr(stderr):
            result = main(list(arguments))
        return result, stdout.getvalue(), stderr.getvalue()

    def test_dev_runs_official_discovery_transcript_and_cleans_process_dirs(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = self.make_source(root)
            workspace = root / "workspace"
            workspace.mkdir()
            codex, transcript_path = self.make_fake_codex(root)

            result, stdout, stderr = self.invoke(
                "dev",
                str(source),
                "--workspace",
                str(workspace),
                "--codex-bin",
                str(codex),
                "--json",
            )

            self.assertEqual(result, 0, stderr)
            self.assertEqual(stderr, "")
            payload = json.loads(stdout)
            self.assertEqual(payload["state"], "discovery_ready")
            self.assertEqual(payload["roleSpawn"], "not_run")
            self.assertEqual(payload["modelAcceptance"], "not_run")
            self.assertEqual(payload["discovery"]["skills"]["observed"], ["supervisor"])
            self.assertEqual(payload["discovery"]["mcpServers"]["observed"], ["native"])
            self.assertFalse(Path(payload["profile"]["path"]).exists())

            transcript = json.loads(transcript_path.read_text(encoding="utf-8"))
            messages = transcript["messages"]
            self.assertEqual(
                [message["method"] for message in messages],
                [
                    "initialize",
                    "initialized",
                    "skills/list",
                    "thread/start",
                    "mcpServerStatus/list",
                ],
            )
            self.assertTrue(messages[0]["params"]["capabilities"]["experimentalApi"])
            self.assertEqual(messages[2]["params"]["cwds"], [str(workspace.resolve())])
            self.assertEqual(
                messages[3]["params"]["selectedCapabilityRoots"],
                [
                    {
                        "id": "native",
                        "location": {
                            "type": "environment",
                            "environmentId": "local",
                            "path": str((source / "tools/native").resolve()),
                        },
                    }
                ],
            )
            self.assertTrue(messages[3]["params"]["ephemeral"])
            self.assertEqual(messages[3]["params"]["approvalPolicy"], "never")
            self.assertEqual(messages[3]["params"]["sandbox"], "read-only")
            self.assertEqual(
                messages[3]["params"]["environments"],
                [{
                    "environmentId": "local",
                    "cwd": str(workspace.resolve()),
                    "runtimeWorkspaceRoots": [str(workspace.resolve())],
                }],
            )
            self.assertEqual(messages[4]["params"]["threadId"], "thread-dev")
            self.assertEqual(messages[4]["params"]["limit"], 100)
            self.assertEqual(transcript["mcpTools"], {"health": {"name": "health"}})
            self.assertNotEqual(transcript["cwd"], transcript["profile"])
            self.assertNotEqual(transcript["home"], transcript["profile"])
            self.assertTrue(transcript["setup"])
            self.assertFalse(Path(transcript["cwd"]).exists())
            self.assertFalse(Path(transcript["home"]).exists())
            self.assertFalse(Path(transcript["data"]).exists())

    def test_dev_explicit_profile_is_preserved_without_tool_copy(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = self.make_source(root)
            workspace = root / "workspace"
            workspace.mkdir()
            profile = root / "profile"
            codex, _ = self.make_fake_codex(root)

            result, stdout, stderr = self.invoke(
                "dev", str(source), "--workspace", str(workspace),
                "--profile", str(profile), "--codex-bin", str(codex), "--json"
            )

            self.assertEqual(result, 0, stderr)
            payload = json.loads(stdout)
            self.assertFalse(payload["profile"]["temporary"])
            self.assertTrue(payload["profile"]["preserved"])
            self.assertEqual(
                sorted(path.name for path in profile.iterdir()),
                [OWNER_MARKER, "agents", "skills"],
            )
            self.assertFalse((profile / "tools").exists())

    def test_dev_rejects_missing_tool_setup_before_runtime(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = self.make_source(root)
            (source / "tools/native/bin/setup-env").unlink()
            workspace = root / "workspace"
            workspace.mkdir()
            codex, transcript = self.make_fake_codex(root)

            result, stdout, stderr = self.invoke(
                "dev", str(source), "--workspace", str(workspace),
                "--codex-bin", str(codex), "--json"
            )

            self.assertEqual(result, 2)
            self.assertEqual(stderr, "")
            error = json.loads(stdout)["error"]
            self.assertEqual(error["code"], "EnvironmentUnavailable")
            self.assertEqual(error["stage"], "tool-setup")
            self.assertFalse(transcript.exists())

    def test_dev_reports_tool_setup_failure_without_runtime(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = self.make_source(root)
            setup = source / "tools/native/bin/setup-env"
            secret = "SENTINEL_SETUP_SECRET"
            setup.write_text(
                f"#!/usr/bin/env bash\nprintf '{secret}' >&2\nexit 9\n"
            )
            setup.chmod(0o755)
            workspace = root / "workspace"
            workspace.mkdir()
            codex, transcript = self.make_fake_codex(root)

            result, stdout, stderr = self.invoke(
                "dev", str(source), "--workspace", str(workspace),
                "--codex-bin", str(codex), "--json"
            )

            self.assertEqual(result, 2)
            self.assertEqual(stderr, "")
            error = json.loads(stdout)["error"]
            self.assertEqual(error["code"], "EnvironmentUnavailable")
            self.assertIn("status 9", error["message"])
            self.assertNotIn("cause", error)
            self.assertNotIn(secret, stdout)
            self.assertNotIn(secret, stderr)
            self.assertFalse(transcript.exists())

    def test_dev_rejects_skill_discovery_errors_before_name_matching(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = self.make_source(root)
            workspace = root / "workspace"
            workspace.mkdir()
            codex, _ = self.make_fake_codex(
                root,
                skill_errors=[{"path": "broken", "message": "parse failed"}],
            )

            result, stdout, stderr = self.invoke(
                "dev", str(source), "--workspace", str(workspace),
                "--codex-bin", str(codex), "--json"
            )

            self.assertEqual(result, 2)
            self.assertEqual(stderr, "")
            error = json.loads(stdout)["error"]
            self.assertEqual(error["code"], "DiscoveryIncomplete")
            self.assertIn("Skill discovery errors", error["message"])

    def test_dev_does_not_count_mcp_server_with_empty_tools(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = self.make_source(root)
            workspace = root / "workspace"
            workspace.mkdir()
            codex, _ = self.make_fake_codex(root, mcp_tools={})

            result, stdout, stderr = self.invoke(
                "dev", str(source), "--workspace", str(workspace),
                "--codex-bin", str(codex), "--json"
            )

            self.assertEqual(result, 2)
            self.assertEqual(stderr, "")
            error = json.loads(stdout)["error"]
            self.assertEqual(error["code"], "DiscoveryIncomplete")
            self.assertIn("native", error["message"])

    def test_dev_json_reports_absolute_workspace_failure_without_launch(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = self.make_source(root)
            result, stdout, stderr = self.invoke(
                "dev", str(source), "--workspace", "relative", "--json"
            )
            self.assertEqual(result, 2)
            self.assertEqual(stderr, "")
            error = json.loads(stdout)["error"]
            self.assertEqual(error["code"], "WorkspaceInvalid")
            self.assertEqual(error["stage"], "workspace")


if __name__ == "__main__":
    unittest.main()
