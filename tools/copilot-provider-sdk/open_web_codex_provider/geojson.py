"""Compact, content-derived GeoJSON profiles for model-authored presentation."""

from __future__ import annotations

import math
import re
from collections.abc import Mapping
from typing import Any, Literal

from pydantic import BaseModel, ConfigDict, Field, field_validator, model_validator

from .contracts import ResourceRef
from .errors import ProviderContractError

MAX_FEATURES = 100_000
MAX_FEATURE_TYPES = 32
MAX_PROPERTIES_PER_TYPE = 64

GeoJsonPropertyKind = Literal[
    "boolean",
    "number",
    "string",
    "null",
    "array",
    "object",
    "mixed",
    "boolean?",
    "number?",
    "string?",
    "array?",
    "object?",
    "mixed?",
]


class GeoJsonFeatureTypeProfile(BaseModel):
    """One compact schema slice used to validate a map layer."""

    model_config = ConfigDict(extra="forbid")

    value: str = Field(min_length=1, max_length=128)
    feature_count: int = Field(ge=1, le=MAX_FEATURES)
    geometry_types: list[str] = Field(min_length=1, max_length=8)
    # A property-name → observed-value-kind map is deliberately kept compact:
    # it exposes no source values, but lets a renderer reject expressions that
    # would otherwise silently fall back at runtime (for example, numeric
    # interpolation over a string field or a comparison against an all-null
    # field).
    properties: dict[str, GeoJsonPropertyKind] = Field(max_length=MAX_PROPERTIES_PER_TYPE)

    @field_validator("properties")
    @classmethod
    def validate_properties(
        cls,
        properties: dict[str, GeoJsonPropertyKind],
    ) -> dict[str, GeoJsonPropertyKind]:
        if any(not property_name or len(property_name) > 128 for property_name in properties):
            raise ValueError("GeoJSON feature profile property name is invalid")
        return properties


class GeoJsonProfile(BaseModel):
    """Bounded source schema needed to author and validate one map card."""

    model_config = ConfigDict(extra="forbid")

    schema_version: Literal["geojson-profile.v3"] = "geojson-profile.v3"
    feature_count: int = Field(ge=0, le=MAX_FEATURES)
    discriminator_property: str | None = Field(default=None, min_length=1, max_length=128)
    feature_types: list[GeoJsonFeatureTypeProfile] = Field(max_length=MAX_FEATURE_TYPES)


class GeoJsonResourceRef(ResourceRef):
    """One exact GeoJSON Resource identity plus its compact map-validation profile."""

    format: Literal["geojson"] = "geojson"
    profile: GeoJsonProfile

    @model_validator(mode="after")
    def validate_local_resource_identity(self) -> GeoJsonResourceRef:
        if self.server.startswith("mcp__"):
            raise ValueError("GeoJSON Resource server must be the raw MCP server ID")
        if any(
            character.isspace() or ord(character) < 32 or ord(character) == 127
            for character in self.uri
        ):
            raise ValueError("GeoJSON Resource URI contains invalid characters")
        scheme, separator, resource = self.uri.partition("://")
        if (
            not separator
            or not resource
            or scheme in {"file", "http", "https"}
            or re.fullmatch(r"[a-z][a-z0-9+.-]*", scheme) is None
        ):
            raise ValueError("GeoJSON Resource URI must be a non-public MCP Resource URI")
        return self


def derive_geojson_profile(
    geojson: Mapping[str, Any],
    *,
    discriminator_property: str = "kind",
) -> GeoJsonProfile:
    """Derive a bounded profile without copying the FeatureCollection into model context."""

    if geojson.get("type") != "FeatureCollection":
        raise ProviderContractError("geojson_feature_collection_required")
    features = geojson.get("features")
    if not isinstance(features, list) or len(features) > MAX_FEATURES:
        raise ProviderContractError("geojson_features_invalid")

    use_discriminator = bool(features)
    groups: dict[str, list[tuple[str, Mapping[str, Any]]]] = {}
    for feature in features:
        if not isinstance(feature, Mapping) or feature.get("type") != "Feature":
            raise ProviderContractError("geojson_feature_invalid")
        geometry = feature.get("geometry")
        properties = feature.get("properties")
        if not isinstance(geometry, Mapping) or not isinstance(geometry.get("type"), str):
            raise ProviderContractError("geojson_geometry_invalid")
        if not isinstance(properties, Mapping):
            raise ProviderContractError("geojson_properties_invalid")
        discriminator = properties.get(discriminator_property)
        if not isinstance(discriminator, str) or not discriminator.strip():
            use_discriminator = False
        geometry_type = geometry["type"]
        group_key = str(discriminator) if use_discriminator else f"geometry:{geometry_type}"
        groups.setdefault(group_key, []).append((geometry_type, properties))

    if not use_discriminator and features:
        groups = {}
        for feature in features:
            geometry = feature["geometry"]
            properties = feature["properties"]
            geometry_type = geometry["type"]
            groups.setdefault(f"geometry:{geometry_type}", []).append(
                (geometry_type, properties)
            )
    if len(groups) > MAX_FEATURE_TYPES:
        raise ProviderContractError("geojson_feature_types_too_many")

    profile = GeoJsonProfile(
        feature_count=len(features),
        discriminator_property=discriminator_property if use_discriminator else None,
        feature_types=[
            _profile_group(value, rows)
            for value, rows in sorted(groups.items(), key=lambda item: item[0])
        ],
    )
    return profile


def _profile_group(
    value: str,
    rows: list[tuple[str, Mapping[str, Any]]],
) -> GeoJsonFeatureTypeProfile:
    geometry_types: set[str] = set()
    property_names: set[str] = set()
    for geometry_type, properties in rows:
        geometry_types.add(geometry_type)
        for name in properties:
            if not isinstance(name, str) or not name or len(name) > 128:
                raise ProviderContractError("geojson_property_name_invalid")
            property_names.add(name)
    if len(property_names) > MAX_PROPERTIES_PER_TYPE:
        raise ProviderContractError("geojson_properties_too_many")

    return GeoJsonFeatureTypeProfile(
        value=value,
        feature_count=len(rows),
        geometry_types=sorted(geometry_types),
        properties={
            name: _property_kind([properties.get(name) for _, properties in rows])
            for name in sorted(property_names)
        },
    )


def _property_kind(values: list[Any]) -> GeoJsonPropertyKind:
    kinds = {_json_value_kind(value) for value in values}
    nullable = "null" in kinds
    kinds.discard("null")
    if not kinds:
        return "null"
    kind = next(iter(kinds)) if len(kinds) == 1 else "mixed"
    return f"{kind}?" if nullable else kind


def _json_value_kind(value: Any) -> Literal[
    "boolean", "number", "string", "null", "array", "object"
]:
    if value is None:
        return "null"
    if isinstance(value, bool):
        return "boolean"
    if isinstance(value, (int, float)):
        if not math.isfinite(float(value)):
            raise ProviderContractError("geojson_property_number_invalid")
        return "number"
    if isinstance(value, str):
        return "string"
    if isinstance(value, list):
        return "array"
    if isinstance(value, Mapping):
        return "object"
    raise ProviderContractError("geojson_property_value_invalid")
