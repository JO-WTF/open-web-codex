"""The single current Map card contract.

Mapbox owns layer semantics. Open Web owns authorization of GeoJSON source data,
camera defaults, and the optional hover/legend extensions.
"""

from __future__ import annotations

import json
import subprocess
from pathlib import Path
from typing import Literal

from open_web_codex_provider import (
    GeoJsonFeatureTypeProfile,
    GeoJsonProfile,
    GeoJsonResourceRef,
)
from pydantic import BaseModel, ConfigDict, Field, model_validator

_VALIDATOR = Path(__file__).parent / "assets" / "validate-mapbox-style.mjs"


class ExtensibleModel(BaseModel):
    model_config = ConfigDict(extra="allow")


class GeoJsonSource(ExtensibleModel):
    """Mapbox GeoJSON source addressed by an authorized MCP Resource."""

    model_config = ConfigDict(
        extra="allow",
        json_schema_extra={"not": {"required": ["data"]}},
    )

    type: Literal["geojson"]
    data_ref: GeoJsonResourceRef = Field(
        description=(
            "Copy the reviewed local MCP GeoJSON data_ref unchanged; GeoJSON contents "
            "must not be passed through the model context."
        ),
    )

    @model_validator(mode="after")
    def reject_inline_data(self) -> GeoJsonSource:
        if self.model_extra and "data" in self.model_extra:
            raise ValueError("source.data is not supported; use data_ref")
        return self


class HoverField(ExtensibleModel):
    property: str = Field(min_length=1)
    label: str | None = Field(default=None, min_length=1)


class HoverLayer(ExtensibleModel):
    layer: str = Field(min_length=1)
    title_property: str | None = Field(default=None, min_length=1)
    fields: list[str | HoverField] = Field(default_factory=list)

    @model_validator(mode="after")
    def validate_content(self) -> HoverLayer:
        if self.title_property is None and not self.fields:
            raise ValueError("hover layer requires title_property or fields")
        names = [field if isinstance(field, str) else field.property for field in self.fields]
        if any(not name.strip() for name in names):
            raise ValueError("hover field property names must not be empty")
        if len(names) != len(set(names)):
            raise ValueError("hover field properties must be unique")
        return self


class HoverExtension(ExtensibleModel):
    layers: list[HoverLayer]

    @model_validator(mode="after")
    def validate_unique_layers(self) -> HoverExtension:
        layer_ids = [layer.layer for layer in self.layers]
        if len(layer_ids) != len(set(layer_ids)):
            raise ValueError("hover layer references must be unique")
        return self


class LegendItem(ExtensibleModel):
    label: str = Field(min_length=1)
    color: str = Field(min_length=1)
    type: Literal["circle", "line", "fill"] | None = None


class LegendExtension(ExtensibleModel):
    title: str | None = None
    items: list[LegendItem] = Field(min_length=1)


class MapExtensions(ExtensibleModel):
    hover: HoverExtension | None = None
    legend: LegendExtension | None = None


class RendererResourceRef(BaseModel):
    model_config = ConfigDict(extra="forbid")

    type: Literal["mcp_resource"] = "mcp_resource"
    server: str
    uri: str
    format: Literal["geojson"] = "geojson"


class RendererSource(ExtensibleModel):
    type: Literal["geojson"]
    data: RendererResourceRef


class MapPayload(BaseModel):
    model_config = ConfigDict(extra="forbid")

    title: str = Field(min_length=1)
    intent: str = Field(min_length=1)
    status: Literal["loading", "ready", "error"]
    fallback_text: str | None = None
    summary: str | None = None
    sources: dict[str, RendererSource]
    layers: list[dict[str, object]]
    center: tuple[float, float] | None = None
    zoom: float | None = None
    bearing: float | None = None
    pitch: float | None = None
    extensions: MapExtensions | None = None


class Renderer(BaseModel):
    model_config = ConfigDict(extra="forbid")

    kind: Literal["map.v3"]
    payload: MapPayload


class Artifact(BaseModel):
    model_config = ConfigDict(extra="forbid")

    ref: str = Field(pattern=r"^[A-Za-z0-9_.-]{1,128}$")
    renderer: Renderer


class Embed(BaseModel):
    model_config = ConfigDict(extra="forbid")

    syntax: Literal["codex-inline-vis.artifact.v1"]
    code: str = Field(pattern=r'^::codex-inline-vis\{artifact="[A-Za-z0-9_.-]{1,128}"\}$')


class Warning(BaseModel):
    model_config = ConfigDict(extra="forbid")

    code: Literal["ignored_extra_input", "mapbox_style_warning"]
    path: str = Field(min_length=1, max_length=512)
    message: str | None = Field(default=None, min_length=1, max_length=1024)


