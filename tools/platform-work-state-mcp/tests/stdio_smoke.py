"""Verify the Domain Agent Work State MCP exposes only typed mutations."""

from __future__ import annotations

import asyncio
import os
from pathlib import Path

from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client


ROOT = Path(__file__).resolve().parents[1]


async def smoke() -> None:
    environment = dict(os.environ)
    repository_data_dir = ROOT.parent.parent / ".local" / "open-web-codex"
    environment.update(
        {
            "OPEN_WEB_CODEX_DATA_DIR": str(repository_data_dir),
            "OPEN_WEB_CODEX_WORK_STATE_MCP_VENV": str(
                repository_data_dir / "tool-envs" / "platform-work-state"
            ),
        }
    )
    parameters = StdioServerParameters(
        command=str(ROOT / "bin" / "platform-work-state-launcher"),
        args=[],
        cwd=str(ROOT),
        env=environment,
    )
    async with stdio_client(parameters) as streams:
        async with ClientSession(*streams) as session:
            await asyncio.wait_for(session.initialize(), timeout=10)
            tools = await asyncio.wait_for(session.list_tools(), timeout=10)
            assert {tool.name for tool in tools.tools} == {
                "begin_work_operation",
                "apply_work_state_mutation",
                "fail_work_operation",
                "get_work_state_context",
            }


if __name__ == "__main__":
    asyncio.run(smoke())
