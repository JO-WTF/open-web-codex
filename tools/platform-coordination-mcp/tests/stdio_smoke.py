"""Verify the read-only coordination MCP starts through its checked-in launcher."""

from __future__ import annotations

import asyncio
import os
from pathlib import Path

from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client


ROOT = Path(__file__).resolve().parents[1]
EXPECTED_TOOLS = {
    "get_collaboration_status",
    "list_agent_executions",
    "get_work_state_summary",
    "list_blocking_inputs",
    "list_deliverables",
}


async def smoke() -> None:
    repository_data_dir = ROOT.parent.parent / ".local" / "open-web-codex"
    environment = dict(os.environ)
    environment.update(
        {
            "OPEN_WEB_CODEX_DATA_DIR": str(repository_data_dir),
            "OPEN_WEB_CODEX_COORDINATION_MCP_VENV": str(
                repository_data_dir / "tool-envs" / "platform-coordination"
            ),
            "OPEN_WEB_CODEX_LOG_DIR": str(repository_data_dir / "logs"),
        }
    )
    parameters = StdioServerParameters(
        command=str(ROOT / "bin" / "platform-coordination-launcher"),
        args=[],
        cwd=str(ROOT),
        env=environment,
    )
    async with stdio_client(parameters) as streams:
        async with ClientSession(*streams) as session:
            await asyncio.wait_for(session.initialize(), timeout=10)
            tools = await asyncio.wait_for(session.list_tools(), timeout=10)
            assert {tool.name for tool in tools.tools} == EXPECTED_TOOLS


if __name__ == "__main__":
    asyncio.run(smoke())
