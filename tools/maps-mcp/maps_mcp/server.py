"""FastMCP entry point for Google Maps and Mapbox tools."""

from __future__ import annotations

import argparse
from pathlib import Path
from typing import Literal

from mcp.server.fastmcp import Context
from mcp.server.fastmcp import FastMCP
from mcp.server.session import ServerSession
from pydantic import BaseModel
from pydantic import Field

from .clients import GoogleMapsClient
from .clients import MapboxMapsClient
from .credential_prompt import LoopbackCredentialPrompt
from .credentials import WorkspaceCredentialStore

Provider = Literal["google", "mapbox"]
TravelMode = Literal[
    "driving", "driving_traffic", "walking", "bicycling", "transit", "two_wheeler"
]


class Point(BaseModel):
    latitude: float = Field(ge=-90, le=90)
    longitude: float = Field(ge=-180, le=180)


mcp = FastMCP(
    "Workspace Maps",
    instructions=(
        "Paid geocoding and routing tools for Google Maps and Mapbox. "
        "API keys are requested through elicitation and remembered per workspace."
    ),
    json_response=True,
)
_credential_store = WorkspaceCredentialStore(Path.cwd())


async def _client(provider: Provider, ctx: Context[ServerSession, None]):
    api_key = _credential_store.get_api_key(provider)
    if api_key is None:
        prompt = LoopbackCredentialPrompt(provider)
        prompt.start()
        try:
            result = await ctx.elicit_url(
                message=(
                    f"No {provider} maps API key is stored for this workspace. "
                    "Open the local one-time page to provide it securely."
                ),
                url=prompt.url,
                elicitation_id=prompt.elicitation_id,
            )
            action = getattr(result.action, "value", result.action)
            if action != "accept":
                raise RuntimeError(f"{provider} API key request was declined or cancelled")
            submission = await prompt.wait()
            api_key = submission.api_key
            if submission.remember:
                _credential_store.set_api_key(provider, api_key)
                await ctx.info(f"Stored {provider} API key in workspace credential memory")
            await ctx.session.send_elicit_complete(prompt.elicitation_id)
        finally:
            await prompt.close()
    if provider == "google":
        return GoogleMapsClient(api_key)
    return MapboxMapsClient(api_key)


def _point(point: Point) -> dict[str, float]:
    return point.model_dump()


@mcp.tool()
async def batch_geocode(
    provider: Provider,
    addresses: list[str],
    ctx: Context[ServerSession, None],
    language: str | None = None,
    region: str | None = None,
) -> dict[str, object]:
    """Batch-convert addresses to coordinates with Google Maps or Mapbox.

    Each input address is billable according to the selected provider. Results preserve input order.
    At most 500 addresses are accepted per call; provider-specific request limits are chunked
    automatically.
    """
    client = await _client(provider, ctx)
    return await client.batch_geocode(addresses, language=language, region=region)


@mcp.tool()
async def batch_reverse_geocode(
    provider: Provider,
    points: list[Point],
    ctx: Context[ServerSession, None],
    language: str | None = None,
    region: str | None = None,
) -> dict[str, object]:
    """Batch-convert WGS84 latitude/longitude points to addresses.

    Each input point is billable according to the selected provider. Results preserve input order.
    At most 500 coordinates are accepted per call.
    """
    client = await _client(provider, ctx)
    return await client.batch_reverse_geocode(
        [_point(point) for point in points], language=language, region=region
    )


@mcp.tool()
async def get_route(
    provider: Provider,
    origin: Point,
    destination: Point,
    ctx: Context[ServerSession, None],
    waypoints: list[Point] | None = None,
    mode: TravelMode = "driving",
    alternatives: bool = False,
    include_steps: bool = False,
    language: str | None = None,
) -> dict[str, object]:
    """Request a navigation route between coordinates using Google Maps or Mapbox.

    Supports optional intermediate waypoints, alternative routes, and turn-by-turn steps. Provider
    travel modes differ: Mapbox supports driving, driving_traffic, walking, and bicycling; Google
    additionally supports transit and two_wheeler.
    """
    client = await _client(provider, ctx)
    return await client.get_route(
        _point(origin),
        _point(destination),
        waypoints=[_point(point) for point in waypoints or []],
        mode=mode,
        alternatives=alternatives,
        include_steps=include_steps,
        language=language,
    )


@mcp.tool()
async def distance_matrix(
    provider: Provider,
    origins: list[Point],
    destinations: list[Point],
    ctx: Context[ServerSession, None],
    mode: TravelMode = "driving",
) -> dict[str, object]:
    """Calculate a batch distance/time matrix with Google Maps or Mapbox.

    Matrix elements are billable as origins multiplied by destinations. Calls are split to respect
    provider request limits. A single tool call is capped at 2,500 elements to bound cost.
    """
    client = await _client(provider, ctx)
    return await client.distance_matrix(
        [_point(point) for point in origins],
        [_point(point) for point in destinations],
        mode=mode,
    )


def main() -> None:
    parser = argparse.ArgumentParser(description="Google Maps and Mapbox MCP server")
    parser.add_argument(
        "--workspace-root",
        type=Path,
        default=Path.cwd(),
        help="Workspace whose .codex directory stores the provider credential memory",
    )
    parser.add_argument(
        "--transport",
        choices=("stdio", "streamable-http"),
        default="stdio",
    )
    args = parser.parse_args()

    global _credential_store
    _credential_store = WorkspaceCredentialStore(args.workspace_root)
    mcp.run(transport=args.transport)


if __name__ == "__main__":
    main()
