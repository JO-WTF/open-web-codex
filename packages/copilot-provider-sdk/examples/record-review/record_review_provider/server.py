"""Runnable FastMCP server for the Provider SDK cookbook."""

from __future__ import annotations

import os
from pathlib import Path

from mcp.server.fastmcp import Context, FastMCP
from mcp.types import CallToolResult, ToolAnnotations
from open_web_codex_provider import McpResourceRuntime, ResourceRef

from .service import load_review as load_review_value
from .service import publish_review as publish_review_value

SERVER_NAME = "record_review"
URI_PREFIX = "record-review://resources/"

mcp = FastMCP(SERVER_NAME)
CREATE_NEW = ToolAnnotations(
    readOnlyHint=False,
    destructiveHint=False,
    idempotentHint=False,
    openWorldHint=False,
)
READ_ONLY = ToolAnnotations(
    readOnlyHint=True,
    destructiveHint=False,
    idempotentHint=True,
    openWorldHint=False,
)

_runtime = McpResourceRuntime(
    Path.cwd(),
    Path(os.environ.get("CODEX_HOME", Path.cwd() / ".codex")),
    SERVER_NAME,
    URI_PREFIX,
)


def configure_runtime(workspace: Path, profile_home: Path) -> None:
    """Replace process-owned state for isolated tests before serving requests."""

    global _runtime
    _runtime = McpResourceRuntime(workspace, profile_home, SERVER_NAME, URI_PREFIX)


@mcp.resource(
    "record-review://resources/{resource_id}",
    name="record_review_resource",
    title="Record review resource",
    mime_type="application/json",
)
def read_record_review_resource(resource_id: str) -> str:
    """Read one immutable Resource produced by this server."""

    return _runtime.read(resource_id)


@mcp.tool(structured_output=True, annotations=CREATE_NEW)
def publish_review(
    record_id: str,
    score: int,
    ctx: Context,
    threshold: int = 80,
) -> CallToolResult:
    """Review one record, create a new Workspace file, and publish a Resource."""

    return publish_review_value(_runtime, ctx, record_id, score, threshold)


@mcp.tool(structured_output=True, annotations=READ_ONLY)
def load_review(
    resource_ref: ResourceRef,
) -> CallToolResult:
    """Load one exact Resource reference and return its typed review."""

    return load_review_value(_runtime, resource_ref)


if __name__ == "__main__":
    mcp.run()
