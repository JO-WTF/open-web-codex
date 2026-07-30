from __future__ import annotations

import json
import tomllib
from pathlib import Path

from supply_chain_planner import __version__


def test_plugin_manifest_and_mcp_config_are_wired() -> None:
    root = Path(__file__).resolve().parents[1]
    manifest = json.loads((root / ".codex-plugin" / "plugin.json").read_text())
    mcp_config = json.loads((root / ".mcp.json").read_text())
    project = tomllib.loads((root / "pyproject.toml").read_text())

    assert manifest["name"] == "supply-chain-network-planner"
    assert manifest["version"] == project["project"]["version"] == __version__ == "0.3.0"
    assert manifest["skills"] == "./skills/"
    assert manifest["mcpServers"] == "./.mcp.json"
    data_server = mcp_config["mcpServers"]["supply_chain_data"]
    assert data_server["command"] == "./bin/supply-chain-planner-launcher"
    assert data_server["args"][0] == "--data-server"
    assert data_server["cwd"] == "."
    assert data_server["default_tools_approval_mode"] == "approve"
    server = mcp_config["mcpServers"]["supply_chain_planner"]
    assert server["command"] == "./bin/supply-chain-planner-launcher"
    assert server["cwd"] == "."
    assert server["default_tools_approval_mode"] == "approve"
    assert "tools" not in data_server
    assert "tools" not in server
