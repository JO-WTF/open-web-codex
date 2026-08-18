from __future__ import annotations

import asyncio
import tomllib
import unittest
from pathlib import Path

import maps_mcp.server as maps_server


class PluginConfigTests(unittest.TestCase):
    def test_tool_author_source_has_no_runtime_transport_or_lifecycle_scripts(self) -> None:
        root = Path(__file__).parents[1]
        self.assertFalse((root / ".codex-plugin").exists())
        self.assertFalse((root / ".mcp.json").exists())
        self.assertFalse((root / "bin").exists())

    def test_maps_mcp_annotations_separate_local_and_external_tools(self) -> None:
        tools = {tool.name: tool for tool in asyncio.run(maps_server.mcp.list_tools())}
        for name in {"create_map_card", "revise_map_card"}:
            local = tools[name].annotations
            self.assertIsNotNone(local)
            assert local is not None
            self.assertEqual(
                local.model_dump(by_alias=True, exclude_none=True),
                {
                    "readOnlyHint": True,
                    "destructiveHint": False,
                    "idempotentHint": False,
                    "openWorldHint": False,
                },
            )
        for name in {"batch_geocode", "batch_reverse_geocode", "get_route", "distance_matrix"}:
            annotations = tools[name].annotations
            self.assertIsNotNone(annotations)
            assert annotations is not None
            self.assertEqual(
                annotations.model_dump(by_alias=True, exclude_none=True),
                {
                    "readOnlyHint": False,
                    "destructiveHint": False,
                    "idempotentHint": False,
                    "openWorldHint": True,
                },
            )

    def test_generic_runtime_declaration_owns_dependencies_and_server_entry(self) -> None:
        root = Path(__file__).parents[1]
        runtime = tomllib.loads((root / "runtime.toml").read_text())

        self.assertEqual(runtime["schema_version"], 1)
        self.assertEqual(
            runtime["dependencies"],
            [
                {
                    "id": "python",
                    "kind": "python-project",
                    "manifest": "pyproject.toml",
                    "lock": "requirements.lock",
                    "platform_packages": ["open-web-codex-provider-sdk"],
                },
                {
                    "id": "style-spec",
                    "kind": "node-project",
                    "manifest": "package.json",
                    "lock": "package-lock.json",
                },
            ],
        )
        self.assertEqual(len(runtime["servers"]), 1)
        server = runtime["servers"][0]
        self.assertEqual(server["id"], "map_utils")
        self.assertEqual(
            server["entry"],
            {
                "kind": "python-module",
                "dependency": "python",
                "module": "maps_mcp.server",
            },
        )
        bindings = {entry["name"]: entry for entry in server["env"]}
        self.assertEqual(bindings["OPEN_WEB_CODEX_DATA_DIR"]["source"], "tool_state_root")
        self.assertEqual(
            bindings["OPEN_WEB_CODEX_MAPS_NODE_ENV"],
            {
                "name": "OPEN_WEB_CODEX_MAPS_NODE_ENV",
                "source": "dependency_root",
                "dependency": "style-spec",
            },
        )
        self.assertEqual(bindings["HTTP_PROXY"]["source"], "host")
        self.assertEqual(server["startup_timeout_sec"], 60)
        self.assertEqual(server["tool_timeout_sec"], 90)
        lock = (root / "requirements.lock").read_text()
        self.assertIn("--hash=sha256:", lock)
        self.assertIn("setuptools==", lock)
        self.assertNotIn(" -e ", lock)
        self.assertNotIn("file:", lock)


if __name__ == "__main__":
    unittest.main()
