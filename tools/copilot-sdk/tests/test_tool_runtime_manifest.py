from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

from copilot_sdk.tool_runtime_manifest import (
    ToolRuntimeManifestError,
    load_tool_runtime_manifest,
)


LOCK = """mcp==1.27.1 \\
    --hash=sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
setuptools==80.9.0 \\
    --hash=sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
"""


class ToolRuntimeManifestTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.tool = self.root / "tools" / "routes"
        self.tool.mkdir(parents=True)
        (self.tool / "pyproject.toml").write_text(
            '[build-system]\nrequires=["setuptools>=77"]\n'
            'build-backend="setuptools.build_meta"\n'
            '[project]\nname="routes"\nversion="0.1.0"\n', encoding="utf-8"
        )
        (self.tool / "requirements.lock").write_text(LOCK, encoding="utf-8")
        (self.tool / "package.json").write_text(
            json.dumps({"name": "route-assets", "version": "0.1.0"}), encoding="utf-8"
        )
        (self.tool / "package-lock.json").write_text(
            json.dumps(
                {
                    "name": "route-assets",
                    "lockfileVersion": 3,
                    "packages": {"": {"name": "route-assets", "version": "0.1.0"}},
                }
            ),
            encoding="utf-8",
        )
        self._write_runtime()

    def tearDown(self) -> None:
        self.temp.cleanup()

    def _write_runtime(self, extra: str = "") -> None:
        (self.tool / "runtime.toml").write_text(
            """schema_version = 1

[[dependencies]]
id = "python"
kind = "python-project"
manifest = "pyproject.toml"
lock = "requirements.lock"

[[dependencies]]
id = "assets"
kind = "node-project"
manifest = "package.json"
lock = "package-lock.json"

[[servers]]
id = "routing"
entry = { kind = "python-module", dependency = "python", module = "routes.server" }
args = ["--stdio"]

[[servers.env]]
name = "CODEX_HOME"
source = "profile_home"

[[servers.env]]
name = "ROUTE_STATE"
source = "tool_state_root"

[[servers.env]]
name = "ASSET_ROOT"
source = "dependency_root"
dependency = "assets"

[[servers.env]]
name = "HTTPS_PROXY"
source = "host"
"""
            + extra,
            encoding="utf-8",
        )

    def test_loads_typed_runtime_v1(self) -> None:
        runtime = load_tool_runtime_manifest(
            self.root, self.tool, "tools/routes/runtime.toml"
        )
        self.assertEqual([item.id for item in runtime.dependencies], ["python", "assets"])
        self.assertEqual(runtime.servers[0].entry.module, "routes.server")
        self.assertEqual(runtime.servers[0].env[2].dependency, "assets")

    def test_rejects_python_requirement_without_own_hash(self) -> None:
        (self.tool / "requirements.lock").write_text(
            LOCK + "httpx==0.28.1\n", encoding="utf-8"
        )
        with self.assertRaises(ToolRuntimeManifestError) as caught:
            load_tool_runtime_manifest(self.root, self.tool, "tools/routes/runtime.toml")
        self.assertEqual(caught.exception.code, "invalid_lock")

    def test_rejects_unlocked_python_build_backend(self) -> None:
        (self.tool / "requirements.lock").write_text(
            LOCK.split("setuptools==", 1)[0], encoding="utf-8"
        )
        with self.assertRaises(ToolRuntimeManifestError) as caught:
            load_tool_runtime_manifest(self.root, self.tool, "tools/routes/runtime.toml")
        self.assertEqual(caught.exception.code, "invalid_lock")

    def test_rejects_node_lock_name_or_version_contract_drift(self) -> None:
        lock = json.loads((self.tool / "package-lock.json").read_text())
        lock["lockfileVersion"] = 2
        (self.tool / "package-lock.json").write_text(json.dumps(lock), encoding="utf-8")
        with self.assertRaises(ToolRuntimeManifestError) as caught:
            load_tool_runtime_manifest(self.root, self.tool, "tools/routes/runtime.toml")
        self.assertEqual(caught.exception.code, "invalid_lock")

    def test_rejects_non_python_server_dependency(self) -> None:
        text = (self.tool / "runtime.toml").read_text(encoding="utf-8")
        (self.tool / "runtime.toml").write_text(
            text.replace('dependency = "python", module', 'dependency = "assets", module'),
            encoding="utf-8",
        )
        with self.assertRaises(ToolRuntimeManifestError) as caught:
            load_tool_runtime_manifest(self.root, self.tool, "tools/routes/runtime.toml")
        self.assertEqual(caught.exception.code, "missing_reference")

    def test_rejects_dependency_binding_without_reference(self) -> None:
        text = (self.tool / "runtime.toml").read_text(encoding="utf-8")
        (self.tool / "runtime.toml").write_text(
            text.replace('dependency = "assets"', 'dependency = "missing"'),
            encoding="utf-8",
        )
        with self.assertRaises(ToolRuntimeManifestError) as caught:
            load_tool_runtime_manifest(self.root, self.tool, "tools/routes/runtime.toml")
        self.assertEqual(caught.exception.code, "missing_reference")

    def test_rejects_host_binding_outside_proxy_capability(self) -> None:
        text = (self.tool / "runtime.toml").read_text(encoding="utf-8")
        (self.tool / "runtime.toml").write_text(
            text.replace('name = "HTTPS_PROXY"', 'name = "DATABASE_URL"'),
            encoding="utf-8",
        )

        with self.assertRaises(ToolRuntimeManifestError) as caught:
            load_tool_runtime_manifest(self.root, self.tool, "tools/routes/runtime.toml")

        self.assertEqual(caught.exception.code, "unsupported_host_environment")

    def test_rejects_runtime_outside_tool_root(self) -> None:
        (self.root / "runtime.toml").write_text(
            (self.tool / "runtime.toml").read_text(encoding="utf-8"), encoding="utf-8"
        )
        with self.assertRaises(ToolRuntimeManifestError) as caught:
            load_tool_runtime_manifest(self.root, self.tool, "runtime.toml")
        self.assertEqual(caught.exception.code, "invalid_path")


if __name__ == "__main__":
    unittest.main()
