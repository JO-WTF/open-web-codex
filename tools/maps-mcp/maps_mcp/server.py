"""FastMCP entry point for provider-neutral Google Maps and Mapbox tools."""

from __future__ import annotations

import argparse
from pathlib import Path
from typing import Annotated, Literal

from mcp.server.fastmcp import Context, FastMCP
from mcp.server.session import ServerSession
from mcp.types import CallToolResult, ResourceLink, TextContent
from pydantic import BaseModel, ConfigDict, Field, model_validator

from .clients import GoogleMapsClient, MapboxMapsClient
from .credential_prompt import LoopbackCredentialPrompt
from .credentials import WorkspaceCredentialStore
from .data_refs import GeoJsonResourceStore, PublishedGeoJson

Provider = Literal["google", "mapbox"]
TravelMode = Literal[
    "driving", "driving_traffic", "walking", "bicycling", "transit", "two_wheeler"
]
GeometryKind = Literal["point", "line", "polygon"]
MCP_SERVER_NAME = "map_utils"
CssColor = Annotated[
    str,
    Field(
        pattern=(
            r"^(#[0-9A-Fa-f]{6}|red|orange|yellow|green|blue|purple|pink|"
            r"gray|black|white)$"
        )
    ),
]
DashValue = Annotated[float, Field(ge=0.1, le=64)]


class Point(BaseModel):
    latitude: float = Field(ge=-90, le=90)
    longitude: float = Field(ge=-180, le=180)


class FitPadding(BaseModel):
    model_config = ConfigDict(extra="forbid")

    top: float = Field(ge=0, le=256)
    right: float = Field(ge=0, le=256)
    bottom: float = Field(ge=0, le=256)
    left: float = Field(ge=0, le=256)


class FitViewport(BaseModel):
    model_config = ConfigDict(extra="forbid")

    mode: Literal["fit"] = "fit"
    padding: float | FitPadding | None = Field(default=None)
    max_zoom: float | None = Field(default=None, ge=0, le=24)
    min_zoom: float | None = Field(default=None, ge=0, le=24)

    @model_validator(mode="after")
    def validate_zoom_range(self) -> FitViewport:
        if (
            self.min_zoom is not None
            and self.max_zoom is not None
            and self.min_zoom > self.max_zoom
        ):
            raise ValueError("min_zoom must not exceed max_zoom")
        return self


class CameraViewport(BaseModel):
    model_config = ConfigDict(extra="forbid")

    mode: Literal["camera"] = "camera"
    center: tuple[float, float]
    zoom: float = Field(ge=0, le=24)
    bearing: float | None = Field(default=None, ge=-180, le=180)
    pitch: float | None = Field(default=None, ge=0, le=85)

    @model_validator(mode="after")
    def validate_center(self) -> CameraViewport:
        longitude, latitude = self.center
        if not -180 <= longitude <= 180 or not -90 <= latitude <= 90:
            raise ValueError("camera center must be [longitude, latitude]")
        return self


MapViewport = Annotated[FitViewport | CameraViewport, Field(discriminator="mode")]


class McpResourceMapData(BaseModel):
    model_config = ConfigDict(extra="forbid")

    type: Literal["mcp_resource"] = "mcp_resource"
    server: Literal["map_utils"] = Field(
        description=(
            "Raw MCP server ID for Resource reads. Use this exact value, not the "
            "model-visible mcp__map_utils Tool namespace."
        ),
    )
    uri: str = Field(
        pattern=r"^maps-data://geojson/[A-Za-z0-9_.-]{1,128}$",
        description=(
            "Canonical MCP Resource URI. Copy this unchanged into create_map_card and use "
            "the same value as read_mcp_resource.uri when the GeoJSON contents are needed."
        ),
    )
    format: Literal["geojson"] = "geojson"


class InlineMapData(BaseModel):
    model_config = ConfigDict(extra="forbid")

    type: Literal["inline"] = "inline"
    format: Literal["geojson"] = "geojson"
    geojson: dict[str, object]

    @model_validator(mode="after")
    def validate_geojson_root(self) -> InlineMapData:
        allowed = {
            "FeatureCollection",
            "Feature",
            "GeometryCollection",
            "Point",
            "MultiPoint",
            "LineString",
            "MultiLineString",
            "Polygon",
            "MultiPolygon",
        }
        if self.geojson.get("type") not in allowed:
            raise ValueError("inline data must contain a GeoJSON root object")
        return self


MapData = Annotated[McpResourceMapData | InlineMapData, Field(discriminator="type")]


class MapSource(BaseModel):
    model_config = ConfigDict(extra="forbid")

    id: str = Field(pattern=r"^[A-Za-z0-9_.-]{1,128}$")
    data: MapData


