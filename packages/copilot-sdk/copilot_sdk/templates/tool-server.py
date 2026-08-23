"""Expose the domain-neutral typed Tool through FastMCP."""

from mcp.server.fastmcp import FastMCP
from mcp.types import ToolAnnotations

from .core import AnalysisResult
from .core import analyze_record as analyze_record_value

mcp = FastMCP("__TOOL_ID__")
READ_ONLY = ToolAnnotations(
    readOnlyHint=True,
    destructiveHint=False,
    idempotentHint=True,
    openWorldHint=False,
)


@mcp.tool(structured_output=True, annotations=READ_ONLY)
def analyze_record(record_id: str, value: int) -> AnalysisResult:
    """Validate one neutral record and return a deterministic typed result."""

    return analyze_record_value(record_id, value)


if __name__ == "__main__":
    mcp.run()
