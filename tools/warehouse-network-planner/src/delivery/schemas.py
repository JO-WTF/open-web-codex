"""Provider-owned JSON Schema sources for final delivery bundles."""

from __future__ import annotations

from typing import Any

from supply_chain_planner.delivery.report_service import NetworkPlanningReportBundle

DELIVERY_SCHEMA_SOURCES = {
    "network_planning_report_bundle.v2": (
        NetworkPlanningReportBundle,
        "Network planning report bundle",
    ),
}
DECIMAL_PATTERN = r"^(?!^[-+.]*$)[+-]?0*\d*\.?\d*$"


def _normalize_schema(value: Any) -> Any:
    if isinstance(value, dict):
        normalized = {key: _normalize_schema(item) for key, item in value.items()}
        branches = normalized.get("anyOf")
        if isinstance(branches, list):
            has_number = any(
                isinstance(branch, dict) and branch.get("type") == "number"
                for branch in branches
            )
            if has_number:
                for branch in branches:
                    if isinstance(branch, dict) and branch.get("type") == "string":
                        branch.setdefault("pattern", DECIMAL_PATTERN)
        return normalized
    if isinstance(value, list):
        return [_normalize_schema(item) for item in value]
    return value


def model_schema(schema_name: str) -> dict[str, Any]:
    """Return a stable, provider-owned schema without dropping model constraints."""

    try:
        model, title = DELIVERY_SCHEMA_SOURCES[schema_name]
    except KeyError as error:
        raise ValueError(f"unknown delivery schema: {schema_name}") from error
    schema = _normalize_schema(model.model_json_schema())
    schema["$schema"] = "https://json-schema.org/draft/2020-12/schema"
    schema["$id"] = f"urn:open-web-codex:supply-chain:{schema_name}"
    schema["title"] = title
    return schema