class PointStyle(BaseModel):
    model_config = ConfigDict(extra="forbid")

    color: CssColor | None = None
    opacity: float | None = Field(default=None, ge=0, le=1)
    radius: float | None = Field(default=None, ge=1, le=64)
    stroke_color: CssColor | None = None
    stroke_width: float | None = Field(default=None, ge=0, le=32)
    stroke_opacity: float | None = Field(default=None, ge=0, le=1)


class LineStyle(BaseModel):
    model_config = ConfigDict(extra="forbid")

    color: CssColor | None = None
    opacity: float | None = Field(default=None, ge=0, le=1)
    width: float | None = Field(default=None, ge=0.5, le=32)
    dash: list[DashValue] | None = Field(default=None, min_length=1, max_length=8)
    cap: Literal["butt", "round", "square"] | None = None
    join: Literal["bevel", "round", "miter"] | None = None


class PolygonStyle(BaseModel):
    model_config = ConfigDict(extra="forbid")

    fill_color: CssColor | None = None
    fill_opacity: float | None = Field(default=None, ge=0, le=1)
    stroke_color: CssColor | None = None
    stroke_width: float | None = Field(default=None, ge=0, le=32)
    stroke_opacity: float | None = Field(default=None, ge=0, le=1)
    stroke_dash: list[DashValue] | None = Field(default=None, min_length=1, max_length=8)


class PointLayer(BaseModel):
    model_config = ConfigDict(extra="forbid")

    id: str = Field(pattern=r"^[A-Za-z0-9_.-]{1,128}$")
    source: str = Field(pattern=r"^[A-Za-z0-9_.-]{1,128}$")
    geometry: Literal["point"] = "point"
    label_property: str | None = None
    style: PointStyle = Field(default_factory=PointStyle)


class LineLayer(BaseModel):
    model_config = ConfigDict(extra="forbid")

    id: str = Field(pattern=r"^[A-Za-z0-9_.-]{1,128}$")
    source: str = Field(pattern=r"^[A-Za-z0-9_.-]{1,128}$")
    geometry: Literal["line"] = "line"
    label_property: str | None = None
    style: LineStyle = Field(default_factory=LineStyle)


class PolygonLayer(BaseModel):
    model_config = ConfigDict(extra="forbid")

    id: str = Field(pattern=r"^[A-Za-z0-9_.-]{1,128}$")
    source: str = Field(pattern=r"^[A-Za-z0-9_.-]{1,128}$")
    geometry: Literal["polygon"] = "polygon"
    label_property: str | None = None
    style: PolygonStyle = Field(default_factory=PolygonStyle)


MapLayer = Annotated[PointLayer | LineLayer | PolygonLayer, Field(discriminator="geometry")]


class LegendItem(BaseModel):
    model_config = ConfigDict(extra="forbid")

    label: str = Field(min_length=1)
    color: CssColor


class MapLegend(BaseModel):
    model_config = ConfigDict(extra="forbid")

    title: str | None = None
    items: list[LegendItem] = Field(min_length=1, max_length=32)


class MapCardPayload(BaseModel):
    """Browser-facing map.v2 contract."""

    model_config = ConfigDict(extra="forbid")

    title: str = Field(min_length=1)
    intent: str = Field(min_length=1)
    status: Literal["loading", "ready", "error"]
    fallback_text: str | None = None
    summary: str | None = None
    viewport: MapViewport
    sources: list[MapSource] = Field(min_length=1, max_length=64)
    layers: list[MapLayer] = Field(min_length=1, max_length=128)
    legend: MapLegend | None = None

    @model_validator(mode="after")
    def validate_graph(self) -> MapCardPayload:
        source_ids = [source.id for source in self.sources]
        layer_ids = [layer.id for layer in self.layers]
        if len(source_ids) != len(set(source_ids)):
            raise ValueError("map source ids must be unique")
        if len(layer_ids) != len(set(layer_ids)):
            raise ValueError("map layer ids must be unique")
        unknown = {layer.source for layer in self.layers}.difference(source_ids)
        if unknown:
            raise ValueError(f"map layers reference unknown sources: {sorted(unknown)}")
        return self


class MapCardToolResult(BaseModel):
    """Schema-validated structured MCP output for ``create_map_card``."""

    model_config = ConfigDict(extra="forbid")

    type: Literal["open-web-card"]
    kind: Literal["map.v2"]
    card: MapCardPayload


class GeoJsonToolResult(BaseModel):
    model_config = ConfigDict(extra="forbid")

    provider: str
    summary: str
    feature_count: int = Field(ge=0)
    data_ref: McpResourceMapData = Field(
        description=(
            "Copy this object unchanged into create_map_card sources[].data. "
            "Its server and uri are the canonical MCP Resource routing identity."
        ),
    )


