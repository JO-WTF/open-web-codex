"""Bounded stdio checks for the current Workspace-wide intake contract.

This smoke intentionally does not invoke the retired tutorial servers.  It
proves source discovery/profile/mapping handoff and proves that analysis tools
are rejected when the Platform execution gate is absent.
"""

from __future__ import annotations

import asyncio
import json
import os
import tempfile
from pathlib import Path

import pytest
from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client
from pydantic import AnyUrl

ROOT = Path(__file__).resolve().parents[1]
LAUNCHER = ROOT / "bin" / "supply-chain-planner-launcher"
SANDBOX_META = "codex/sandbox-state-meta"


def server_environment(state_root: Path) -> dict[str, str]:
    environment = dict(os.environ)
    environment.update(
        {
            "CODEX_HOME": str(state_root / "codex-home"),
            "OPEN_WEB_CODEX_LOG_DIR": str(state_root / "logs"),
            "SUPPLY_CHAIN_DATA_RESOURCE_DIR": str(state_root / "data-resources"),
            "SUPPLY_CHAIN_RESOURCE_DIR": str(state_root / "planning-resources"),
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
        demo_workspace = state_root / "demo-workspace"
        demo_workspace.mkdir()
        environment = server_environment(state_root)
        demo_parameters = StdioServerParameters(
            command=str(LAUNCHER),
            args=["--demo-server", "--workspace-root", str(ROOT)],
            cwd=str(ROOT),
            env=environment,
        )
        async with stdio_client(demo_parameters) as streams:
            async with ClientSession(*streams) as session:
                initialized = await asyncio.wait_for(session.initialize(), timeout=10)
                assert SANDBOX_META in (initialized.capabilities.experimental or {})
                tools = await asyncio.wait_for(session.list_tools(), timeout=10)
                assert [tool.name for tool in tools.tools] == ["create_demo_workspace_sources"]
                missing_meta = await asyncio.wait_for(
                    session.call_tool("create_demo_workspace_sources", {}), timeout=10
                )
                assert missing_meta.isError is True
                meta = workspace_meta(demo_workspace)
                created = await asyncio.wait_for(
                    session.call_tool("create_demo_workspace_sources", {}, meta=meta), timeout=10
                )
                reused = await asyncio.wait_for(
                    session.call_tool("create_demo_workspace_sources", {}, meta=meta), timeout=10
                )
                assert created.structuredContent["status"] == "created"
                assert reused.structuredContent["status"] == "reused"
                assert created.structuredContent["dataClassification"] == "synthetic_demo"
        (workspace / "network.csv").write_text(
            "demand_location_id,name,region,latitude,longitude\nd-1,Jakarta,Jakarta,-6.2,106.8\n",
            encoding="utf-8",
        )
        data_parameters = StdioServerParameters(
            command=str(LAUNCHER),
            args=["--data-server", "--workspace-root", str(workspace)],
            cwd=str(ROOT),
            env=environment,
        )
        async with stdio_client(data_parameters) as streams:
            async with ClientSession(*streams) as session:
                await asyncio.wait_for(session.initialize(), timeout=10)
                tools = await asyncio.wait_for(session.list_tools(), timeout=10)
                names = {tool.name for tool in tools.tools}
                assert {
                    "discover_workspace_sources",
                    "inspect_workspace_sources",
                    "publish_source_profile",
                    "publish_mapping_proposal",
                    "normalize_planning_dataset",
                    "validate_planning_dataset",
                } <= names
                mapping_tool = next(
                    tool for tool in tools.tools if tool.name == "publish_mapping_proposal"
                )
                assert "source_profile_ref" in mapping_tool.inputSchema["properties"]
                assert "source_profile" not in mapping_tool.inputSchema["properties"]
                meta = workspace_meta(workspace)
                discovered = await asyncio.wait_for(
                    session.call_tool("discover_workspace_sources", {}, meta=meta), timeout=10
                )
                assert discovered.isError is not True
                source = discovered.structuredContent["sources"][0]
                source_ref = source["source_ref"]
                profiled = await asyncio.wait_for(
                    session.call_tool(
                        "publish_source_profile", {"source_refs": [source_ref]}, meta=meta
                    ),
                    timeout=10,
                )
                assert profiled.isError is not True
                profile_ref = profiled.structuredContent["data_ref"]
                requirement_profile = {
                    "schemaVersion": "data_requirement_profile.v1",
                    "entities": [
                        {
                            "name": "CityDemand",
                            "requiredFields": [
                                {"name": "name"},
                                {"name": "latitude"},
                            ],
                        }
                    ],
                }
                mapping = await asyncio.wait_for(
                    session.call_tool(
                        "publish_mapping_proposal",
                        {
                            "source_profile_ref": profile_ref,
                            "requirement_profile": requirement_profile,
                        },
                    ),
                    timeout=10,
                )
                assert mapping.isError is not True
                assert mapping.structuredContent["data_ref"]["resource_schema"] == (
                    "mapping_proposal.v1"
                )
                proposal = await read_resource(
                    session, mapping.structuredContent["data_ref"]
                )
                assert proposal["candidates"]
        planning_parameters = StdioServerParameters(
            command=str(LAUNCHER),
            args=["--workspace-root", str(workspace)],
            cwd=str(ROOT),
            env=environment,
        )
        async with stdio_client(planning_parameters) as streams:
            async with ClientSession(*streams) as session:
                await asyncio.wait_for(session.initialize(), timeout=10)
                blocked = await asyncio.wait_for(
                    session.call_tool(
                        "prepare_network_snapshot_from_planning_dataset",
                        {
                            "planning_dataset_ref": {
                                "server": "supply_chain_data",
                                "uri": "supply-chain-data://resources/planning-dataset.v2-test",
                                "resource_schema": "planning-dataset.v2",
                            }
                        },
                    ),
                    timeout=10,
                )
                assert blocked.isError is True
                assert "analysis_authorization_required" in "\n".join(
                    item.text for item in blocked.content if getattr(item, "type", None) == "text"
                )


def test_stdio_smoke() -> None:
    if os.environ.get("RUN_REAL_STDIO_SMOKE") != "1":
        pytest.skip("set RUN_REAL_STDIO_SMOKE=1 to launch both current MCP stdio servers")
    asyncio.run(asyncio.wait_for(smoke(), timeout=60))


if __name__ == "__main__":
    asyncio.run(asyncio.wait_for(smoke(), timeout=60))
