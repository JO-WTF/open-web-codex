from __future__ import annotations

import asyncio
import json
import unittest
from pathlib import Path

import maps_mcp.server as maps_server


class PluginConfigTests(unittest.TestCase):
    def test_maps_mcp_preapproves_only_local_map_card(self) -> None:
        config_path = Path(__file__).parents[1] / ".mcp.json"
        config = json.loads(config_path.read_text())
        server = config["mcpServers"]["map_utils"]

        self.assertEqual(server["default_tools_approval_mode"], "prompt")
        self.assertEqual(
            server["tools"],
            {"create_map_card": {"approval_mode": "approve"}},
        )

    def test_maps_mcp_annotations_separate_local_and_external_tools(self) -> None:
        tools = {tool.name: tool for tool in asyncio.run(maps_server.mcp.list_tools())}
        local = tools["create_map_card"].annotations
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

    def test_maps_mcp_forwards_standard_proxy_environment_variables(self) -> None:
        config_path = Path(__file__).parents[1] / ".mcp.json"
        config = json.loads(config_path.read_text())
        env_vars = config["mcpServers"]["map_utils"]["env_vars"]

        self.assertTrue(
            {
                "HTTP_PROXY",
                "HTTPS_PROXY",
                "ALL_PROXY",
                "NO_PROXY",
                "http_proxy",
                "https_proxy",
                "all_proxy",
                "no_proxy",
            }.issubset(env_vars)
        )


if __name__ == "__main__":
    unittest.main()
