"""Implement the typed tools for __DISPLAY_NAME__."""

from mcp.server.fastmcp import FastMCP


mcp = FastMCP("__TOOL_ID__")


@mcp.tool()
def health() -> dict[str, str]:
    """Return a deterministic local discovery result."""

    return {"status": "ok", "tool": "__TOOL_ID__.health"}


if __name__ == "__main__":
    mcp.run()
