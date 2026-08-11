"""Read-only supply-chain data MCP for the Data Agent."""

from __future__ import annotations

import argparse
import asyncio
import hashlib
import json
import math
import os
import re
from datetime import UTC, date, datetime
from decimal import Decimal, InvalidOperation
from pathlib import Path
from typing import Annotated, Any, Literal

from mcp.server.fastmcp import Context, FastMCP
from mcp.server.stdio import stdio_server
from mcp.types import (
    CallToolResult,
    EmbeddedResource,
    ResourceLink,
    TextResourceContents,
)
from pydantic import BaseModel, ConfigDict, Field

from .data_core import build_planning_dataset as aggregate_planning_dataset
from .geography import (
    build_administrative_candidates as _build_administrative_candidates,
)
from .geography import enrich_network_geography
from .geography import (
    load_administrative_catalog as _load_administrative_catalog,
)
from .geography import (
    resolve_place_names as _resolve_place_names,
)
from .geography import (
    validate_points_within_boundaries as _validate_points_within_boundaries,
)
from .mapping import FieldObservation, TransformSpec, suggest_role_mappings
from .mcp_contracts import ResourceRef
from .mcp_resources import McpResourceRuntime, bind_runtime
from .models import (
    MCP_SERVER_NAME,
    City,
    CityDemand,
    CityLane,
    ConfirmedSourceDecision,
    DataAgentResourceToolResult,
    Facility,
    GeographyOverride,
    PlanningDataset,
    PlanningSource,
    Point,
    PreparedNetworkResource,
    ServicePolicy,
    ValidationResult,
    WarehouseCityCoverage,
)
from .network_models import NormalizedInputBatch
from .normalization import (
    ConfirmedFieldMapping,
    ConfirmedSourceRows,
    normalize_confirmed_rows,
)
from .resource_store import PublishedResource, ResourceStore
from .workspace_intake import (
    discover,
    flatten_record,
    inspect,
    read_json_document,
    read_rows,
    workspace_source_metadata,
)

RESOURCE_URI_PREFIX = "supply-chain://resources/"
MAX_SOURCE_CATALOG_ENTRIES = 500
SANDBOX_STATE_META_CAPABILITY = "codex/sandbox-state-meta"


class _SourceProfileResource(BaseModel):
    model_config = ConfigDict(extra="forbid", populate_by_name=True)

    schema_version: Literal["source_profile.v1"] = Field(
        default="source_profile.v1",
        alias="schemaVersion",
    )
    sources: list[dict[str, Any]] = Field(min_length=1, max_length=500)

mcp = FastMCP(
    "Supply Chain Data",
    instructions=(
        "This is a read-only enterprise data boundary for the supply-chain Data Agent. "
        "Inputs are validated Workspace-relative paths resolved under the trusted Turn Workspace; "
        "never request or accept organization IDs, Profile IDs, credentials, arbitrary SQL, "
        "filesystem paths, or write statements. Discover and inspect the complete authorized "
        "Workspace before confirming mappings. Inspection returns exact record counts plus a "
        "head preview; preview rows are examples only and never the full source. Never use "
        "the preview row count as the source row count. The normalization tool rereads the "
        "complete source files and publishes only the entities required by the current "
        "question as normalized_network_input.v1. Copy every "
        "returned Resource reference unchanged. Validate the Resource before handing it to "
        "the Network Planning Agent. Do not paste unbounded source rows into messages and do "
        "not choose a warehouse-network solution."
    ),
    json_response=True,
)

_workspace_root = Path.cwd().resolve()
_profile_state_root = Path(os.environ.get("CODEX_HOME", _workspace_root / ".codex")).resolve()
_mcp_resource_runtime: McpResourceRuntime | None = None
_CONTRACT_PATH = (
    Path(__file__).resolve().parents[1] / "contracts" / "warehouse-network-planning-1.0.0.json"
)


def _store() -> ResourceStore:
    return _runtime().store


def _runtime() -> McpResourceRuntime:
    global _mcp_resource_runtime
    if _mcp_resource_runtime is None:
        _mcp_resource_runtime = bind_runtime(
            _workspace_root,
            _profile_state_root,
            MCP_SERVER_NAME,
            RESOURCE_URI_PREFIX,
        )
    return _mcp_resource_runtime


@mcp.resource(
    "supply-chain://resources/{resource_id}",
    name="supply_chain_resource",
    title="Supply-chain data Resource",
    mime_type="application/json",
)
def read_data_resource(resource_id: str) -> str:
    """Read one immutable data Resource by its opaque Resource name."""
    return _runtime().read(resource_id)


def _resource_link(
    published: PublishedResource,
    description: str,
) -> ResourceLink:
    return ResourceLink(
        type="resource_link",
        name=published.resource_id,
        title=published.schema,
        uri=published.uri,
        description=description,
        mimeType="application/json",
        size=published.size,
    )


