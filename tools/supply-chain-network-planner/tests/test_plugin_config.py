from __future__ import annotations

import asyncio
import json
import tomllib
from pathlib import Path

from supply_chain_planner import __version__
from supply_chain_planner.server import mcp as network_mcp


def test_plugin_manifest_and_mcp_config_are_wired() -> None:
    root = Path(__file__).resolve().parents[1]
    manifest = json.loads((root / ".codex-plugin" / "plugin.json").read_text())
    mcp_config = json.loads((root / ".mcp.json").read_text())
    project = tomllib.loads((root / "pyproject.toml").read_text())

    assert manifest["name"] == "supply-chain-network-planner"
    assert manifest["version"] == project["project"]["version"] == __version__ == "0.4.0"
    assert manifest["skills"] == "./skills/"
    assert manifest["mcpServers"] == "./.mcp.json"
    assert set(mcp_config["mcpServers"]) == {"supply_chain"}
    server = mcp_config["mcpServers"]["supply_chain"]
    assert server["command"] == "./bin/supply-chain-planner-launcher"
    assert "cwd" not in server
    assert server["default_tools_approval_mode"] == "prompt"
    assert server["tools"] == {
        "plan_route_matrix": {"approval_mode": "approve"},
        "build_haversine_route_matrix": {"approval_mode": "approve"},
        "build_provided_route_matrix": {"approval_mode": "approve"},
        "validate_route_matrix": {"approval_mode": "approve"},
        "register_navigation_route_matrix": {"approval_mode": "approve"},
        "plan_cost_matrix": {"approval_mode": "approve"},
        "prepare_network_distribution_map": {"approval_mode": "approve"},
        "prepare_network_comparison_map": {"approval_mode": "approve"},
        "evaluate_network_baseline": {"approval_mode": "approve"},
        "evaluate_facility_scenario": {"approval_mode": "approve"},
        "solve_p_median": {"approval_mode": "approve"},
        "compare_network_scenarios": {"approval_mode": "approve"},
    }


def test_network_server_exposes_only_network_tools() -> None:
    names = {tool.name for tool in asyncio.run(network_mcp.list_tools())}
    assert names == {
        "build_haversine_route_matrix",
        "build_provided_route_matrix",
        "compare_network_scenarios",
        "evaluate_facility_scenario",
        "evaluate_network_baseline",
        "plan_route_matrix",
        "plan_cost_matrix",
        "prepare_network_comparison_map",
        "prepare_network_distribution_map",
        "publish_network_planning_report",
        "register_navigation_route_matrix",
        "render_network_comparison_map",
        "solve_p_median",
        "validate_route_matrix",
    }


def test_demo_skill_has_explicit_trigger_and_no_embedded_template() -> None:
    root = Path(__file__).resolve().parents[1]
    skill = (root / "skills" / "create-demo-workspace-data" / "SKILL.md").read_text()
    assert "用户明确要求" in skill
    assert "Workspace 为空" in skill
    assert "create_demo_workspace_sources" not in skill
    assert "demand-locations.csv" not in skill
