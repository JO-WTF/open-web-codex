"""Run initialize, Tool calls, Resource read, and a typed error over real stdio."""

from __future__ import annotations

import asyncio
import json
import os
import sys
import tempfile
from pathlib import Path

from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client
from pydantic import AnyUrl

SANDBOX_META = "codex/sandbox-state-meta"


async def smoke() -> None:
    with tempfile.TemporaryDirectory(prefix="record-review-provider-smoke-") as directory:
        root = Path(directory)
        workspace = root / "workspace"
        profile = root / "profile"
        workspace.mkdir()
        profile.mkdir()
        environment = dict(os.environ)
        environment["CODEX_HOME"] = str(profile)
        parameters = StdioServerParameters(
            command=sys.executable,
            args=["-m", "record_review_provider.server"],
            cwd=str(workspace),
            env=environment,
        )
        meta = {SANDBOX_META: {"sandboxCwd": workspace.as_uri()}}

        async with stdio_client(parameters) as streams:
            async with ClientSession(*streams) as session:
                await asyncio.wait_for(session.initialize(), timeout=15)
                tools = await asyncio.wait_for(session.list_tools(), timeout=15)
                assert {tool.name for tool in tools.tools} == {
                    "publish_review",
                    "load_review",
                }

                published = await asyncio.wait_for(
                    session.call_tool(
                        "publish_review",
                        {"record_id": "sample", "score": 92, "threshold": 80},
                        meta=meta,
                    ),
                    timeout=15,
                )
                assert published.isError is not True, published.content
                resource_ref = published.structuredContent["resource_ref"]
                assert published.structuredContent["workspace_file"]["relative_path"] == (
                    "outputs/record-review/sample.json"
                )

                resource = await asyncio.wait_for(
                    session.read_resource(AnyUrl(resource_ref["uri"])), timeout=15
                )
                assert len(resource.contents) == 1
                resource_payload = json.loads(resource.contents[0].text)
                assert resource_payload["record_id"] == "sample"
                assert resource_payload["status"] == "accepted"

                loaded = await asyncio.wait_for(
                    session.call_tool("load_review", {"resource_ref": resource_ref}),
                    timeout=15,
                )
                assert loaded.isError is not True, loaded.content
                assert loaded.structuredContent["review"] == resource_payload

                duplicate = await asyncio.wait_for(
                    session.call_tool(
                        "publish_review",
                        {"record_id": "sample", "score": 63, "threshold": 80},
                        meta=meta,
                    ),
                    timeout=15,
                )
                assert duplicate.isError is True
                expected_error = {
                    "schemaVersion": "record_review_error.v1",
                    "status": "error",
                    "code": "workspace_file_invalid",
                    "retryable": False,
                }
                assert duplicate.structuredContent == expected_error
                assert len(duplicate.content) == 1
                assert duplicate.content[0].text.startswith("{"), duplicate.model_dump()
                assert json.loads(duplicate.content[0].text) == expected_error


if __name__ == "__main__":
    asyncio.run(smoke())
    print("Provider SDK record-review stdio smoke passed")