_INTAKE_SCHEMAS = {
    "data_requirement_profile.v2",
    "source_profile.v1",
    "mapping_proposal.v1",
    "input_gap.v1",
    "planning-dataset.v2",
    "analysis_readiness_review.v1",
}


def _bounded_intake_envelope(
    published: PublishedResource,
    payload: dict[str, Any],
) -> EmbeddedResource | None:
    """Return a bounded companion for a large intake Resource.

    The full Resource remains the only dataset authority.  This companion is
    deliberately limited to fields the Platform may project into readiness;
    it is never used as a planning input.
    """
    if published.schema not in _INTAKE_SCHEMAS or published.size <= 128 * 1024:
        return None
    encoded = json.dumps(
        payload,
        ensure_ascii=False,
        sort_keys=True,
        separators=(",", ":"),
    ).encode("utf-8")
    keep = {
        "schemaVersion": payload.get("schemaVersion"),
        "contract": payload.get("contract"),
        "taskEvidence": payload.get("taskEvidence"),
        "envelopeSchemaVersion": "intake_evidence_envelope.v1",
        "resourceUri": published.uri,
        "resourceSchema": published.schema,
        "resourceContentSha256": hashlib.sha256(encoded).hexdigest(),
    }
    for key in (
        "taskGoal",
        "problemType",
        "entities",
        "requiredEntities",
        "parameters",
        "outputs",
        "conditionalRequirements",
        "assumptions",
        "exclusions",
        "inputRequest",
        "sources",
        "candidates",
        "gaps",
        "ready",
        "summary",
        "checks",
        "plannedAnalysis",
        "limitations",
        "normalization_status",
        "data_quality",
        "source_summary",
        "normalization_statistics",
        "rejections",
    ):
        if key in payload:
            keep[key] = payload[key]
    envelope = json.dumps(keep, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
    if len(envelope.encode("utf-8")) > 128 * 1024:
        # Preserve the schema-level readiness signal even when a source profile
        # or mapping contains more candidates than the platform projection cap.
        for key in (
            "sources",
            "candidates",
            "entities",
            "requiredEntities",
            "checks",
            "limitations",
        ):
            if isinstance(keep.get(key), list):
                keep[key] = keep[key][:128]
        envelope = json.dumps(keep, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
    if len(envelope.encode("utf-8")) > 128 * 1024:
        raise ValueError("intake_evidence_envelope_exceeds_platform_limit")
    return EmbeddedResource(
        type="resource",
        resource=TextResourceContents(
            uri=published.uri,
            mimeType="application/json",
            text=envelope,
        ),
    )


def _workspace(ctx: Context) -> Path:
    return _runtime().require_workspace(ctx)


def _publish_json(
    schema: str,
    payload: BaseModel | dict[str, Any],
    summary: str,
) -> CallToolResult:
    return _runtime().publish(schema, payload, summary)


def _intake_contract_metadata() -> dict[str, str]:
    contract = json.loads(_CONTRACT_PATH.read_text(encoding="utf-8"))
    return {
        "id": str(contract["contractId"]),
        "version": str(contract["version"]),
        "contentHash": str(contract["contentSha256"]),
    }


def _wrap_intake_payload(
    schema: str,
    payload: dict[str, Any],
    *,
    profile_hash: str | None = None,
    source_hash: str | None = None,
    mapping_hash: str | None = None,
    parameter_hash: str | None = None,
) -> dict[str, Any]:
    return {
        "schemaVersion": schema,
        "contract": _intake_contract_metadata(),
        "taskEvidence": {
            "profileHash": profile_hash,
            "sourceSnapshotHash": source_hash,
            "mappingHash": mapping_hash,
            "parameterHash": parameter_hash,
        },
        **payload,
    }


@mcp.tool(structured_output=True)
def discover_workspace_sources(ctx: Context) -> dict[str, Any]:
    """Discover bounded Excel/CSV/JSON metadata across the trusted Workspace."""
    root = _workspace(ctx)
    sources = discover(root)
    return {
        "schema": "workspace_source_catalog.v1",
        "workspace_scope": "authorized_workspace",
        "sources": sources,
        "truncated": False,
        **workspace_source_metadata(root),
    }


@mcp.tool(structured_output=True)
def inspect_workspace_sources(
    relative_paths: list[str],
    ctx: Context,
) -> Annotated[CallToolResult, DataAgentResourceToolResult]:
    """Inspect selected Workspace files and publish one typed source profile.

    Preview rows are examples for schema inspection only. They are never a
    complete source snapshot and must not be used as the source row count.
    """
    profile = _inspect_workspace_sources(relative_paths, ctx)
    summary = f"Inspected {len(profile['sources'])} authorized Workspace sources."
    return _publish_json("source_profile.v1", profile, summary)


def _inspect_workspace_sources(
    relative_paths: list[str],
    ctx: Context,
) -> dict[str, Any]:
    if not relative_paths or len(relative_paths) > MAX_SOURCE_CATALOG_ENTRIES:
        raise ValueError("relative_paths must contain 1-500 Workspace-relative paths")
    if len(set(relative_paths)) != len(relative_paths):
        raise ValueError("relative_paths must not contain duplicates")
    sources = [inspect(_workspace(ctx), relative_path) for relative_path in relative_paths]
    for source in sources:
        source["mapping_suggestions"] = _mapping_suggestions(source["structure"])
    return _bound_agent_previews(
        {
            "schemaVersion": "source_profile.v1",
            "sources": sources,
        }
    )


def _mapping_suggestions(structure: dict[str, Any]) -> list[dict[str, Any]]:
    observations: list[FieldObservation] = []

    def add(columns: list[Any], rows: list[Any]) -> None:
        for index, column in enumerate(columns):
            name = str(column).strip()
            if not name:
                continue
            samples = tuple(
                str(row[index])[:256]
                for row in rows[:3]
                if isinstance(row, list) and index < len(row) and row[index] not in (None, "")
            )
            observations.append(FieldObservation(name=name, sample_values=samples))

    kind = structure.get("kind")
    if kind == "table":
        add(structure.get("columns", []), structure.get("preview", {}).get("rows", []))
    elif kind == "workbook":
        for sheet in structure.get("sheets", []):
            add(sheet.get("columns", []), sheet.get("preview", {}).get("rows", []))
    elif kind == "json":
        for array in structure.get("arrays", []):
            values: dict[str, list[str]] = {}
            for item in array.get("preview", {}).get("rows", []):
                for name, field in item.get("fields", {}).items():
                    sample = field.get("sample") if isinstance(field, dict) else None
                    if sample not in (None, ""):
                        values.setdefault(str(name), []).append(str(sample)[:256])
            observations.extend(
                FieldObservation(name=name, sample_values=tuple(samples[:3]))
                for name, samples in values.items()
            )
    return [
        {
            "role": suggestion.role.value,
            "confidence": suggestion.confidence,
            "ambiguous": suggestion.ambiguous,
            "field_mappings": [
                {
                    "target_field": mapping.target_field,
                    "source_fields": list(mapping.source_fields),
                    "transform": mapping.transform.model_dump(mode="json"),
                    "score": mapping.score,
                    "reason_code": mapping.reason_code,
                }
                for mapping in suggestion.field_mappings
            ],
        }
        for suggestion in suggest_role_mappings(observations)
    ]


def _bound_agent_previews(profile: dict[str, Any]) -> dict[str, Any]:
    """Keep structural evidence while preventing sample rows from entering context."""

    def trim(value: Any) -> Any:
        if isinstance(value, dict):
            result = {key: trim(item) for key, item in value.items()}
            preview = result.get("preview")
            if isinstance(preview, dict) and isinstance(preview.get("rows"), list):
                preview["rows"] = preview["rows"][:3]
                preview["returned_count"] = len(preview["rows"])
                preview["limit"] = min(int(preview.get("limit", 3)), 3)
            return result
        if isinstance(value, list):
            return [trim(item) for item in value]
        return value

    return trim(profile)


def _load_source_profile(resource_ref: ResourceRef) -> dict[str, Any]:
    """Load and validate the canonical source_profile.v1 Resource.

    Workspace source references and MCP Resource references are different
    contracts.  This function accepts only the latter and never attempts to
    resolve a Workspace path from a model-provided value.
    """
    profile = _runtime().load_model(
        resource_ref,
        "source_profile.v1",
        _SourceProfileResource,
    ).model_dump(mode="json", by_alias=True)
    sources = profile.get("sources")
    if not isinstance(sources, list) or not sources:
        raise ValueError("source_profile_sources_missing")
    for index, source in enumerate(sources):
        if not isinstance(source, dict):
            raise ValueError(f"source_profile_source_invalid:{index}")
        structure = source.get("structure")
        if not isinstance(structure, dict) or not structure.get("kind"):
            raise ValueError(f"source_profile_structure_missing:{index}")
    return profile


@mcp.tool(structured_output=True)
def normalize_network_input(
    source_profile_ref: ResourceRef,
    confirmed_sources: list[ConfirmedSourceDecision],
    country_code: str,
    ctx: Context,
) -> Annotated[CallToolResult, DataAgentResourceToolResult]:
    """Normalize exact Workspace files using only explicit confirmed decisions."""
    profile = _load_source_profile(source_profile_ref)
    country = country_code.strip().upper()
    if not re.fullmatch(r"[A-Z]{2,3}", country):
        raise ValueError("country_code_required_iso_alpha2_or_alpha3")
    available = {str(source["relative_path"]): source for source in profile.get("sources", [])}
    if not confirmed_sources or len(confirmed_sources) > len(available):
        raise ValueError("confirmed_sources_must_select_profile_sources")
    selected_paths = [decision.relative_path for decision in confirmed_sources]
    if len(set(selected_paths)) != len(selected_paths):
        raise ValueError("confirmed_source_relative_paths_must_be_unique")
    if not set(selected_paths) <= set(available):
        raise ValueError("confirmed_source_not_in_profile")
    normalized_sources = []
    for decision in confirmed_sources:
        mappings = [
            ConfirmedFieldMapping(
                target_field=mapping.target_field,
                source_field=mapping.source_field,
                transform=TransformSpec(
                    kind=mapping.transform,
                    factor=mapping.factor,
                ),
            )
            for mapping in decision.mappings
        ]
        normalized_sources.append(
            ConfirmedSourceRows(
                role=decision.role,
                rows=read_rows(_workspace(ctx), decision.relative_path),
                mappings=mappings,
            )
        )
    state, batch = normalize_confirmed_rows(normalized_sources)
    payload = PreparedNetworkResource(
        country_code=country,
        state=state,
        **batch.model_dump(mode="json"),
    )
    return _publish_json(
        "normalized_network_input.v1",
        payload,
        f"Normalized {len(confirmed_sources)} confirmed Workspace sources; state is {state}.",
    )


@mcp.tool(structured_output=True)
def prepare_network_geography(
    normalized_input_ref: ResourceRef,
    administrative_catalog_relative_path: str,
    admin_level: str,
    ctx: Context,
    overrides: list[GeographyOverride] | None = None,
) -> Annotated[CallToolResult, DataAgentResourceToolResult]:
    """Enrich normalized records from one validated administrative catalog."""
    payload = _runtime().load_model(
        normalized_input_ref,
        "normalized_network_input.v1",
        PreparedNetworkResource,
    )
    country_code = payload.country_code
    batch = NormalizedInputBatch.model_validate(
        payload.model_dump(
            include={
                "demand_cities",
                "warehouses",
                "current_assignments",
                "route_quotes",
                "issues",
            }
        )
    )
    catalog = _load_administrative_catalog(
        country_code,
        admin_level,
        read_json_document(_workspace(ctx), administrative_catalog_relative_path),
    )
    override_map = {
        (override.entity, override.entity_id): override.catalog_city_id
        for override in overrides or []
    }
    if len(override_map) != len(overrides or []):
        raise ValueError("geography_overrides_must_be_unique")
    demands, warehouses, _candidates, issues = enrich_network_geography(
        batch.demand_cities,
        batch.warehouses,
        catalog,
        overrides=override_map,
    )
    prepared = batch.model_copy(
        update={
            "demand_cities": demands,
            "warehouses": warehouses,
            "issues": [*batch.issues, *issues],
        }
    )
    has_missing_coordinates = any(
        item.longitude is None or item.latitude is None
        for item in [*prepared.demand_cities, *prepared.warehouses]
    )
    state = (
        "needs_input"
        if any(issue.severity == "error" for issue in prepared.issues)
        else "needs_geography"
        if has_missing_coordinates
        else "ready"
    )
    return _publish_json(
        "normalized_network_input.v1",
        PreparedNetworkResource(
            country_code=country_code,
            state=state,
            **prepared.model_dump(mode="json"),
        ),
        f"Prepared network geography; state is {state}.",
    )


def validate_normalized_network_input(
    normalized_input_ref: ResourceRef,
    requested_analysis: str,
) -> dict[str, Any]:
    """Validate only the data needed by the requested analysis."""
    if normalized_input_ref.resource_schema != "normalized_network_input.v1":
        raise ValueError("normalized_input_ref_must_be_normalized_network_input_v1")
    payload = _store().load_uri(normalized_input_ref.uri)
    quality = payload.get("quality") or {}
    errors = [issue for issue in quality.get("issues", []) if issue.get("severity") == "error"]
    if requested_analysis in {"current_coverage", "current_cost"} and not payload.get(
        "current_assignments"
    ):
        return {
            "schema": "data_quality_report.v1",
            "state": "needs_input",
            "issues": [
                {
                    "code": "current_assignment_missing",
                    "severity": "warning",
                    "business_message": "没有提供当前覆盖关系，将改为计算已有仓范围内的优化基线。",
                }
            ],
        }
    return {
        "schema": "data_quality_report.v1",
        "state": "failed" if errors else "ready",
        "issues": errors,
        "requested_analysis": requested_analysis,
    }


def load_administrative_catalog(
    country_code: str,
    admin_level: str,
    source_ref: str,
    ctx: Context,
) -> Annotated[CallToolResult, DataAgentResourceToolResult]:
    """Load a country administrative catalog from an authorized JSON source."""
    payload = read_json_document(_workspace(ctx), source_ref)
    catalog = _load_administrative_catalog(country_code, admin_level, payload)
    return _publish_json(
        "administrative_catalog.v1",
        catalog,
        f"Loaded the {country_code.upper()} {admin_level} administrative catalog.",
    )


def resolve_place_names(
    rows_ref: ResourceRef,
    admin_catalog_ref: ResourceRef,
) -> Annotated[CallToolResult, DataAgentResourceToolResult]:
    """Resolve city identifiers and names without guessing ambiguous matches."""
    rows_payload = _store().load_uri(rows_ref.uri)
    catalog_payload = _store().load_uri(admin_catalog_ref.uri)
    rows = rows_payload.get("rows") or rows_payload.get("records") or []
    result = _resolve_place_names(rows, catalog_payload)
    return _publish_json(
        "place_resolution.v1", result, "Resolved administrative names and reported ambiguities."
    )


def build_administrative_candidates(
    admin_catalog_ref: ResourceRef,
    level: str,
) -> Annotated[CallToolResult, DataAgentResourceToolResult]:
    """Create province- or city-level candidate warehouse records."""
    catalog = _store().load_uri(admin_catalog_ref.uri)
    result = _build_administrative_candidates(catalog, level)
    return _publish_json(
        "warehouse_candidates.v1", result, "Built administrative candidate warehouse locations."
    )


def validate_points_within_boundaries(
    points_source_ref: str,
    boundary_ref: ResourceRef,
    ctx: Context,
) -> dict[str, Any]:
    """Validate Workspace point records against a published boundary Resource."""
    points = read_rows(_workspace(ctx), points_source_ref)
    boundary_payload = _store().load_uri(boundary_ref.uri)
    return _validate_points_within_boundaries(points, boundary_payload)


def normalize_planning_dataset(
    source_refs: list[str],
    requirement_profile: dict[str, Any],
    confirmed_mapping: dict[str, Any],
    confirmed_parameters: dict[str, Any],
    ctx: Context,
) -> Annotated[CallToolResult, DataAgentResourceToolResult]:
    """Publish a planning-dataset.v2 from Workspace source refs.

    The model supplies only opaque refs, the published requirement profile and
    confirmation snapshots for the mapping and parameters. It cannot
    pass raw rows or a fabricated normalized dataset through this boundary.
    """
    profile_payload = requirement_profile
    if (
        not isinstance(profile_payload, dict)
        or profile_payload.get("schemaVersion") != "data_requirement_profile.v2"
    ):
        raise ValueError("requirement_profile must contain data_requirement_profile.v2")
    if confirmed_mapping.get("confirmed") is not True:
        raise ValueError("planning dataset normalization requires confirmed mapping")
    if confirmed_parameters.get("confirmed") is not True:
        raise ValueError("planning dataset normalization requires confirmed parameters")
    if not source_refs or len(source_refs) > MAX_SOURCE_CATALOG_ENTRIES:
        raise ValueError("source_refs must contain 1-100 opaque source references")
    source_profile = _inspect_workspace_sources(source_refs, ctx)
    mapping_items = confirmed_mapping.get("mappings") or confirmed_mapping.get("candidates") or []
    if not isinstance(mapping_items, list) or not mapping_items:
        raise ValueError("confirmed_mapping must contain a non-empty mappings list")
    source = _build_planning_source(
        _workspace(ctx), source_refs, mapping_items, confirmed_parameters.get("answers") or []
    )
    dataset = aggregate_planning_dataset(source)
    payload = dataset.model_dump(mode="json", by_alias=True, exclude_none=True)
    source_metadata = workspace_source_metadata(_workspace(ctx))
    payload["dataClassification"] = source_metadata["dataClassification"]
    if "demoTemplate" in source_metadata:
        payload["demoTemplate"] = source_metadata["demoTemplate"]
    payload["normalization"] = {
        "mapping": confirmed_mapping,
        "parameters": confirmed_parameters,
        "source_profile": source_profile,
    }
    payload = _wrap_intake_payload(
        "planning-dataset.v2",
        payload,
        source_hash=hashlib.sha256(
            json.dumps(source_profile, sort_keys=True, separators=(",", ":")).encode()
        ).hexdigest(),
        mapping_hash=hashlib.sha256(
            json.dumps(confirmed_mapping, sort_keys=True, separators=(",", ":")).encode()
        ).hexdigest(),
        parameter_hash=hashlib.sha256(
            json.dumps(confirmed_parameters, sort_keys=True, separators=(",", ":")).encode()
        ).hexdigest(),
    )
    payload["normalization"]["profile"] = profile_payload
    payload["normalization_status"] = "ready"
    summary = (
        f"Normalized {dataset.source_summary.city_demand_row_count} city-demand rows, "
        f"{dataset.source_summary.facility_count} facilities and "
        f"{dataset.source_summary.lane_count} city lanes into planning-dataset.v2."
    )
    return _publish_json("planning-dataset.v2", payload, summary)


def _build_planning_source(
    root: Path,
    relative_paths: list[str],
    mapping_items: list[dict[str, Any]],
    answers: list[dict[str, Any]],
) -> PlanningSource:
    params = {str(item.get("name")): item.get("value") for item in answers}
    planning_mode = str(params.get("planning_mode") or "").strip().lower()
    if planning_mode != "candidate_warehouse_optimization":
        raise ValueError("planning_mode_must_be_candidate_warehouse_optimization")
    if not _text(params.get("cost_scope"), None):
        raise ValueError("cost_scope_parameter_required")
    market = _text(params.get("market"), None)
    if not market or not re.fullmatch(r"[A-Za-z]{2}", market):
        raise ValueError("market_parameter_must_be_iso_code")
    target_sla_hours = _number(params.get("target_sla_hours"))
    coverage_target = _number(params.get("coverage_target"))
    if target_sla_hours <= 0 or not 0 <= coverage_target <= 100:
        raise ValueError("service_target_parameters_out_of_range")
    grouped: dict[str, list[dict[str, Any]]] = {}
    grouped_by_key: dict[str, dict[str, dict[str, Any]]] = {}
    for relative_path in relative_paths:
        for row_index, row in enumerate(read_rows(root, relative_path)):
            flat = flatten_record(row)
            for item in mapping_items:
                if item.get("relative_path") != relative_path:
                    continue
                field = str(item.get("source_field", "")).strip()
                value = flat.get(field)
                if value in (None, ""):
                    continue
                entity_name = str(item.get("target_entity", "")).strip()
                target_field = str(item.get("target_field", "")).strip()
                key = f"{relative_path}:{row.get('__sheet_name', '')}:{row_index}:{entity_name}"
                entity_index = grouped_by_key.setdefault(entity_name, {})
                entity = entity_index.get(key)
                if entity is None:
                    entity = {"_key": key}
                    entity_index[key] = entity
                    grouped.setdefault(entity_name, []).append(entity)
                entity[target_field] = value

    cities = []
    for row in grouped.get("City", []):
        city_id = _text(row.get("city_id"), None)
        city_name = _text(row.get("name"), None)
        region = _text(row.get("region"), None)
        if not city_id or not city_name or not region:
            raise ValueError("planning_dataset_city_fields_incomplete")
        cities.append(
            City(
                city_id=city_id,
                name=city_name,
                region=region,
                location=Point(
                    latitude=_number(row.get("latitude")),
                    longitude=_number(row.get("longitude")),
                ),
            )
        )
    if not cities:
        raise ValueError("planning_dataset_missing_cities")
    city_by_id = {city.city_id: city for city in cities}

    city_demands = []
    for row in grouped.get("CityDemand", []):
        city_id = _text(row.get("city_id"), None)
        if not city_id or city_id not in city_by_id:
            raise ValueError("planning_dataset_city_demand_fields_incomplete")
        city_demands.append(
            CityDemand(
                city_id=city_id,
                demand_date=_date(row.get("date")),
                demand_units=_integer(row.get("quantity"), positive=True),
            )
        )
    if not city_demands:
        raise ValueError("planning_dataset_missing_city_demand")
    planning_period = _derived_planning_period(item.demand_date for item in city_demands)

    facilities = []
    for row in grouped.get("Facility", []):
        facility_id = _text(row.get("facility_id"), None)
        facility_name = _text(row.get("name"), None)
        city_id = _text(row.get("city_id"), None)
        status_value = _text(row.get("existing_or_candidate"), None)
        if (
            not facility_id
            or not facility_name
            or not city_id
            or city_id not in city_by_id
            or not status_value
        ):
            raise ValueError("planning_dataset_facility_fields_incomplete")
        status = status_value.lower()
        if status not in {"existing", "current", "现有", "已有", "candidate", "候选"}:
            raise ValueError("planning_dataset_facility_status_invalid")
        facilities.append(
            Facility(
                facility_id=facility_id,
                city_id=city_id,
                label=facility_name,
                location=city_by_id[city_id].location,
                capacity_units=_integer(row.get("capacity")),
                is_existing=status in {"existing", "current", "现有", "已有"},
                fixed_cost=_money(row.get("fixed_cost")),
                opening_cost=_money(row.get("opening_cost")),
                handling_cost_per_unit=_money(row.get("handling_cost")),
            )
        )
    if not facilities or not any(item.is_existing for item in facilities):
        raise ValueError("planning_dataset_requires_existing_facility")
    if not any(not item.is_existing for item in facilities):
        raise ValueError("planning_dataset_requires_candidate_facility")
    facility_ids = {item.facility_id for item in facilities}
    coverage = []
    for row in grouped.get("Coverage", []):
        facility_id = _text(row.get("facility_id"), None)
        city_id = _text(row.get("city_id"), None)
        if (
            not facility_id
            or not city_id
            or facility_id not in facility_ids
            or city_id not in city_by_id
        ):
            raise ValueError("planning_dataset_coverage_relation_invalid")
        coverage.append(
            WarehouseCityCoverage(
                facility_id=facility_id,
                city_id=city_id,
                is_current=str(row.get("is_current", "true")).lower() in {"true", "1", "yes"},
            )
        )
    if not coverage:
        raise ValueError("planning_dataset_current_coverage_required")

    lanes = []
    currencies: set[str] = set()
    for row in grouped.get("Lane", []):
        origin_city_id = _text(row.get("origin_city_id"), None)
        destination_city_id = _text(row.get("destination_city_id"), None)
        rate_currency = _text(row.get("currency"), None)
        if not origin_city_id or not destination_city_id or not rate_currency:
            raise ValueError("planning_dataset_city_lane_fields_incomplete")
        normalized_currency = rate_currency.upper()
        if not re.fullmatch(r"[A-Z]{3}", normalized_currency):
            raise ValueError("planning_dataset_city_lane_currency_invalid")
        currencies.add(normalized_currency)
        lanes.append(
            CityLane(
                origin_city_id=origin_city_id,
                destination_city_id=destination_city_id,
                distance_km=_number(row.get("distance_km")),
                travel_time_hours=_number(row.get("travel_time_hours")),
                base_cost_per_unit=_money(row.get("base_cost_per_unit")),
                distance_cost_per_km_per_unit=_money(row.get("distance_cost_per_km_per_unit")),
                currency=normalized_currency,
            )
        )
    if not lanes:
        raise ValueError("planning_dataset_missing_city_lanes")
    if len(currencies) != 1:
        raise ValueError("planning_dataset_city_lane_currency_mismatch")
    currency = currencies.pop()
    source_metadata = workspace_source_metadata(root)
    source = PlanningSource(
        source_id="workspace-confirmed",
        market=market.upper(),
        label="Workspace-confirmed warehouse network planning dataset",
        source_updated_at=datetime.now(UTC),
        planning_period=planning_period,
        currency=currency.upper(),
        service_policy=ServicePolicy(
            policy_id="user-confirmed-target-sla",
            max_delivery_seconds=int(target_sla_hours * 3600),
        ),
        cities=cities,
        city_demands=city_demands,
        facilities=facilities,
        warehouse_city_coverage=coverage,
        lanes=lanes,
        route_provider="workspace-quoted-lanes",
        route_method="quoted",
        dataClassification=source_metadata["dataClassification"],
        demoTemplate=source_metadata.get("demoTemplate"),
    )
    return source


def _derived_planning_period(dates: Any) -> str:
    values = list(dates)
    if not values:
        raise ValueError("planning_dataset_missing_demand_dates")
    start, end = min(values), max(values)
    if start.year == end.year and start.month == end.month:
        return f"{start.year:04d}-{start.month:02d}"
    return f"{start.isoformat()} to {end.isoformat()}"


def _text(value: Any, default: str | None) -> str | None:
    if value is None:
        return default
    text = str(value).strip()
    return text or default


def _number(value: Any, default: float | None = None) -> float:
    if value in (None, ""):
        if default is not None:
            return default
        raise ValueError("numeric_value_required")
    try:
        number = float(str(value).replace(",", ""))
    except (TypeError, ValueError) as error:
        raise ValueError("invalid_numeric_value") from error
    if not math.isfinite(number):
        raise ValueError("numeric_value_must_be_finite")
    return number


def _integer(value: Any, scale: float = 1.0, positive: bool = False) -> int:
    number = _number(value)
    if (positive and number <= 0) or number < 0:
        raise ValueError("numeric_value_must_be_non_negative")
    return int(round(number * scale))


def _money(value: Any) -> Decimal:
    if value in (None, ""):
        raise ValueError("money_value_required")
    try:
        return Decimal(str(value).replace(",", ""))
    except (InvalidOperation, ValueError) as error:
        raise ValueError("invalid_money_value") from error


def _date(value: Any) -> date:
    if isinstance(value, datetime):
        return value.date()
    if isinstance(value, date):
        return value
    text = str(value).strip()
    for fmt in ("%Y-%m-%d", "%Y/%m/%d", "%d/%m/%Y", "%Y-%m"):
        try:
            return datetime.strptime(text, fmt).date()
        except ValueError:
            continue
    raise ValueError("invalid_date_value")


def validate_planning_dataset(
    resource_ref: ResourceRef,
) -> ValidationResult:
    """Validate planning-dataset structure, totals, handoff projection, and quality state."""
    if resource_ref.server != MCP_SERVER_NAME:
        raise ValueError(f"resource_ref.server must be {MCP_SERVER_NAME}")
    if resource_ref.resource_schema != "planning-dataset.v2":
        raise ValueError("resource_ref must identify planning-dataset.v2")
    raw = _store().load_uri(resource_ref.uri)
    if raw.get("normalization_status") == "mapped_candidate":
        records = raw.get("records", [])
        return ValidationResult(
            valid=False,
            resource_schema="planning-dataset.v2",
            errors=[
                "mapped candidate requires Network Agent entity, unit, relationship "
                "and route validation",
            ]
            if records
            else ["no confirmed source fields produced normalized records"],
            warnings=[
                "planning-dataset.v2 contains bounded mapped records, not a silently "
                "completed model"
            ],
            checks=[
                "source refs and the published requirement profile are present",
                f"mapped record count: {len(records)}",
            ],
        )
    if raw.get("schemaVersion") != "planning-dataset.v2":
        return ValidationResult(
            valid=False,
            resource_schema="planning-dataset.v2",
            errors=["planning-dataset.v2 is missing its bounded provenance envelope"],
            warnings=[],
            checks=[],
        )
    dataset = PlanningDataset.model_validate(raw)
    errors: list[str] = []
    warnings = list(dataset.data_quality.warnings)
    checks: list[str] = ["resource conforms to planning-dataset.v2"]

    normalization = raw.get("normalization")
    if not isinstance(normalization, dict):
        errors.append("normalization provenance is missing")
    else:
        profile = normalization.get("profile")
        mapping_confirmation = normalization.get("mapping")
        parameter_confirmation = normalization.get("parameters")
        if (
            not isinstance(profile, dict)
            or profile.get("schemaVersion") != "data_requirement_profile.v2"
        ):
            errors.append("data requirement profile is missing")
        if (
            not isinstance(mapping_confirmation, dict)
            or mapping_confirmation.get("confirmed") is not True
        ):
            errors.append("mapping confirmation is missing")
        if (
            not isinstance(parameter_confirmation, dict)
            or parameter_confirmation.get("confirmed") is not True
        ):
            errors.append("parameter confirmation is missing")

    distribution_units = sum(item.demand_units for item in dataset.demand_distribution)
    projected_units = sum(item.demand_units for item in dataset.network_input.demand_points)
    if distribution_units != dataset.source_summary.demand_units:
        errors.append("distribution demand units do not match source summary")
    if projected_units != dataset.source_summary.demand_units:
        errors.append("network_input demand units do not match source summary")
    if (
        dataset.delivery_baseline.observed_demand_units
        + dataset.delivery_baseline.unobserved_demand_units
        != dataset.source_summary.demand_units
    ):
        errors.append("delivery baseline units do not match source summary")
    distribution_ids = {item.demand_id for item in dataset.demand_distribution}
    projection_ids = {item.demand_id for item in dataset.network_input.demand_points}
    if distribution_ids != projection_ids:
        errors.append("distribution and network_input demand identifiers differ")
    expected_route_pairs = {
        (facility.city_id, demand.city_id)
        for facility in dataset.network_input.facilities
        for demand in dataset.network_input.demand_points
    }
    route_pairs = {
        (route.origin_city_id, route.destination_city_id) for route in dataset.route_entries
    }
    if route_pairs != expected_route_pairs:
        errors.append("route facts do not cover the network handoff projection")
    if dataset.data_quality.errors:
        errors.extend(dataset.data_quality.errors)
    if not errors:
        checks.extend(
            [
                "demand totals reconcile across source, distribution, and network projection",
                "delivery observed and unobserved units reconcile to total demand",
                "demand identifiers match the network handoff projection",
                "route facts cover every unique origin-city to destination-city lane",
            ]
        )
    return ValidationResult(
        valid=not errors,
        resource_schema=dataset.schema_version,
        errors=errors,
        warnings=warnings,
        checks=checks,
    )


def main() -> None:
    parser = argparse.ArgumentParser(description="Read-only supply-chain data MCP server")
    parser.add_argument("--transport", choices=("stdio",), default="stdio")
    parser.parse_args()

    global _workspace_root, _profile_state_root, _mcp_resource_runtime
    _workspace_root = Path.cwd().resolve(strict=True)
    _profile_state_root = Path(os.environ.get("CODEX_HOME", _workspace_root / ".codex")).resolve()
    _mcp_resource_runtime = bind_runtime(
        _workspace_root,
        _profile_state_root,
        MCP_SERVER_NAME,
        RESOURCE_URI_PREFIX,
    )
    asyncio.run(run_stdio())


async def run_stdio() -> None:
    initialization_options = mcp._mcp_server.create_initialization_options(
        experimental_capabilities={SANDBOX_STATE_META_CAPABILITY: {}},
    )
    async with stdio_server() as streams:
        await mcp._mcp_server.run(streams[0], streams[1], initialization_options)


if __name__ == "__main__":
    main()
