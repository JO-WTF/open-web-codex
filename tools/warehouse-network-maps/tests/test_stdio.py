from __future__ import annotations

import asyncio
import os
import sys
import tempfile

from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client
from open_web_codex_provider import SANDBOX_STATE_META_CAPABILITY


def test_maps_stdio_initialize_advertises_the_runtime_workspace_capability() -> None:
    async def initialize() -> None:
        with tempfile.TemporaryDirectory(prefix="maps-mcp-stdio-") as directory:
            parameters = StdioServerParameters(
                command=sys.executable,
                args=["-m", "maps_mcp.server"],
                cwd=directory,
                env=dict(os.environ),
            )
            async with stdio_client(parameters) as streams:
                async with ClientSession(*streams) as session:
                    result = await asyncio.wait_for(session.initialize(), timeout=10)
                    assert result.capabilities.experimental == {
                        SANDBOX_STATE_META_CAPABILITY: {}
                    }

    asyncio.run(initialize())