class ToolResult(BaseModel):
    model_config = ConfigDict(extra="forbid")

    type: Literal["open-web-artifact"]
    kind: Literal["inline-visualization.v1"]
    artifact: Artifact
    embed: Embed
    warnings: list[Warning] | None = None

    @model_validator(mode="after")
    def validate_embed(self) -> ToolResult:
        expected = f'::codex-inline-vis{{artifact="{self.artifact.ref}"}}'
        if self.embed.code != expected:
            raise ValueError("embed code must reference artifact.ref exactly")
        return self


def renderer_sources(
    sources: dict[str, GeoJsonSource],
) -> dict[str, RendererSource]:
    rendered: dict[str, RendererSource] = {}
    for source_id, source in sources.items():
        options = source.model_dump(
            mode="json",
            by_alias=True,
            exclude={"data_ref"},
            exclude_none=True,
        )
        rendered[source_id] = RendererSource(
            data=RendererResourceRef(
                server=source.data_ref.server,
                uri=source.data_ref.uri,
            ),
            **options,
        )
    return rendered


def validate_style(
    sources: dict[str, GeoJsonSource],
    layers: list[dict[str, object]],
    *,
    center: tuple[float, float] | None,
    zoom: float | None,
    bearing: float | None,
    pitch: float | None,
) -> list[Warning]:
    """Validate with Mapbox's official Style Specification implementation."""

    style_sources: dict[str, object] = {}
    for source_id, source in sources.items():
        value = source.model_dump(
            mode="json",
            by_alias=True,
            exclude={"data_ref"},
            exclude_none=True,
        )
        if source.data_ref is not None:
            value["data"] = {"type": "FeatureCollection", "features": []}
        style_sources[source_id] = value
    style: dict[str, object] = {
        "version": 8,
        # The platform-owned basemap supplies glyphs at render time. Include the
        # canonical placeholder so the official validator can validate symbol
        # text layers in this source/layer overlay.
        "glyphs": "mapbox://fonts/mapbox/{fontstack}/{range}.pbf",
        "sources": style_sources,
        "layers": layers,
    }
    for key, value in {
        "center": center,
        "zoom": zoom,
        "bearing": bearing,
        "pitch": pitch,
    }.items():
        if value is not None:
            style[key] = value
    try:
        completed = subprocess.run(
            ["node", str(_VALIDATOR)],
            input=json.dumps(style, ensure_ascii=False),
            text=True,
            capture_output=True,
            check=True,
            timeout=10,
        )
        output = json.loads(completed.stdout)
    except (OSError, subprocess.SubprocessError, json.JSONDecodeError) as error:
        raise RuntimeError(f"Mapbox Style Spec validator is unavailable: {error}") from error

    warnings: list[Warning] = []
    errors: list[str] = []
    for diagnostic in output.get("diagnostics", []):
        message = str(diagnostic.get("message", "Mapbox style validation failed"))
        if diagnostic.get("severity") == "warning":
            path = message.split(":", 1)[0].strip() or "style"
            warnings.append(
                Warning(
                    code="mapbox_style_warning",
                    path=path[:512],
                    message=message[:1024],
                )
            )
        else:
            errors.append(message)
    if errors:
        raise ValueError("Invalid Mapbox style: " + "; ".join(errors))
    return warnings


def extension_warnings(extensions: MapExtensions | None) -> list[Warning]:
    paths: list[str] = []

    def collect(value: object, path: str) -> None:
        if isinstance(value, BaseModel):
            for key in value.model_extra or {}:
                paths.append(f"{path}.{key}")
            for name in type(value).model_fields:
                child = getattr(value, name)
                if child is not None:
                    collect(child, f"{path}.{name}")
        elif isinstance(value, list):
            for index, child in enumerate(value):
                collect(child, f"{path}[{index}]")

    if extensions is not None:
        collect(extensions, "extensions")
    return [Warning(code="ignored_extra_input", path=path[:512]) for path in dict.fromkeys(paths)]


def sanitized_extensions(extensions: MapExtensions | None) -> MapExtensions | None:
    if extensions is None:
        return None
    hover = None
    if extensions.hover is not None:
        hover = HoverExtension(
            layers=[
                HoverLayer(
                    layer=layer.layer,
                    title_property=layer.title_property,
                    fields=[
                        field
                        if isinstance(field, str)
                        else HoverField(property=field.property, label=field.label)
                        for field in layer.fields
                    ],
                )
                for layer in extensions.hover.layers
            ]
        )
    legend = None
    if extensions.legend is not None:
        legend = LegendExtension(
            title=extensions.legend.title,
            items=[
                LegendItem(label=item.label, color=item.color, type=item.type)
                for item in extensions.legend.items
            ],
        )
    return MapExtensions(hover=hover, legend=legend)


