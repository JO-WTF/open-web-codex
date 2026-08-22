from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from unittest.mock import patch

from copilot_sdk.platform_packages import PlatformPackageSpec, ResolvedPlatformPackage
from copilot_sdk.tool_environment import (
    OWNER_MARKER,
    PREPARED_DESCRIPTOR,
    PreparedDelivery,
    ToolEnvironmentError,
    ToolRuntimeSource,
    _build_lock,
    garbage_collect_tool_builds,
    materialize_capability_roots,
    prepare_tool_composition,
)


class ToolEnvironmentTests(unittest.TestCase):
    def _write_package_source(self, root: Path, package_id: str = "one") -> Path:
        package_root = root / "packages" / package_id
        package_root.mkdir(parents=True)
        (package_root / "copilot.toml").write_text(
            f'schema_version = 1\nid = "{package_id}"\n', encoding="utf-8"
        )
        return root / "packages"

    def _gc(self, root: Path, store: Path, package_id: str = "one") -> dict[str, int]:
        packages_root = self._write_package_source(root, package_id)
        self._set_marker_source(root / "prepared" / package_id, packages_root / package_id)
        return garbage_collect_tool_builds(
            prepared_root=root / "prepared",
            build_store_root=store,
            packages_root=packages_root,
            active_package_ids=(package_id,),
        )

    def _set_marker_source(self, package_root: Path, source_root: Path) -> None:
        marker_path = package_root / OWNER_MARKER
        marker = json.loads(marker_path.read_text(encoding="utf-8"))
        marker["sourceRoot"] = str(source_root.resolve())
        marker_path.write_text(json.dumps(marker), encoding="utf-8")

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

    def _prepare_fake(
        self,
        root: Path,
        tool: Path,
        output_root: Path,
        build_store_root: Path,
        commands: list[tuple[str, ...]] | None = None,
        environments: list[dict[str, str]] | None = None,
        run_command=None,
    ):
        captured_commands = [] if commands is None else commands
        captured_environments = [] if environments is None else environments

        def fake_run(command, cwd, environment):
            captured_commands.append(tuple(command))
            captured_environments.append(dict(environment))
            if run_command is not None:
                return run_command(command, cwd, environment)
            if command[1:3] == ("-m", "venv"):
                python = Path(command[-1]) / "bin/python"
                python.parent.mkdir(parents=True, exist_ok=True)
                python.write_text("python", encoding="utf-8")
            elif "wheel" in command:
                wheel_dir = Path(command[command.index("--wheel-dir") + 1])
                wheel_dir.mkdir(parents=True, exist_ok=True)
                (wheel_dir / "demo-0.1.0-py3-none-any.whl").write_text(
                    "wheel", encoding="utf-8"
                )

        return prepare_tool_composition(
            source_root=root,
            tools=(ToolRuntimeSource("demo", tool, Path("tools/demo/runtime.toml")),),
            output_root=output_root,
            build_store_root=build_store_root,
            composition_descriptor_sha256="a" * 64,
            host_environment={"PATH": os.environ["PATH"]},
            run_command=fake_run,
            executable_identity_resolver=lambda name, env: (
                Path("/fake") / name,
                100,
                200,
            ),
        )

    def test_identical_tool_fingerprint_shares_one_immutable_build(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            tool = self._write_python_tool(root)
            store = root / "tool-builds"
            first = self._prepare_fake(root, tool, root / "prepared/one", store)
            commands: list[tuple[str, ...]] = []
            second = self._prepare_fake(root, tool, root / "prepared/two", store, commands)

            self.assertEqual(first.state, "built")
            self.assertEqual(second.state, "reused")
            first_command = first.capability_roots[0].servers[0].command
            second_command = second.capability_roots[0].servers[0].command
            self.assertEqual(first_command, second_command)
            self.assertTrue(first_command.is_relative_to((store / "builds").resolve()))
            self.assertFalse((root / "prepared/one/builds").exists())
            self.assertFalse((root / "prepared/two/builds").exists())
            self.assertEqual(len(commands), 0)
            self.assertEqual(len(list((store / "builds").iterdir())), 1)

    def test_real_python_venv_command_is_a_regular_file_in_shared_build(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            tool = self._write_python_tool(root)

            def hybrid_run(command, cwd, environment):
                if command[1:3] == ("-m", "venv"):
                    subprocess.run(command, cwd=cwd, env=dict(environment), check=True)
                    return
                if "wheel" in command:
                    wheel_dir = Path(command[command.index("--wheel-dir") + 1])
                    wheel_dir.mkdir(parents=True, exist_ok=True)
                    (wheel_dir / "demo-0.1.0-py3-none-any.whl").write_text(
                        "wheel", encoding="utf-8"
                    )

            prepared = prepare_tool_composition(
                source_root=root,
                tools=(ToolRuntimeSource("demo", tool, Path("tools/demo/runtime.toml")),),
                output_root=root / "prepared",
                build_store_root=root / "tool-builds",
                composition_descriptor_sha256="a" * 64,
                host_environment={"PATH": os.environ["PATH"]},
                run_command=hybrid_run,
                executable_identity_resolver=lambda name, env: (
                    Path(sys.executable),
                    Path(sys.executable).stat().st_size,
                    Path(sys.executable).stat().st_mtime_ns,
                ),
            )
            command = prepared.capability_roots[0].servers[0].command
            self.assertTrue(command.is_file())
            self.assertFalse(command.is_symlink())

    def test_different_tool_fingerprints_are_isolated(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            tool = self._write_python_tool(root)
            store = root / "tool-builds"
            first = self._prepare_fake(root, tool, root / "prepared/one", store)
            (tool / "demo/source.py").parent.mkdir(exist_ok=True)
            (tool / "demo/source.py").write_text("changed", encoding="utf-8")
            second = self._prepare_fake(root, tool, root / "prepared/two", store)

            self.assertEqual(first.state, "built")
            self.assertEqual(second.state, "built")
            self.assertNotEqual(
                first.capability_roots[0].servers[0].command,
                second.capability_roots[0].servers[0].command,
            )
            self.assertEqual(len(list((store / "builds").iterdir())), 2)

    def test_concurrent_winner_and_loser_publish_one_valid_build(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            tool = self._write_python_tool(root)
            store = root / "tool-builds"

            def prepare(index: int):
                return self._prepare_fake(root, tool, root / f"prepared/{index}", store)

            with ThreadPoolExecutor(max_workers=2) as pool:
                results = list(pool.map(prepare, (1, 2)))

            self.assertEqual(sorted(result.state for result in results), ["built", "reused"])
            self.assertEqual(len(list((store / "builds").iterdir())), 1)
            for result in results:
                descriptor = json.loads(result.descriptor_path.read_text(encoding="utf-8"))
                command = Path(descriptor["capabilityRoots"][0]["servers"][0]["command"])
                self.assertTrue(command.is_file())
                self.assertFalse(result.descriptor_path.with_suffix(".tmp").exists())

    def test_concurrent_publish_without_fcntl_reuses_winner(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            tool = self._write_python_tool(root)
            store = root / "tool-builds"

            def prepare(index: int):
                return self._prepare_fake(root, tool, root / f"prepared/{index}", store)

            with patch("copilot_sdk.tool_environment.fcntl", None), ThreadPoolExecutor(
                max_workers=2
            ) as pool:
                results = list(pool.map(prepare, (1, 2)))

            self.assertEqual(sorted(result.state for result in results), ["built", "reused"])
            self.assertEqual(len(list((store / "builds").iterdir())), 1)

    def test_failed_rebuild_preserves_old_descriptor_and_build(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            tool = self._write_python_tool(root)
            store = root / "tool-builds"
            output = root / "prepared"
            prepared = self._prepare_fake(root, tool, output, store)
            descriptor_before = prepared.descriptor_path.read_bytes()
            marker_before = (output / OWNER_MARKER).read_bytes()
            (tool / "demo/source.py").parent.mkdir(exist_ok=True)
            (tool / "demo/source.py").write_text("changed", encoding="utf-8")

            def fail(command, cwd, environment):
                raise ToolEnvironmentError("EnvironmentUnavailable", str(cwd), "simulated failure")

            with self.assertRaises(ToolEnvironmentError):
                self._prepare_fake(root, tool, output, store, run_command=fail)
            self.assertEqual(prepared.descriptor_path.read_bytes(), descriptor_before)
            self.assertEqual((output / OWNER_MARKER).read_bytes(), marker_before)
            self.assertTrue(prepared.capability_roots[0].servers[0].command.is_file())
            self.assertEqual(len(list((store / "builds").iterdir())), 1)

    def test_descriptor_write_failure_preserves_old_descriptor_and_marker_pair(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            tool = self._write_python_tool(root)
            store = root / "tool-builds"
            output = root / "prepared"
            prepared = self._prepare_fake(root, tool, output, store)
            descriptor_before = prepared.descriptor_path.read_bytes()
            marker_before = (output / OWNER_MARKER).read_bytes()
            (tool / "demo/source.py").parent.mkdir(exist_ok=True)
            (tool / "demo/source.py").write_text("changed", encoding="utf-8")
            with (
                patch(
                    "copilot_sdk.tool_environment._write_prepared_descriptor",
                    side_effect=ToolEnvironmentError(
                        "EnvironmentUnavailable", str(prepared.descriptor_path), "write failed"
                    ),
                ),
                self.assertRaises(ToolEnvironmentError),
            ):
                self._prepare_fake(root, tool, output, store)
            self.assertEqual(prepared.descriptor_path.read_bytes(), descriptor_before)
            self.assertEqual((output / OWNER_MARKER).read_bytes(), marker_before)

    def test_marker_write_failure_is_repairable_on_next_prepare(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            tool = self._write_python_tool(root)
            store = root / "tool-builds"
            output = root / "prepared"
            self._prepare_fake(root, tool, output, store)
            (tool / "demo/source.py").parent.mkdir(exist_ok=True)
            (tool / "demo/source.py").write_text("changed", encoding="utf-8")
            original_marker = (output / OWNER_MARKER).read_bytes()
            with (
                patch(
                    "copilot_sdk.tool_environment._write_owner_marker",
                    side_effect=ToolEnvironmentError(
                        "EnvironmentUnavailable", str(output / OWNER_MARKER), "marker write failed"
                    ),
                ),
                self.assertRaises(ToolEnvironmentError),
            ):
                self._prepare_fake(root, tool, output, store)
            self.assertEqual((output / OWNER_MARKER).read_bytes(), original_marker)
            repaired = self._prepare_fake(root, tool, output, store)
            self.assertEqual(repaired.state, "reused")
            self.assertTrue((output / OWNER_MARKER).is_file())

    def test_owned_legacy_descriptor_is_upgraded_to_shared_store(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            tool = self._write_python_tool(root)
            store = root / "tool-builds"
            output = root / "prepared"
            self._prepare_fake(root, tool, output, store)
            fingerprint = next((store / "builds").iterdir()).name
            shared_build = store / "builds" / fingerprint
            legacy_build = output / "builds" / fingerprint
            shutil.copytree(shared_build, legacy_build)
            descriptor_path = output / PREPARED_DESCRIPTOR
            descriptor = json.loads(descriptor_path.read_text(encoding="utf-8"))

            def rewrite(value):
                if isinstance(value, dict):
                    return {key: rewrite(item) for key, item in value.items()}
                if isinstance(value, list):
                    return [rewrite(item) for item in value]
                if isinstance(value, str):
                    return value.replace(str(shared_build), str(legacy_build))
                return value

            descriptor_path.write_text(json.dumps(rewrite(descriptor)), encoding="utf-8")
            shutil.rmtree(shared_build)
            rebuilt = self._prepare_fake(root, tool, output, store)

            self.assertEqual(rebuilt.state, "built")
            self.assertTrue(
                rebuilt.capability_roots[0].servers[0].command.is_relative_to(
                    (store / "builds").resolve()
                )
            )
            self.assertTrue(legacy_build.is_dir())

    def test_build_cache_is_stable_and_npm_ci_is_offline_bounded(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            tool = self._write_python_tool(root)
            (tool / "package.json").write_text('{"name":"demo-assets","version":"1.0.0"}\n')
            (tool / "package-lock.json").write_text(
                '{"name":"demo-assets","version":"1.0.0","lockfileVersion":3,'
                '"packages":{"":{"name":"demo-assets","version":"1.0.0"}}}\n'
            )
            runtime = tool / "runtime.toml"
            runtime.write_text(
                runtime.read_text(encoding="utf-8").replace(
                    "[[servers]]",
                    "[[dependencies]]\nid='assets'\nkind='node-project'\n"
                    "manifest='package.json'\nlock='package-lock.json'\n[[servers]]",
                ),
                encoding="utf-8",
            )
            commands: list[tuple[str, ...]] = []
            environments: list[dict[str, str]] = []
            self._prepare_fake(root, tool, root / "prepared", root / "tool-builds", commands, environments)
            npm_command = next(command for command in commands if command[1] == "ci")
            self.assertEqual(
                npm_command[2:],
                ("--prefer-offline", "--no-audit", "--no-fund", "--ignore-scripts"),
            )
            npm_env = environments[commands.index(npm_command)]
            self.assertEqual(
                npm_env["npm_config_cache"], str((root / "tool-builds/cache/npm").resolve())
            )
            self.assertFalse(
                Path(npm_env["npm_config_cache"]).is_relative_to(
                    (root / "tool-builds/builds").resolve()
                )
            )

    def test_gc_keeps_referenced_build_and_removes_unreferenced_build(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            tool = self._write_python_tool(root)
            store = root / "tool-builds"
            first = self._prepare_fake(root, tool, root / "prepared/one", store)
            (tool / "demo/source.py").parent.mkdir(exist_ok=True)
            (tool / "demo/source.py").write_text("changed", encoding="utf-8")
            second = self._prepare_fake(root, tool, root / "prepared/two", store)
            second_build = second.capability_roots[0].servers[0].command.parents[6]
            for name in ("builds", "pip-cache", "npm-cache", "tmp"):
                legacy = root / "prepared/one" / name
                legacy.mkdir(parents=True)
                (legacy / "stale.bin").write_bytes(b"stale")
            shutil.rmtree(root / "prepared/two")

            result = self._gc(root, store)
            self.assertEqual(result["descriptors"], 1)
            self.assertEqual(result["removed"], 1)
            self.assertEqual(result["legacyRemovedEntries"], 4)
            self.assertEqual(result["legacyRemovedBytes"], 20)
            self.assertTrue(first.capability_roots[0].servers[0].command.is_file())
            self.assertFalse(second_build.exists())

    def test_gc_uses_active_ids_and_removes_superseded_historical_package(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            tool = self._write_python_tool(root)
            store = root / "tool-builds"
            prepared_root = root / "prepared"
            multi = self._prepare_fake(root, tool, prepared_root / "multi", store)
            shutil.copytree(prepared_root / "multi", prepared_root / "single")
            packages_root = root / "packages"
            multi_source = packages_root / "multi-source"
            single_source = packages_root / "single-source"
            for source, package_id in ((multi_source, "multi"), (single_source, "single")):
                source.mkdir(parents=True)
                (source / "copilot.toml").write_text(
                    f'schema_version = 1\nid = "{package_id}"\n', encoding="utf-8"
                )
            self._set_marker_source(prepared_root / "multi", multi_source)
            self._set_marker_source(prepared_root / "single", single_source)

            stale = prepared_root / "warehouse-network"
            shutil.copytree(prepared_root / "multi", stale)
            self._set_marker_source(stale, multi_source)
            (stale / PREPARED_DESCRIPTOR).write_text('{"malformed": true}', encoding="utf-8")
            unknown = prepared_root / "old-unknown"
            unknown.mkdir()
            (unknown / "keep.txt").write_text("keep", encoding="utf-8")

            result = garbage_collect_tool_builds(
                prepared_root=prepared_root,
                build_store_root=store,
                packages_root=packages_root,
                active_package_ids=("multi", "single"),
            )

            self.assertEqual(result["descriptors"], 2)
            self.assertEqual(result["staleRemovedEntries"], 1)
            self.assertEqual(result["unknownSkippedEntries"], 1)
            self.assertFalse(stale.exists())
            self.assertTrue(unknown.is_dir())
            self.assertTrue(multi.capability_roots[0].servers[0].command.is_file())

    def test_gc_active_missing_descriptor_fails_before_any_cleanup(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            tool = self._write_python_tool(root)
            store = root / "tool-builds"
            prepared_root = root / "prepared"
            prepared = self._prepare_fake(root, tool, prepared_root / "multi", store)
            packages_root = root / "packages"
            source = packages_root / "multi-source"
            source.mkdir(parents=True)
            (source / "copilot.toml").write_text('schema_version = 1\nid = "multi"\n', encoding="utf-8")
            self._set_marker_source(prepared_root / "multi", source)
            descriptor = prepared_root / "multi" / PREPARED_DESCRIPTOR
            descriptor.unlink()
            with self.assertRaises(ToolEnvironmentError) as caught:
                garbage_collect_tool_builds(
                    prepared_root=prepared_root,
                    build_store_root=store,
                    packages_root=packages_root,
                    active_package_ids=("multi",),
                )
            self.assertEqual(caught.exception.code, "OutputConflict")
            self.assertTrue(prepared.capability_roots[0].servers[0].command.is_file())

    def test_gc_rejects_descriptor_referencing_path_outside_build_store(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            prepared_root = root / "prepared/pkg"
            prepared_root.mkdir(parents=True)
            (prepared_root / PREPARED_DESCRIPTOR).parent.mkdir(parents=True, exist_ok=True)
            fingerprint = "a" * 64
            (prepared_root / OWNER_MARKER).write_text(
                json.dumps({"schemaVersion": 1, "preparationFingerprint": fingerprint}),
                encoding="utf-8",
            )
            (prepared_root / PREPARED_DESCRIPTOR).write_text(
                json.dumps(
                    {
                        "schemaVersion": 1,
                        "compositionDescriptorSha256": "b" * 64,
                        "capabilityRoots": [
                            {
                                "id": "demo",
                                "servers": [
                                    {
                                        "id": "demo",
                                        "transport": "stdio",
                                        "command": "/tmp/not-owned/python",
                                        "args": [],
                                        "envBindings": [],
                                    }
                                ],
                            }
                        ],
                        "deliveries": [],
                    }
                ),
                encoding="utf-8",
            )
            with self.assertRaises(ToolEnvironmentError) as caught:
                self._gc(root, root / "tool-builds", "pkg")
            self.assertEqual(caught.exception.code, "UnsafePath")

    def test_gc_rejects_legacy_symlink_without_removing_it(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            tool = self._write_python_tool(root)
            store = root / "tool-builds"
            self._prepare_fake(root, tool, root / "prepared/one", store)
            target = root / "external-legacy"
            target.mkdir()
            legacy = root / "prepared/one/pip-cache"
            legacy.symlink_to(target, target_is_directory=True)
            with self.assertRaises(ToolEnvironmentError) as caught:
                self._gc(root, store)
            self.assertEqual(caught.exception.code, "UnsafePath")
            self.assertTrue(legacy.is_symlink())

    def test_gc_removes_legacy_internal_symlinks_without_following_targets(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            tool = self._write_python_tool(root)
            store = root / "tool-builds"
            self._prepare_fake(root, tool, root / "prepared/one", store)
            external_file = root / "external-python"
            external_file.write_text("keep", encoding="utf-8")
            external_directory = root / "external-directory"
            external_directory.mkdir()
            sentinel = external_directory / "sentinel"
            sentinel.write_text("keep", encoding="utf-8")
            legacy = root / "prepared/one/builds"
            (legacy / "venv/bin").mkdir(parents=True)
            (legacy / "venv/bin/python").symlink_to(external_file)
            (legacy / "venv/lib").mkdir(parents=True)
            (legacy / "venv/lib/site-packages").symlink_to(external_directory, target_is_directory=True)
            (legacy / "node_modules/.bin").mkdir(parents=True)
            (legacy / "node_modules/.bin/tool").symlink_to(external_file)

            result = self._gc(root, store)

            self.assertEqual(result["legacyRemovedEntries"], 1)
            self.assertGreater(result["legacyRemovedBytes"], 0)
            self.assertFalse(legacy.exists())
            self.assertEqual(external_file.read_text(encoding="utf-8"), "keep")
            self.assertEqual(sentinel.read_text(encoding="utf-8"), "keep")

    def test_windows_lock_fails_bounded_on_stale_lock(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            lock = Path(directory) / "fingerprint.lock"
            stale = lock.with_suffix(lock.suffix + ".d")
            stale.mkdir()
            with patch("copilot_sdk.tool_environment.fcntl", None), patch(
                "copilot_sdk.tool_environment.time.sleep", return_value=None
            ), self.assertRaises(ToolEnvironmentError) as caught, _build_lock(lock):
                pass
            self.assertEqual(caught.exception.code, "EnvironmentUnavailable")
            self.assertTrue(stale.is_dir())

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
                    python = Path(command[-1]) / "bin/python"
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
                build_store_root=process_data / "build-store",
                composition_descriptor_sha256="abc",
                deliveries=(
                    PreparedDelivery(
                        "result",
                        "demo",
                        "publish_result",
                        "workspace_artifact",
                        "demo.v1",
                        "application/json",
                        "Demo result",
                        {
                            "kind": "json_schema",
                            "value": {"type": "object"},
                        },
                    ),
                ),
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
                descriptor["deliveries"][0],
                {
                    "contentVerifier": {
                        "kind": "json_schema",
                        "value": {"type": "object"},
                    },
                    "displayName": "Demo result",
                    "id": "result",
                    "kind": "workspace_artifact",
                    "mimeType": "application/json",
                    "schema": "demo.v1",
                    "server": "demo",
                    "tool": "publish_result",
                },
            )
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
                build_store_root=process_data / "build-store",
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
                build_store_root=process_data / "build-store",
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
                build_store_root=process_data / "build-store",
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
                build_store_root=process_data / "build-store",
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
                build_store_root=process_data / "build-store",
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

    def test_installs_registered_platform_package_from_sdk_distribution(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            tool = self._write_python_tool(root)
            runtime = tool / "runtime.toml"
            runtime.write_text(
                runtime.read_text(encoding="utf-8").replace(
                    "lock='requirements.lock'",
                    "lock='requirements.lock'\n"
                    "platform_packages=['open-web-codex-provider-sdk']",
                ),
                encoding="utf-8",
            )
            manifest = tool / "pyproject.toml"
            manifest.write_text(
                manifest.read_text(encoding="utf-8").replace(
                    "version='0.1.0'",
                    "version='0.1.0'\n"
                    "dependencies=['open-web-codex-provider-sdk>=0.1,<0.2']",
                ),
                encoding="utf-8",
            )
            installed_package = root / "installed/open_web_codex_provider"
            installed_package.mkdir(parents=True)
            (installed_package / "__init__.py").write_text(
                "VALUE = 'provider'\n", encoding="utf-8"
            )
            resolved = ResolvedPlatformPackage(
                PlatformPackageSpec(
                    "open-web-codex-provider-sdk",
                    "open-web-codex-provider-sdk",
                    "0.1.",
                    ("open_web_codex_provider",),
                ),
                "0.1.0",
                (installed_package,),
            )
            commands: list[tuple[str, ...]] = []

            def fake_run(command, cwd, environment):
                commands.append(tuple(command))
                if command[1:3] == ("-m", "venv"):
                    python = Path(command[-1]) / "bin/python"
                    python.parent.mkdir(parents=True)
                    python.write_text("python", encoding="utf-8")
                elif "wheel" in command:
                    wheel_dir = Path(command[command.index("--wheel-dir") + 1])
                    wheel_dir.mkdir(parents=True, exist_ok=True)
                    (wheel_dir / "demo-0.1.0-py3-none-any.whl").write_text(
                        "tool wheel", encoding="utf-8"
                    )

            prepared = prepare_tool_composition(
                source_root=root,
                tools=(ToolRuntimeSource("demo", tool, Path("tools/demo/runtime.toml")),),
                output_root=root / "data/prepared",
                build_store_root=root / "data/build-store",
                composition_descriptor_sha256="abc",
                host_environment={"PATH": os.environ["PATH"]},
                run_command=fake_run,
                executable_identity_resolver=lambda name, env: (
                    Path("/fake") / name,
                    100,
                    200,
                ),
                platform_package_resolver=lambda package_id: resolved,
            )

            self.assertEqual(prepared.state, "built")
            platform_installs = [
                command
                for command in commands
                if "pip" in command
                and "install" in command
                and any(item.endswith("py3-none-any.whl") for item in command)
                and any("provider_sdk" in item for item in command)
            ]
            self.assertEqual(len(platform_installs), 1)

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
                    python = Path(command[-1]) / "bin/python"
                    python.parent.mkdir(parents=True)
                    python.write_text("python", encoding="utf-8")

            with self.assertRaises(Exception) as caught:
                prepare_tool_composition(
                    source_root=root,
                    tools=(
                        ToolRuntimeSource("demo", tool, Path("tools/demo/runtime.toml")),
                    ),
                    output_root=root / "data/prepared",
                    build_store_root=root / "data/build-store",
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
                    build_store_root=root / "build-store",
                    composition_descriptor_sha256="abc",
                    host_environment={"PATH": os.environ["PATH"]},
                    run_command=fail,
                    executable_identity_resolver=identity,
                )
            self.assertFalse((output / ".copilot-tool-environment.json").exists())
            self.assertFalse((output / "copilot-sdk/prepared-tools.v1.json").exists())

            def succeed(command, cwd, environment):
                if command[1:3] == ("-m", "venv"):
                    python = Path(command[-1]) / "bin/python"
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
                build_store_root=root / "build-store",
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
                build_store_root=root / "build-store",
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
