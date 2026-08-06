from __future__ import annotations

import asyncio
import json
import tomllib
from pathlib import Path

from supply_chain_planner import __version__
from supply_chain_planner.data_server import mcp as data_mcp


def test_plugin_manifest_and_mcp_config_are_wired() -> None:
    root = Path(__file__).resolve().parents[1]
    manifest = json.loads((root / ".codex-plugin" / "plugin.json").read_text())
    mcp_config = json.loads((root / ".mcp.json").read_text())
    project = tomllib.loads((root / "pyproject.toml").read_text())

    assert manifest["name"] == "supply-chain-network-planner"
    assert manifest["version"] == project["project"]["version"] == __version__ == "0.4.0"
    assert manifest["skills"] == "./skills/"
    assert manifest["mcpServers"] == "./.mcp.json"
    data_server = mcp_config["mcpServers"]["supply_chain_data"]
    assert data_server["command"] == "./bin/supply-chain-planner-launcher"
    assert data_server["args"][0] == "--data-server"
    assert data_server["cwd"] == "."
    assert data_server["default_tools_approval_mode"] == "approve"
    demo_server = mcp_config["mcpServers"]["supply_chain_demo"]
    assert demo_server["command"] == "./bin/supply-chain-planner-launcher"
    assert demo_server["args"] == ["--demo-server", "--workspace-root", "."]
    assert demo_server["cwd"] == "."
    assert demo_server["default_tools_approval_mode"] == "approve"
    server = mcp_config["mcpServers"]["supply_chain_planner"]
    assert server["command"] == "./bin/supply-chain-planner-launcher"
    assert server["cwd"] == "."
    assert server["default_tools_approval_mode"] == "approve"
    assert "tools" not in data_server
    assert "tools" not in demo_server
    assert "tools" not in server


def test_data_server_inventory_has_no_static_source_tools() -> None:
    names = {tool.name for tool in asyncio.run(data_mcp.list_tools())}
    assert names == {
        "discover_workspace_sources",
        "inspect_workspace_sources",
        "publish_source_profile",
        "publish_mapping_proposal",
        "normalize_planning_dataset",
        "validate_planning_dataset",
    }
    assert names.isdisjoint(
        {"list_planning_sources", "inspect_planning_source", "build_planning_dataset"}
    )


def test_demo_skill_has_explicit_trigger_and_no_embedded_template() -> None:
    root = Path(__file__).resolve().parents[1]
    skill = (root / "skills" / "create-demo-workspace-data" / "SKILL.md").read_text()
    assert "Require an explicit user request" in skill
    assert "Never trigger because a Workspace is empty" in skill
    assert "create_demo_workspace_sources" in skill
    assert "demand-locations.csv" not in skill
