"""Provider-owned JSON Schema sources for final delivery bundles."""

from __future__ import annotations

from collections.abc import Mapping
from typing import Any

from .map_service import NetworkComparisonMapBundle
from .report_service import NetworkPlanningReportBundle

DELIVERY_SCHEMA_SOURCES = {
    "network_comparison_map_bundle.v1": (
        NetworkComparisonMapBundle,
        "Network comparison map bundle",
    ),
    "network_planning_report_bundle.v1": (
        NetworkPlanningReportBundle,
        "Network planning report bundle",
    ),
}


def model_schema(schema_name: str) -> dict[str, Any]:
    """Return a stable, provider-owned schema without dropping model constraints."""

    try:
        model, title = DELIVERY_SCHEMA_SOURCES[schema_name]
    except KeyError as error:
        raise ValueError(f"unknown delivery schema: {schema_name}") from error
    schema = model.model_json_schema()
    schema["$schema"] = "https://json-schema.org/draft/2020-12/schema"
    schema["$id"] = f"urn:open-web-codex:supply-chain:{schema_name}"
    schema["title"] = title
    return schema


def model_schema_sources() -> Mapping[str, type]:
    """Expose the versioned model mapping for the generator and drift tests."""

    return {name: model for name, (model, _) in DELIVERY_SCHEMA_SOURCES.items()}
