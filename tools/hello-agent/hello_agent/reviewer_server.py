"""MCP boundary for independent greeting review."""

from __future__ import annotations

from mcp.server.fastmcp import FastMCP

from .core import Greeting, GreetingReview, evaluate_greeting

mcp = FastMCP(
    "Hello Reviewer",
    instructions=(
        "Call review_greeting with the Writer's unchanged structured result. "
        "Return approval and reasons. Never rewrite a rejected greeting."
    ),
    json_response=True,
)


@mcp.tool()
def review_greeting(greeting: Greeting) -> GreetingReview:
    """Approve or reject one greeting without modifying it."""

    return evaluate_greeting(greeting)


def main() -> None:
    """Run the Reviewer MCP over standard input and output."""

    mcp.run(transport="stdio")


if __name__ == "__main__":
    main()
