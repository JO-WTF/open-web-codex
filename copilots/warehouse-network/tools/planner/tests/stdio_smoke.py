"""Bounded stdio checks for the current Workspace-wide intake contract.

This smoke proves native-cwd source discovery and one typed source Profile.
It inventories Network tools but never invokes them in the Data-only sample.
"""

from __future__ import annotations

import asyncio
import json
import os
import sys
import tempfile
from pathlib import Path

import pytest
from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client
from pydantic import AnyUrl

ROOT = Path(__file__).resolve().parents[1]
SANDBOX_META = "codex/sandbox-state-meta"


def server_environment(state_root: Path) -> dict[str, str]:
    environment = dict(os.environ)
    repository_data_dir = state_root / "runtime"
    environment.update(
        {
            "CODEX_HOME": str(state_root / "codex-home"),
            "OPEN_WEB_CODEX_DATA_DIR": str(repository_data_dir),
            # Force analysis MCP calls to fail closed in this isolated smoke.
            "OPEN_WEB_CODEX_ANALYSIS_GATE_URL": "",
            "OPEN_WEB_CODEX_ANALYSIS_GATE_KEY": "",
        }
    )
    return environment


def workspace_meta(root: Path) -> dict[str, object]:
    return {SANDBOX_META: {"sandboxCwd": root.as_uri()}}


async def read_resource(session: ClientSession, ref: dict[str, object]) -> dict[str, object]:
    result = await session.read_resource(AnyUrl(str(ref["uri"])))
    assert len(result.contents) == 1
    return json.loads(result.contents[0].text)


async def smoke() -> None:
    with tempfile.TemporaryDirectory(prefix="supply-chain-mcp-smoke-") as directory:
        state_root = Path(directory)
        workspace = state_root / "workspace"
        workspace.mkdir()
        environment = server_environment(state_root)
        (workspace / "network.csv").write_text(
            "city_id,city_name,demand_quantity,latitude,longitude\ncity-1,Jakarta,10,-6.2,106.8\n",
            encoding="utf-8",
        )
        data_parameters = StdioServerParameters(
            command=sys.executable,
            args=["-m", "supply_chain_planner.data.server"],
            cwd=str(workspace),
            env=environment,
        )
        async with stdio_client(data_parameters) as streams:
            async with ClientSession(*streams) as session:
                await asyncio.wait_for(session.initialize(), timeout=10)
                tools = await asyncio.wait_for(session.list_tools(), timeout=10)
                names = {tool.name for tool in tools.tools}
                expected_tools = {
                    "discover_workspace_sources",
                    "inspect_workspace_sources",
                    "normalize_network_input",
                    "prepare_network_geography",
                }
                assert names == expected_tools
                meta = workspace_meta(workspace)
                discovered = await asyncio.wait_for(
                    session.call_tool("discover_workspace_sources", {}, meta=meta), timeout=10
                )
                assert discovered.isError is not True
                source = discovered.structuredContent["sources"][0]
                relative_path = source["relative_path"]
                profiled = await asyncio.wait_for(
                    session.call_tool(
                        "inspect_workspace_sources",
                        {"relative_paths": [relative_path]},
                        meta=meta,
                    ),
                    timeout=10,
                )
                assert profiled.isError is not True
                profile_ref = profiled.structuredContent["resource_ref"]
                assert profile_ref["server"] == "supply_chain_data"
                assert profile_ref["resource_schema"] == "source_profile.v1"
                profile = await read_resource(session, profile_ref)
                assert profile["sources"][0]["relative_path"] == "network.csv"
                assert profile["sources"][0]["mapping_suggestions"]
        planning_parameters = StdioServerParameters(
            command=sys.executable,
            args=["-m", "supply_chain_planner.network.server"],
            cwd=str(workspace),
            env=environment,
        )
        async with stdio_client(planning_parameters) as streams:
            async with ClientSession(*streams) as session:
                await asyncio.wait_for(session.initialize(), timeout=10)
                names = {tool.name for tool in (await session.list_tools()).tools}
                expected_tools = {
                    "plan_route_matrix",
                    "build_haversine_route_matrix",
                    "build_provided_route_matrix",
                    "register_navigation_route_matrix",
                    "validate_route_matrix",
                    "plan_cost_matrix",
                    "prepare_network_comparison_map",
                    "prepare_network_distribution_map",
                    "evaluate_network_baseline",
                    "assess_facility_change",
                    "evaluate_facility_scenario",
                    "solve_p_median",
                    "compare_network_scenarios",
                    "render_network_comparison_map",
                    "publish_network_planning_report",
                }
                assert expected_tools <= names, sorted(expected_tools - names)


def test_stdio_smoke() -> None:
    if os.environ.get("RUN_REAL_STDIO_SMOKE") != "1":
        pytest.skip("set RUN_REAL_STDIO_SMOKE=1 to launch both current MCP stdio servers")
    asyncio.run(asyncio.wait_for(smoke(), timeout=60))


if __name__ == "__main__":
    asyncio.run(asyncio.wait_for(smoke(), timeout=60))