mcp = FastMCP(
    "Map Utils",
    instructions=(
        "Geocoding and routing tools publish GeoJSON as MCP Resources. Copy structuredContent."
        "data_ref unchanged into create_map_card sources[].data. For read_mcp_resource, pass "
        "data_ref.server as server and data_ref.uri as uri unchanged; the server is map_utils, "
        "never the model-visible mcp__map_utils namespace. Never copy Resource JSON into the "
        "assistant reply. "
        "create_map_card returns schema-validated map.v2 structuredContent for inline host "
        "rendering. One selected provider and API key are shared by every maps tool."
    ),
    json_response=True,
)
_credential_store = WorkspaceCredentialStore(Path.cwd())
_resource_store = GeoJsonResourceStore(Path.cwd())


@mcp.resource(
    "maps-data://geojson/{resource_id}",
    name="maps_geojson",
    title="Maps GeoJSON",
    mime_type="application/geo+json",
)
def read_geojson_resource(resource_id: str) -> str:
    """Read GeoJSON previously published by a maps data tool."""
    return _resource_store.read(resource_id)


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


def _resource_result(
    provider: object,
    summary: str,
    geojson: dict[str, object],
) -> CallToolResult:
    published = _resource_store.publish(geojson)
    structured = GeoJsonToolResult(
        provider=str(provider),
        summary=summary,
        feature_count=len(geojson.get("features", [])),
        data_ref=McpResourceMapData(server=MCP_SERVER_NAME, uri=published.uri),
    ).model_dump(mode="json")
    return CallToolResult(
        content=[
            TextContent(type="text", text=summary),
            _resource_link(published, summary),
        ],
        structuredContent=structured,
    )


def _resource_link(published: PublishedGeoJson, description: str) -> ResourceLink:
    return ResourceLink(
        type="resource_link",
        name=published.resource_id,
        title="Maps GeoJSON",
        uri=published.uri,
        description=description,
        mimeType="application/geo+json",
        size=published.size,
    )


def _geocode_geojson(result: dict[str, object]) -> dict[str, object]:
    features: list[dict[str, object]] = []
    results = result.get("results")
    if isinstance(results, list):
        for entry in results:
            if not isinstance(entry, dict):
                continue
            match = entry.get("match")
            if not isinstance(match, dict):
                continue
            location = match.get("location")
            if not isinstance(location, dict):
                continue
            longitude = location.get("longitude")
            latitude = location.get("latitude")
            if not isinstance(longitude, (int, float)) or not isinstance(
                latitude, (int, float)
            ):
                continue
            properties = {
                "index": entry.get("index"),
                "label": match.get("formatted_address") or entry.get("query"),
                "description": match.get("formatted_address"),
                "query": entry.get("query"),
                "place_id": match.get("place_id") or match.get("mapbox_id"),
            }
            features.append(
                {
                    "type": "Feature",
                    "properties": {key: value for key, value in properties.items() if value},
                    "geometry": {
                        "type": "Point",
                        "coordinates": [longitude, latitude],
                    },
                }
            )
    return {"type": "FeatureCollection", "features": features}


def _decode_polyline(encoded: str) -> list[list[float]]:
    coordinates: list[list[float]] = []
    latitude = 0
    longitude = 0
    index = 0
    while index < len(encoded):
        deltas: list[int] = []
        for _ in range(2):
            shift = 0
            result = 0
            while True:
                byte = ord(encoded[index]) - 63
                index += 1
                result |= (byte & 0x1F) << shift
                shift += 5
                if byte < 0x20:
                    break
            deltas.append(~(result >> 1) if result & 1 else result >> 1)
        latitude += deltas[0]
        longitude += deltas[1]
        coordinates.append([longitude / 100000, latitude / 100000])
    return coordinates


def _route_geojson(
    result: dict[str, object],
    fallback: list[Point],
) -> dict[str, object]:
    features: list[dict[str, object]] = []
    routes = result.get("routes")
    if isinstance(routes, list):
        for index, route in enumerate(routes):
            if not isinstance(route, dict):
                continue
            geometry = route.get("geometry")
            if not isinstance(geometry, dict):
                polyline = route.get("polyline")
                encoded = (
                    polyline.get("encodedPolyline")
                    if isinstance(polyline, dict)
                    else None
                )
                if isinstance(encoded, str):
                    geometry = {
                        "type": "LineString",
                        "coordinates": _decode_polyline(encoded),
                    }
            if not isinstance(geometry, dict):
                geometry = {
                    "type": "LineString",
                    "coordinates": [
                        [point.longitude, point.latitude] for point in fallback
                    ],
                }
            features.append(
                {
                    "type": "Feature",
                    "properties": {
                        "index": index,
                        "label": f"Route {index + 1}",
                        "distance_meters": route.get("distanceMeters")
                        or route.get("distance"),
                        "duration": route.get("duration"),
                    },
                    "geometry": geometry,
                }
            )
    return {"type": "FeatureCollection", "features": features}


