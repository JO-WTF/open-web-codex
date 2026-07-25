from __future__ import annotations

import json
from pathlib import Path
import unittest


class PluginConfigTests(unittest.TestCase):
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
