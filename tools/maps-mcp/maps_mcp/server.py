"""FastMCP entry point for provider-neutral Google Maps and Mapbox tools."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Annotated
from typing import Literal

from mcp.server.fastmcp import Context
from mcp.server.fastmcp import FastMCP
from mcp.server.session import ServerSession
from mcp.types import CallToolResult
from mcp.types import TextContent
from pydantic import BaseModel
from pydantic import ConfigDict
from pydantic import Field
from pydantic import model_validator

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


class MapCardPoint(Point):
    id: str | None = None
    label: str | None = None
    description: str | None = None
    color: str | None = None


class MapCardLine(BaseModel):
    id: str | None = None
    label: str | None = None
    color: str | None = None
    coordinates: list[Point] = Field(min_length=2, max_length=500)


class MapCardPolygon(BaseModel):
    id: str | None = None
    label: str | None = None
    color: str | None = None
    coordinates: list[list[Point]] = Field(min_length=1, max_length=32)


class MapCardLinePayload(BaseModel):
    """A line in the browser-facing map-card contract."""

    model_config = ConfigDict(extra="forbid")

    id: str | None = None
    label: str | None = None
    color: str | None = None
    coordinates: list[tuple[float, float]] = Field(min_length=2, max_length=500)


class MapCardPolygonPayload(BaseModel):
    """A polygon in the browser-facing map-card contract."""

    model_config = ConfigDict(extra="forbid")

    id: str | None = None
    label: str | None = None
    color: str | None = None
    coordinates: list[list[tuple[float, float]]] = Field(min_length=1, max_length=32)


class MapCardPayload(BaseModel):
    """The bounded inline payload encoded by an ``open-web-card map.v1`` marker."""

    model_config = ConfigDict(extra="forbid")

    title: str = Field(min_length=1)
    intent: str = Field(min_length=1)
    status: Literal["loading", "ready", "error"]
    fallback_text: str | None = None
    summary: str | None = None
    input_ref: str | None = None
    artifact_id: str | None = None
    points: list[MapCardPoint] | None = None
    lines: list[MapCardLinePayload] | None = None
    polygons: list[MapCardPolygonPayload] | None = None
    geojson: dict[str, object] | None = None


mcp = FastMCP(
    "Map Utils",
    instructions=(
        "Map-card output contract: after create_map_card succeeds, copy its returned marker "
        "verbatim into the final assistant answer; do not translate, reformat, or omit it. "
        "Paid geocoding and routing tools for Google Maps and Mapbox. "
        "One selected provider and API key are shared by every maps tool. "
        "Missing configuration is requested through elicitation; a later configuration "
        "replaces the previous provider and key."
    ),
    json_response=True,
)
_credential_store = WorkspaceCredentialStore(Path.cwd())


async def _client(ctx: Context[ServerSession, None]):
    credential = _credential_store.get_credential()
    if credential is None:
        prompt = LoopbackCredentialPrompt()
        prompt.start()
        try:
            result = await ctx.elicit_url(
                message=(
                    "A maps provider and API key are required. Configure Mapbox or Google "
                    "in this app; the selected provider will be saved globally and reused."
                ),
                url=prompt.url,
                elicitation_id=prompt.elicitation_id,
            )
            action = getattr(result.action, "value", result.action)
            if action != "accept":
                raise RuntimeError("Maps provider configuration was declined or cancelled")
            submission = await prompt.wait()
            credential = submission
            if submission.remember:
                _credential_store.set_credential(submission.provider, submission.api_key)
                await ctx.info(
                    f"Stored {submission.provider} as the active maps provider "
                    "in workspace credential memory"
                )
            await ctx.session.send_elicit_complete(prompt.elicitation_id)
        finally:
            await prompt.close()
    if credential.provider == "google":
        return GoogleMapsClient(credential.api_key)
    return MapboxMapsClient(credential.api_key)



def _compact_json(value: dict[str, object]) -> str:
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"))


def _map_card_marker(payload: dict[str, object]) -> str:
    body = _compact_json(payload)
    marker = f"```open-web-card map.v1\n{body}\n```"
    if len(marker.encode("utf-8")) > 16 * 1024:
        raise ValueError("map-card marker is too large; store large GeoJSON as an artifact/input_ref")
    return marker


class MapCardToolResult(BaseModel):
    """Structured MCP result for ``create_map_card``.

    FastMCP exports this model as the tool's ``outputSchema`` and validates the
    ``structuredContent`` field before sending it to the client.
    """

    model_config = ConfigDict(extra="forbid")

    type: Literal["open-web-card"]
    kind: Literal["map.v1"]
    marker: str = Field(min_length=1)
    card: MapCardPayload

    @model_validator(mode="after")
    def marker_must_encode_card(self) -> MapCardToolResult:
        expected = _map_card_marker(
            self.card.model_dump(mode="json", exclude_none=True)
        )
        if self.marker != expected:
            raise ValueError("marker must be the exact fenced encoding of card")
        return self


def _point_payload(point: MapCardPoint | Point) -> dict[str, object]:
    payload: dict[str, object] = {
        "latitude": point.latitude,
        "longitude": point.longitude,
    }
    if isinstance(point, MapCardPoint):
        for key in ("id", "label", "description", "color"):
            value = getattr(point, key)
            if value:
                payload[key] = value
    return payload


@mcp.tool(structured_output=True)
async def create_map_card(
    title: str,
    intent: str = "visualization",
    fallback_text: str | None = None,
    summary: str | None = None,
    input_ref: str | None = None,
    artifact_id: str | None = None,
    points: list[MapCardPoint] | None = None,
    lines: list[MapCardLine] | None = None,
    polygons: list[MapCardPolygon] | None = None,
    geojson: dict[str, object] | None = None,
) -> Annotated[CallToolResult, MapCardToolResult]:
    """Create a compact open-web map-card marker for the final assistant answer.

    Use this after geocoding, route, or distance tools when a map visualization helps.
    Return `marker` verbatim in the final answer where the card should appear. Keep inline
    geometry small; use `input_ref` or `artifact_id` for large GeoJSON.
    """
    clean_title = title.strip()
    if not clean_title:
        raise ValueError("title is required")

    payload: dict[str, object] = {
        "title": clean_title,
        "intent": intent.strip() or "visualization",
        "status": "ready" if any([points, lines, polygons, geojson]) else "loading",
    }
    if fallback_text and fallback_text.strip():
        payload["fallback_text"] = fallback_text.strip()
    if summary and summary.strip():
        payload["summary"] = summary.strip()
    if input_ref and input_ref.strip():
        payload["input_ref"] = input_ref.strip()
    if artifact_id and artifact_id.strip():
        payload["artifact_id"] = artifact_id.strip()
    if points:
        payload["points"] = [_point_payload(point) for point in points]
    if lines:
        payload["lines"] = [
            {
                key: value
                for key, value in {
                    "id": line.id,
                    "label": line.label,
                    "color": line.color,
                    "coordinates": [
                        [point.longitude, point.latitude] for point in line.coordinates
                    ],
                }.items()
                if value is not None
            }
            for line in lines
        ]
    if polygons:
        payload["polygons"] = [
            {
                key: value
                for key, value in {
                    "id": polygon.id,
                    "label": polygon.label,
                    "color": polygon.color,
                    "coordinates": [
                        [[point.longitude, point.latitude] for point in ring]
                        for ring in polygon.coordinates
                    ],
                }.items()
                if value is not None
            }
            for polygon in polygons
        ]
    if geojson:
        payload["geojson"] = geojson

    card = MapCardPayload.model_validate(payload)
    result = MapCardToolResult(
        type="open-web-card",
        kind="map.v1",
        marker=_map_card_marker(card.model_dump(mode="json", exclude_none=True)),
        card=card,
    )
    structured_content = result.model_dump(mode="json", exclude_none=True)
    return CallToolResult(
        content=[TextContent(type="text", text=_compact_json(structured_content))],
        structuredContent=structured_content,
    )


def _point(point: Point) -> dict[str, float]:
    return point.model_dump()


@mcp.tool()
async def batch_geocode(
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
    client = await _client(ctx)
    return await client.batch_geocode(addresses, language=language, region=region)


@mcp.tool()
async def batch_reverse_geocode(
    points: list[Point],
    ctx: Context[ServerSession, None],
    language: str | None = None,
    region: str | None = None,
) -> dict[str, object]:
    """Batch-convert WGS84 latitude/longitude points to addresses.

    Each input point is billable according to the selected provider. Results preserve input order.
    At most 500 coordinates are accepted per call.
    """
    client = await _client(ctx)
    return await client.batch_reverse_geocode(
        [_point(point) for point in points], language=language, region=region
    )


@mcp.tool()
async def get_route(
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
    client = await _client(ctx)
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
    origins: list[Point],
    destinations: list[Point],
    ctx: Context[ServerSession, None],
    mode: TravelMode = "driving",
) -> dict[str, object]:
    """Calculate a batch distance/time matrix with Google Maps or Mapbox.

    Matrix elements are billable as origins multiplied by destinations. Calls are split to respect
    provider request limits. A single tool call is capped at 2,500 elements to bound cost.
    """
    client = await _client(ctx)
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