@mcp.tool(structured_output=True)
async def create_map_card(
    title: str,
    sources: list[MapSource],
    layers: list[MapLayer],
    viewport: MapViewport | None = None,
    intent: str = "visualization",
    fallback_text: str | None = None,
    summary: str | None = None,
    legend: MapLegend | None = None,
) -> Annotated[CallToolResult, MapCardToolResult]:
    """Create an inline map.v2 reply card from GeoJSON sources and styled layers.

    An MCP Resource source must be copied unchanged from an earlier data tool result in the
    same Run and Thread. Use a camera viewport for an explicit center/zoom, otherwise fit all source
    data. The host consumes structuredContent; do not reproduce the card JSON in reply text.
    """
    clean_title = title.strip()
    if not clean_title:
        raise ValueError("title is required")
    card = MapCardPayload(
        title=clean_title,
        intent=intent.strip() or "visualization",
        status="ready",
        fallback_text=fallback_text.strip() if fallback_text else None,
        summary=summary.strip() if summary else None,
        viewport=viewport or FitViewport(),
        sources=sources,
        layers=layers,
        legend=legend,
    )
    result = MapCardToolResult(type="open-web-card", kind="map.v2", card=card)
    structured_content = result.model_dump(mode="json", exclude_none=True)
    return CallToolResult(
        content=[TextContent(type="text", text=f"Map card ready: {clean_title}")],
        structuredContent=structured_content,
    )


def _point(point: Point) -> dict[str, float]:
    return point.model_dump()


@mcp.tool(structured_output=True)
async def batch_geocode(
    addresses: list[str],
    ctx: Context[ServerSession, None],
    language: str | None = None,
    region: str | None = None,
) -> Annotated[CallToolResult, GeoJsonToolResult]:
    """Batch-convert addresses to GeoJSON and return its MCP Resource reference."""
    client = await _client(ctx)
    result = await client.batch_geocode(addresses, language=language, region=region)
    geojson = _geocode_geojson(result)
    count = len(geojson["features"])
    return _resource_result(
        result.get("provider"),
        (
            f"Geocoded {count} of {len(addresses)} addresses; use data_ref.server and "
            "data_ref.uri unchanged for map cards and Resource reads."
        ),
        geojson,
    )


@mcp.tool(structured_output=True)
async def batch_reverse_geocode(
    points: list[Point],
    ctx: Context[ServerSession, None],
    language: str | None = None,
    region: str | None = None,
) -> Annotated[CallToolResult, GeoJsonToolResult]:
    """Batch-convert coordinates to GeoJSON and return its MCP Resource reference."""
    client = await _client(ctx)
    result = await client.batch_reverse_geocode(
        [_point(point) for point in points], language=language, region=region
    )
    geojson = _geocode_geojson(result)
    count = len(geojson["features"])
    return _resource_result(
        result.get("provider"),
        (
            f"Reverse-geocoded {count} of {len(points)} coordinates; use data_ref.server and "
            "data_ref.uri unchanged for map cards and Resource reads."
        ),
        geojson,
    )


@mcp.tool(structured_output=True)
async def get_route(
    origin: Point,
    destination: Point,
    ctx: Context[ServerSession, None],
    waypoints: list[Point] | None = None,
    mode: TravelMode = "driving",
    alternatives: bool = False,
    include_steps: bool = False,
    language: str | None = None,
) -> Annotated[CallToolResult, GeoJsonToolResult]:
    """Request route GeoJSON and return its MCP Resource reference."""
    client = await _client(ctx)
    route_points = [origin, *(waypoints or []), destination]
    result = await client.get_route(
        _point(origin),
        _point(destination),
        waypoints=[_point(point) for point in waypoints or []],
        mode=mode,
        alternatives=alternatives,
        include_steps=include_steps,
        language=language,
    )
    geojson = _route_geojson(result, route_points)
    count = len(geojson["features"])
    return _resource_result(
        result.get("provider"),
        (
            f"Created {count} route geometries; use data_ref.server and data_ref.uri unchanged "
            "for map cards and Resource reads."
        ),
        geojson,
    )


@mcp.tool()
async def distance_matrix(
    origins: list[Point],
    destinations: list[Point],
    ctx: Context[ServerSession, None],
    mode: TravelMode = "driving",
) -> dict[str, object]:
    """Calculate a billable distance/time matrix (not a map data source)."""
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
        help="Workspace whose .codex directory stores credentials and GeoJSON Resources",
    )
    parser.add_argument(
        "--transport",
        choices=("stdio", "streamable-http"),
        default="stdio",
    )
    args = parser.parse_args()

    global _credential_store, _resource_store
    _credential_store = WorkspaceCredentialStore(args.workspace_root)
    _resource_store = GeoJsonResourceStore(args.workspace_root)
    mcp.run(transport=args.transport)


if __name__ == "__main__":
    main()