def validate_extension_graph(
    extensions: MapExtensions | None,
    layers: list[dict[str, object]],
) -> None:
    if extensions is None or extensions.hover is None:
        return
    layer_by_id = {layer.get("id"): layer for layer in layers if isinstance(layer.get("id"), str)}
    for hover in extensions.hover.layers:
        layer = layer_by_id.get(hover.layer)
        if layer is None:
            raise ValueError(f"hover references unknown Mapbox layer: {hover.layer}")
        if not isinstance(layer.get("source"), str):
            raise ValueError(f"hover requires a source-backed Mapbox layer: {hover.layer}")


def validate_profile_graph(
    sources: dict[str, GeoJsonSource],
    layers: list[dict[str, object]],
    extensions: MapExtensions | None,
) -> None:
    """Reject style and hover fields absent from the producing Resource profile."""

    layer_profiles: dict[str, tuple[GeoJsonFeatureTypeProfile, ...]] = {}
    for layer in layers:
        layer_id = layer.get("id")
        source_id = layer.get("source")
        if not isinstance(layer_id, str) or not isinstance(source_id, str):
            continue
        source = sources.get(source_id)
        if source is None:
            continue
        selected = _selected_feature_types(source.data_ref.profile, layer.get("filter"))
        layer_profiles[layer_id] = selected
        available_fields = {
            item.name for feature_type in selected for item in feature_type.properties
        }
        unknown = sorted(_property_references(layer) - available_fields)
        if unknown:
            raise ValueError(
                f"Mapbox layer {layer_id} references properties absent from its GeoJSON "
                f"profile: {', '.join(unknown)}"
            )
        _validate_layer_geometry(layer_id, layer.get("type"), selected)

    if extensions is None or extensions.hover is None:
        return
    for hover in extensions.hover.layers:
        selected = layer_profiles.get(hover.layer, ())
        available_fields = {
            item.name for feature_type in selected for item in feature_type.properties
        }
        requested = {
            field if isinstance(field, str) else field.property for field in hover.fields
        }
        if hover.title_property is not None:
            requested.add(hover.title_property)
        unknown = sorted(requested - available_fields)
        if unknown:
            raise ValueError(
                f"hover layer {hover.layer} references properties absent from its GeoJSON "
                f"profile: {', '.join(unknown)}"
            )


def _selected_feature_types(
    profile: GeoJsonProfile,
    filter_expression: object,
) -> tuple[GeoJsonFeatureTypeProfile, ...]:
    selected = tuple(profile.feature_types)
    discriminator = profile.discriminator_property
    if discriminator is None:
        return selected
    values = _required_equal_values(filter_expression, discriminator)
    if not values:
        return selected
    selected = tuple(item for item in selected if item.value in values)
    if not selected:
        raise ValueError(
            f"Mapbox filter selects unknown {discriminator} value: {', '.join(sorted(values))}"
        )
    return selected


def _required_equal_values(expression: object, property_name: str) -> set[str]:
    if not isinstance(expression, list) or not expression:
        return set()
    if expression[0] == "==" and len(expression) == 3:
        left, right = expression[1], expression[2]
        if left == ["get", property_name] and isinstance(right, str):
            return {right}
        if right == ["get", property_name] and isinstance(left, str):
            return {left}
        return set()
    if expression[0] != "all":
        return set()
    values: set[str] = set()
    for child in expression[1:]:
        child_values = _required_equal_values(child, property_name)
        if not child_values:
            continue
        values = child_values if not values else values & child_values
    return values


def _property_references(value: object) -> set[str]:
    found: set[str] = set()
    if isinstance(value, list):
        if len(value) >= 2 and value[0] in {"get", "has"} and isinstance(value[1], str):
            found.add(value[1])
        for child in value:
            found.update(_property_references(child))
    elif isinstance(value, dict):
        for child in value.values():
            found.update(_property_references(child))
    return found


def _validate_layer_geometry(
    layer_id: str,
    layer_type: object,
    selected: tuple[GeoJsonFeatureTypeProfile, ...],
) -> None:
    geometries = {
        geometry for feature_type in selected for geometry in feature_type.geometry_types
    }
    supported = {
        "circle": {"Point", "MultiPoint"},
        "line": {"LineString", "MultiLineString", "Polygon", "MultiPolygon"},
        "fill": {"Polygon", "MultiPolygon"},
    }.get(layer_type)
    if supported is not None and geometries.isdisjoint(supported):
        raise ValueError(
            f"Mapbox layer {layer_id} type {layer_type} cannot render profiled geometries: "
            f"{', '.join(sorted(geometries)) or 'none'}"
        )
