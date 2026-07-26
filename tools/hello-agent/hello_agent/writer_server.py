"""MCP boundary for deterministic greeting generation."""

from __future__ import annotations

from mcp.server.fastmcp import FastMCP

from .core import Greeting, build_greeting

mcp = FastMCP(
    "Hello Writer",
    instructions=(
        "Call say_hello only when a user provides one person to greet. "
        "Return the structured Tool result unchanged. Do not invent a successful "
        "result when the Tool rejects the input or is unavailable."
    ),
    json_response=True,
)


@mcp.tool()
def say_hello(name: str) -> Greeting:
    """Return the canonical structured greeting for one named person."""

    return build_greeting(name)


def main() -> None:
    """Run the Writer MCP over standard input and output."""

    mcp.run(transport="stdio")


if __name__ == "__main__":
    main()
