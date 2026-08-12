from __future__ import annotations

import asyncio
import importlib.util
import json
import tomllib
from pathlib import Path

from supply_chain_planner import __version__
from supply_chain_planner import server as network_server
from supply_chain_planner.data_server import mcp as data_mcp
from supply_chain_planner.server import mcp as network_mcp

REMOVED_CASE_ENTRYPOINTS = (
    "create_network_case",
    "get_network_case_status",
    "archive_network_case",
    "refresh_case_sources",
    "inspect_case_sources",
    "propose_case_mapping",
    "apply_case_mapping",
    "normalize_case_input",
    "define_network_requirements",
)
REMOVED_CASE_LEGACY_ENTRYPOINTS = (
    "_legacy_plan_route_matrix",
    "_legacy_build_haversine_route_matrix",
    "_legacy_validate_route_matrix",
    "_legacy_register_navigation_route_matrix",
    "_legacy_plan_cost_matrix",
    "_legacy_evaluate_network_baseline",
    "_legacy_evaluate_facility_scenario",
    "_legacy_solve_p_median",
    "_legacy_solve_service_constrained_location",
    "_legacy_render_network_comparison_map",
    "_legacy_publish_network_planning_report",
)
REMOVED_MODULES = (
    "analysis_service", "case_models", "case_repository", "case_tools", "case_types",
    "core", "data_core", "decision_core", "demo_server", "indonesia_analysis",
    "indonesia_models", "indonesia_report", "indonesia_server", "mapping_service",
    "matrix_service", "network_data", "readiness", "requirements", "scenario_service",
    "workspace_dataset",
)
REMOVED_CONTRACT_FILES = (
    "supply_chain_planner/case_schema.sql",
    "contracts/warehouse-network-planning-1.0.0.json",
    "contracts/indonesia-warehouse-network-2.0.0.json",
)
DATA_TOOLS = {
    "discover_workspace_sources",
    "inspect_workspace_sources",
    "normalize_network_input",
    "prepare_network_geography",
}
NETWORK_TOOLS = {
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


def test_plugin_manifest_and_mcp_config_are_wired() -> None:
    root = Path(__file__).resolve().parents[1]
    manifest = json.loads((root / ".codex-plugin" / "plugin.json").read_text())
    mcp_config = json.loads((root / ".mcp.json").read_text())
    project = tomllib.loads((root / "pyproject.toml").read_text())

    assert manifest["name"] == "supply-chain-network-planner"
    assert manifest["version"] == project["project"]["version"] == __version__ == "0.4.0"
    assert "skills" not in manifest
    assert "network Resources" in manifest["interface"]["longDescription"]
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
    assert names == NETWORK_TOOLS
    for entrypoint in REMOVED_CASE_ENTRYPOINTS:
        assert not hasattr(network_server, entrypoint)
    for entrypoint in REMOVED_CASE_LEGACY_ENTRYPOINTS:
        assert not hasattr(network_server, entrypoint)
    for module_name in REMOVED_MODULES:
        assert importlib.util.find_spec(f"supply_chain_planner.{module_name}") is None


def test_data_server_exposes_only_data_tools() -> None:
    names = {tool.name for tool in asyncio.run(data_mcp.list_tools())}
    assert names == DATA_TOOLS


def test_legacy_skill_surface_is_removed() -> None:
    root = Path(__file__).resolve().parents[1]
    assert not (root / "skills").exists()


def test_legacy_case_contract_surface_is_removed() -> None:
    root = Path(__file__).resolve().parents[1]
    for relative_path in REMOVED_CONTRACT_FILES:
        assert not (root / relative_path).exists()
