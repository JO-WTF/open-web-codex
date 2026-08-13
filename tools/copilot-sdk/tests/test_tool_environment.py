from __future__ import annotations

import json
import os
import tempfile
import unittest
from pathlib import Path

from copilot_sdk.tool_environment import (
    ToolEnvironmentError,
    ToolRuntimeSource,
    materialize_capability_roots,
    prepare_tool_composition,
)


class ToolEnvironmentTests(unittest.TestCase):
    def _write_python_tool(self, root: Path) -> Path:
        tool = root / "tools/demo"
        tool.mkdir(parents=True)
        (tool / "pyproject.toml").write_text(
            "[build-system]\nrequires=['setuptools>=77']\n"
            "build-backend='setuptools.build_meta'\n"
            "[project]\nname='demo'\nversion='0.1.0'\n",
            encoding="utf-8",
        )
        (tool / "requirements.lock").write_text(
            "mcp==1.0.0 \\\n    --hash=sha256:" + "a" * 64 + "\n"
            "setuptools==80.9.0 \\\n    --hash=sha256:" + "b" * 64 + "\n",
            encoding="utf-8",
        )
        (tool / "runtime.toml").write_text(
            "schema_version=1\n"
            "[[dependencies]]\nid='python'\nkind='python-project'\n"
            "manifest='pyproject.toml'\nlock='requirements.lock'\n"
            "[[servers]]\nid='demo'\n"
            "entry={kind='python-module',dependency='python',module='demo.server'}\n",
            encoding="utf-8",
        )
        return tool

    def test_compiles_generic_projection_and_bounded_descriptor(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            tool = root / "tools/demo"
            tool.mkdir(parents=True)
            (tool / "pyproject.toml").write_text(
                "[build-system]\nrequires=['setuptools>=77']\n"
                "build-backend='setuptools.build_meta'\n"
                "[project]\nname='demo'\nversion='0.1.0'\n"
            )
            (tool / "requirements.lock").write_text(
                "mcp==1.0.0 \\\n    --hash=sha256:" + "a" * 64 + "\n"
                "setuptools==80.9.0 \\\n    --hash=sha256:" + "b" * 64 + "\n"
            )
            (tool / "package.json").write_text(
                json.dumps({"name": "demo-assets", "version": "1.0.0"}) + "\n"
            )
            (tool / "package-lock.json").write_text(
                json.dumps(
                    {
                        "name": "demo-assets",
                        "version": "1.0.0",
                        "lockfileVersion": 3,
                        "packages": {"": {"name": "demo-assets", "version": "1.0.0"}},
                    }
                )
                + "\n"
            )
            (tool / "runtime.toml").write_text(
                "schema_version=1\n"
                "[[dependencies]]\nid='python'\nkind='python-project'\n"
                "manifest='pyproject.toml'\nlock='requirements.lock'\n"
                "[[dependencies]]\nid='assets'\nkind='node-project'\n"
                "manifest='package.json'\nlock='package-lock.json'\n"
                "[[servers]]\nid='demo'\n"
                "entry={kind='python-module',dependency='python',module='demo.server'}\n"
                "args=['--flag']\n"
                "startup_timeout_sec=60\n"
                "tool_timeout_sec=90\n"
                "[[servers.env]]\nname='CODEX_HOME'\nsource='profile_home'\n"
                "[[servers.env]]\nname='DEMO_ASSETS'\nsource='dependency_root'\ndependency='assets'\n"
                "[[servers.env]]\nname='HTTP_PROXY'\nsource='host'\n"
            )
            source_files_before = {
                path.relative_to(tool): path.read_bytes()
                for path in tool.rglob("*")
                if path.is_file()
            }
            process_data = root / "data"
            profile = root / "profile"
            profile.mkdir()

            commands = []
            staged_source_files: list[set[str]] = []

            def fake_run(command, cwd, environment):
                commands.append(tuple(command))
                if command[1:3] == ("-m", "venv"):
                    python = Path(command[3]) / "bin/python"
                    python.parent.mkdir(parents=True)
                    python.write_text("python")
                elif "wheel" in command:
                    staged_source_files.append(
                        {
                            path.relative_to(cwd).as_posix()
                            for path in cwd.rglob("*")
                            if path.is_file()
                        }
                    )
                    wheel_dir = Path(command[command.index("--wheel-dir") + 1])
                    wheel_dir.mkdir(parents=True, exist_ok=True)
                    (wheel_dir / "demo-0.1.0-py3-none-any.whl").write_text("wheel")

            prepared = prepare_tool_composition(
                source_root=root,
                tools=(ToolRuntimeSource("demo", tool, Path("tools/demo/runtime.toml")),),
                output_root=process_data / "prepared",
                composition_descriptor_sha256="abc",
                host_environment={
                    "HTTP_PROXY": "http://proxy",
                    "PATH": os.environ["PATH"],
                },
                run_command=fake_run,
                executable_identity_resolver=lambda name, env: (
                    Path("/fake") / name,
                    100,
                    200,
                ),
            )
            self.assertEqual(prepared.state, "built")

            descriptor = json.loads(prepared.descriptor_path.read_text())
            descriptor_bindings = descriptor["capabilityRoots"][0]["servers"][0]["envBindings"]
            self.assertEqual(descriptor_bindings[0], {"name": "CODEX_HOME", "source": "profile_home"})
            self.assertNotIn("resolvedRoot", descriptor_bindings[0])
            materialized = materialize_capability_roots(
                prepared,
                profile_home=profile,
                state_root=process_data / "runtime",
            )
            capability = materialized.capability_roots[0]
            server = capability.servers[0]
            self.assertEqual(server.args, ("-m", "demo.server", "--flag"))
            self.assertEqual(server.env["CODEX_HOME"], str(profile.resolve()))
            self.assertEqual(
                server.env["DEMO_ASSETS"],
                descriptor_bindings[1]["resolvedRoot"],
            )
            self.assertEqual(server.env_vars, ("HTTP_PROXY",))
            projection = json.loads((capability.projection_root / ".mcp.json").read_text())
            self.assertEqual(projection["mcpServers"]["demo"]["command"], str(server.command))
            self.assertEqual(projection["mcpServers"]["demo"]["startup_timeout_sec"], 60)
            self.assertEqual(projection["mcpServers"]["demo"]["tool_timeout_sec"], 90)
            plugin = json.loads(
                (capability.projection_root / ".codex-plugin/plugin.json").read_text()
            )
            self.assertEqual(plugin["interface"]["displayName"], "demo")
            self.assertEqual(descriptor["schemaVersion"], 1)
            self.assertEqual(descriptor["compositionDescriptorSha256"], "abc")
            self.assertEqual(descriptor["capabilityRoots"][0]["id"], "demo")
            self.assertEqual(
                descriptor_bindings,
                [
                    {"name": "CODEX_HOME", "source": "profile_home"},
                    {
                        "dependency": "assets",
                        "name": "DEMO_ASSETS",
                        "resolvedRoot": descriptor_bindings[1]["resolvedRoot"],
                        "source": "dependency_root",
                    },
                    {"name": "HTTP_PROXY", "source": "host"},
                ],
            )
            self.assertEqual(
                descriptor["capabilityRoots"][0]["servers"][0]["startupTimeoutSec"],
                60,
            )
            self.assertEqual(
                descriptor["capabilityRoots"][0]["servers"][0]["toolTimeoutSec"],
                90,
            )
            source_files_after = {
                path.relative_to(tool): path.read_bytes()
                for path in tool.rglob("*")
                if path.is_file()
            }
            self.assertEqual(source_files_after, source_files_before)
            wheel_command = next(command for command in commands if "wheel" in command)
            self.assertIn("--no-build-isolation", wheel_command)
            self.assertTrue((process_data / "prepared/.copilot-tool-environment.json").is_file())
            built_command_count = len(commands)
            rebuilt = prepare_tool_composition(
                source_root=root,
                tools=(ToolRuntimeSource("demo", tool, Path("tools/demo/runtime.toml")),),
                output_root=process_data / "prepared",
                composition_descriptor_sha256="abc",
                host_environment={"PATH": os.environ["PATH"]},
                run_command=fake_run,
                executable_identity_resolver=lambda name, env: (
                    Path("/fake") / name,
                    100,
                    200,
                ),
            )
            self.assertEqual(rebuilt.descriptor_path, prepared.descriptor_path)
            self.assertEqual(rebuilt.state, "reused")
            self.assertEqual(len(commands), built_command_count)
            prompt_only_change = prepare_tool_composition(
                source_root=root,
                tools=(ToolRuntimeSource("demo", tool, Path("tools/demo/runtime.toml")),),
                output_root=process_data / "prepared",
                composition_descriptor_sha256="def",
                host_environment={"PATH": os.environ["PATH"]},
                run_command=fake_run,
                executable_identity_resolver=lambda name, env: (
                    Path("/fake") / name,
                    100,
                    200,
                ),
            )
            self.assertEqual(prompt_only_change.state, "reused")
            self.assertEqual(len(commands), built_command_count)
            self.assertEqual(
                json.loads(prompt_only_change.descriptor_path.read_text())[
                    "compositionDescriptorSha256"
                ],
                "def",
            )
            for root_item in rebuilt.capability_roots:
                for server_item in root_item.servers:
                    self.assertTrue(server_item.command.is_file())
                    self.assertNotIn(".copilot-prepare-", str(server_item.command))
                    for binding in server_item.env_bindings:
                        if "resolvedRoot" in binding:
                            self.assertTrue(Path(binding["resolvedRoot"]).is_dir())
                            self.assertNotIn(".copilot-prepare-", binding["resolvedRoot"])

            excluded_entries = {
                ".codex/provider-state.json": "runtime state",
                ".git/config": "repository metadata",
                ".pytest_cache/v/cache/nodeids": "test cache",
                ".ruff_cache/cache-entry": "lint cache",
                ".venv/marker": "local environment",
                "__pycache__/server.cpython-313.pyc": "bytecode cache",
                "build/output.txt": "build output",
                "dist/demo.whl": "distribution output",
                "node_modules/package/index.js": "node dependency",
                ".DS_Store": "finder metadata",
                "demo/compiled.pyc": "nested bytecode",
            }
            for relative, contents in excluded_entries.items():
                path = tool / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(contents, encoding="utf-8")
            ignored_mutation = prepare_tool_composition(
                source_root=root,
                tools=(ToolRuntimeSource("demo", tool, Path("tools/demo/runtime.toml")),),
                output_root=process_data / "prepared",
                composition_descriptor_sha256="abc",
                host_environment={"PATH": os.environ["PATH"]},
                run_command=fake_run,
                executable_identity_resolver=lambda name, env: (
                    Path("/fake") / name,
                    100,
                    200,
                ),
            )
            self.assertEqual(ignored_mutation.state, "reused")
            self.assertEqual(len(commands), built_command_count)

            (tool / "demo/source.py").parent.mkdir(exist_ok=True)
            (tool / "demo/source.py").write_text("changed", encoding="utf-8")
            changed = prepare_tool_composition(
                source_root=root,
                tools=(ToolRuntimeSource("demo", tool, Path("tools/demo/runtime.toml")),),
                output_root=process_data / "prepared",
                composition_descriptor_sha256="abc",
                host_environment={"PATH": os.environ["PATH"]},
                run_command=fake_run,
                executable_identity_resolver=lambda name, env: (
                    Path("/fake") / name,
                    100,
                    200,
                ),
            )
            self.assertEqual(changed.state, "built")
            self.assertIn("demo/source.py", staged_source_files[-1])
            self.assertTrue(
                all(relative not in staged_source_files[-1] for relative in excluded_entries)
            )
            after_source_change = len(commands)
            identity_changed = prepare_tool_composition(
                source_root=root,
                tools=(ToolRuntimeSource("demo", tool, Path("tools/demo/runtime.toml")),),
                output_root=process_data / "prepared",
                composition_descriptor_sha256="abc",
                host_environment={"PATH": os.environ["PATH"]},
                run_command=fake_run,
                executable_identity_resolver=lambda name, env: (
                    Path("/fake") / name,
                    100,
                    201,
                ),
            )
            self.assertEqual(identity_changed.state, "built")
            self.assertGreater(len(commands), after_source_change)
            descriptor_server = json.loads(
                identity_changed.descriptor_path.read_text(encoding="utf-8")
            )["capabilityRoots"][0]["servers"][0]
            self.assertNotIn("cwd", descriptor_server)
            materialized_again = materialize_capability_roots(
                identity_changed,
                profile_home=profile,
                state_root=process_data / "runtime-again",
            )
            projection_server = json.loads(
                (
                    materialized_again.capability_roots[0].projection_root
                    / ".mcp.json"
                ).read_text(encoding="utf-8")
            )["mcpServers"]["demo"]
            self.assertNotIn("cwd", projection_server)

    def test_rejects_nested_source_symlink_without_copying_external_content(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            tool = root / "tools/demo"
            nested = tool / "demo/nested"
            nested.mkdir(parents=True)
            external = root / "private.txt"
            external.write_text("must not be copied", encoding="utf-8")
            (nested / "private.txt").symlink_to(external)
            (tool / "pyproject.toml").write_text(
                "[build-system]\nrequires=['setuptools>=77']\n"
                "build-backend='setuptools.build_meta'\n"
                "[project]\nname='demo'\nversion='0.1.0'\n", encoding="utf-8"
            )
            (tool / "requirements.lock").write_text(
                "mcp==1.0.0 \\\n    --hash=sha256:" + "a" * 64 + "\n"
                "setuptools==80.9.0 \\\n    --hash=sha256:" + "b" * 64 + "\n",
                encoding="utf-8",
            )
            (tool / "runtime.toml").write_text(
                "schema_version=1\n"
                "[[dependencies]]\nid='python'\nkind='python-project'\n"
                "manifest='pyproject.toml'\nlock='requirements.lock'\n"
                "[[servers]]\nid='demo'\n"
                "entry={kind='python-module',dependency='python',module='demo.server'}\n",
                encoding="utf-8",
            )
            profile = root / "profile"
            profile.mkdir()

            def fake_run(command, cwd, environment):
                if command[1:3] == ("-m", "venv"):
                    python = Path(command[3]) / "bin/python"
                    python.parent.mkdir(parents=True)
                    python.write_text("python", encoding="utf-8")

            with self.assertRaises(Exception) as caught:
                prepare_tool_composition(
                    source_root=root,
                    tools=(
                        ToolRuntimeSource("demo", tool, Path("tools/demo/runtime.toml")),
                    ),
                    output_root=root / "data/prepared",
                    composition_descriptor_sha256="abc",
                    host_environment={"PATH": os.environ["PATH"]},
                    run_command=fake_run,
                    executable_identity_resolver=lambda name, env: (
                        Path("/fake") / name,
                        100,
                        200,
                    ),
                )

            self.assertEqual(getattr(caught.exception, "code", None), "UnsafePath")
            copied = list((root / "data").rglob("private.txt"))
            self.assertEqual(copied, [])

    def test_failed_first_build_leaves_owned_root_that_same_input_can_rebuild(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            tool = self._write_python_tool(root)
            output = root / "prepared"
            identity = lambda name, env: (Path("/fake/python3"), 100, 200)

            def fail(command, cwd, environment):
                raise ToolEnvironmentError(
                    "EnvironmentUnavailable", str(cwd), "simulated build failure"
                )

            with self.assertRaises(ToolEnvironmentError):
                prepare_tool_composition(
                    source_root=root,
                    tools=(ToolRuntimeSource("demo", tool, Path("tools/demo/runtime.toml")),),
                    output_root=output,
                    composition_descriptor_sha256="abc",
                    host_environment={"PATH": os.environ["PATH"]},
                    run_command=fail,
                    executable_identity_resolver=identity,
                )
            self.assertTrue((output / ".copilot-tool-environment.json").is_file())
            self.assertFalse((output / "copilot-sdk/prepared-tools.v1.json").exists())

            def succeed(command, cwd, environment):
                if command[1:3] == ("-m", "venv"):
                    python = Path(command[3]) / "bin/python"
                    python.parent.mkdir(parents=True)
                    python.write_text("python", encoding="utf-8")
                elif "wheel" in command:
                    wheel_dir = Path(command[command.index("--wheel-dir") + 1])
                    wheel_dir.mkdir(parents=True, exist_ok=True)
                    (wheel_dir / "demo-0.1.0-py3-none-any.whl").write_text(
                        "wheel", encoding="utf-8"
                    )

            prepared = prepare_tool_composition(
                source_root=root,
                tools=(ToolRuntimeSource("demo", tool, Path("tools/demo/runtime.toml")),),
                output_root=output,
                composition_descriptor_sha256="abc",
                host_environment={"PATH": os.environ["PATH"]},
                run_command=succeed,
                executable_identity_resolver=identity,
            )
            self.assertEqual(prepared.state, "built")
            self.assertTrue(prepared.descriptor_path.is_file())

    def test_foreign_nonempty_output_root_remains_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            tool = self._write_python_tool(root)
            output = root / "foreign"
            output.mkdir()
            sentinel = output / "keep.txt"
            sentinel.write_text("foreign", encoding="utf-8")

            with self.assertRaises(ToolEnvironmentError) as caught:
                prepare_tool_composition(
                    source_root=root,
                    tools=(ToolRuntimeSource("demo", tool, Path("tools/demo/runtime.toml")),),
                    output_root=output,
                    composition_descriptor_sha256="abc",
                    host_environment={"PATH": os.environ["PATH"]},
                    run_command=lambda command, cwd, environment: None,
                    executable_identity_resolver=lambda name, env: (
                        Path("/fake/python3"), 100, 200
                    ),
                )
            self.assertEqual(caught.exception.code, "OutputConflict")
            self.assertEqual(sentinel.read_text(encoding="utf-8"), "foreign")


if __name__ == "__main__":
    unittest.main()
