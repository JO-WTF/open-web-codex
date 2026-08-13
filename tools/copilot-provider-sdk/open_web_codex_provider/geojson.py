"""Bounded, content-derived GeoJSON profiles for model-authored presentation."""

from __future__ import annotations

import math
from collections.abc import Mapping
from typing import Any, Literal

from pydantic import BaseModel, ConfigDict, Field

from .errors import ProviderContractError

MAX_FEATURES = 100_000
MAX_FEATURE_TYPES = 32
MAX_PROPERTIES_PER_TYPE = 64
MAX_ENUM_VALUES = 16
MAX_SAMPLE_PROPERTIES = 32
MAX_PROFILE_STRING = 128

JsonValueType = Literal["null", "boolean", "number", "string", "array", "object"]


class GeoJsonPropertyProfile(BaseModel):
    model_config = ConfigDict(extra="forbid")

    name: str = Field(min_length=1, max_length=128)
    types: list[JsonValueType] = Field(min_length=1, max_length=6)
    enum_values: list[str] | None = Field(default=None, max_length=MAX_ENUM_VALUES)
    minimum: float | None = None
    maximum: float | None = None


class GeoJsonFeatureTypeProfile(BaseModel):
    model_config = ConfigDict(extra="forbid")

    value: str = Field(min_length=1, max_length=128)
    feature_count: int = Field(ge=1, le=MAX_FEATURES)
    geometry_types: list[str] = Field(min_length=1, max_length=8)
    properties: list[GeoJsonPropertyProfile] = Field(max_length=MAX_PROPERTIES_PER_TYPE)
    sample_properties: dict[str, bool | float | str | None] = Field(
        max_length=MAX_SAMPLE_PROPERTIES
    )


class GeoJsonProfile(BaseModel):
    """Small projection derived from one exact GeoJSON FeatureCollection."""

    model_config = ConfigDict(extra="forbid")

    schema_version: Literal["geojson-profile.v1"] = "geojson-profile.v1"
    feature_count: int = Field(ge=0, le=MAX_FEATURES)
    discriminator_property: str | None = Field(default=None, min_length=1, max_length=128)
    feature_types: list[GeoJsonFeatureTypeProfile] = Field(max_length=MAX_FEATURE_TYPES)


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

    return GeoJsonProfile(
        feature_count=len(features),
        discriminator_property=discriminator_property if use_discriminator else None,
        feature_types=[
            _profile_group(value, rows)
            for value, rows in sorted(groups.items(), key=lambda item: item[0])
        ],
    )


def _profile_group(
    value: str,
    rows: list[tuple[str, Mapping[str, Any]]],
) -> GeoJsonFeatureTypeProfile:
    property_values: dict[str, list[Any]] = {}
    geometry_types: set[str] = set()
    for geometry_type, properties in rows:
        geometry_types.add(geometry_type)
        for name, item in properties.items():
            if not isinstance(name, str) or not name or len(name) > 128:
                raise ProviderContractError("geojson_property_name_invalid")
            property_values.setdefault(name, []).append(item)
    if len(property_values) > MAX_PROPERTIES_PER_TYPE:
        raise ProviderContractError("geojson_properties_too_many")

    sample: dict[str, bool | float | str | None] = {}
    for name, item in rows[0][1].items():
        scalar = _sample_scalar(item)
        if scalar is not _UNSUPPORTED and len(sample) < MAX_SAMPLE_PROPERTIES:
            sample[name] = scalar

    return GeoJsonFeatureTypeProfile(
        value=value,
        feature_count=len(rows),
        geometry_types=sorted(geometry_types),
        properties=[
            _profile_property(name, values)
            for name, values in sorted(property_values.items(), key=lambda item: item[0])
        ],
        sample_properties=sample,
    )


def _profile_property(name: str, values: list[Any]) -> GeoJsonPropertyProfile:
    types = sorted({_json_type(value) for value in values})
    strings = {
        value
        for value in values
        if isinstance(value, str) and len(value) <= MAX_PROFILE_STRING
    }
    enum_values = sorted(strings) if strings and len(strings) <= MAX_ENUM_VALUES else None
    numbers = [
        float(value)
        for value in values
        if isinstance(value, (int, float))
        and not isinstance(value, bool)
        and math.isfinite(float(value))
    ]
    return GeoJsonPropertyProfile(
        name=name,
        types=types,
        enum_values=enum_values,
        minimum=min(numbers) if numbers else None,
        maximum=max(numbers) if numbers else None,
    )


def _json_type(value: Any) -> JsonValueType:
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


_UNSUPPORTED = object()


def _sample_scalar(value: Any) -> bool | float | str | None | object:
    if value is None or isinstance(value, bool):
        return value
    if isinstance(value, (int, float)) and not isinstance(value, bool):
        number = float(value)
        if not math.isfinite(number):
            raise ProviderContractError("geojson_property_number_invalid")
        return number
    if isinstance(value, str):
        return value[:MAX_PROFILE_STRING]
    return _UNSUPPORTED
