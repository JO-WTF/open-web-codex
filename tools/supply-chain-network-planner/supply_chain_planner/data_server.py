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
from typing import Annotated, Any

from mcp.server.fastmcp import Context, FastMCP
from mcp.server.stdio import stdio_server
from mcp.types import (
    CallToolResult,
    EmbeddedResource,
    ResourceLink,
    TextContent,
    TextResourceContents,
)

from .data_core import build_planning_dataset as aggregate_planning_dataset
from .models import (
    City,
    DataAgentRef,
    DataAgentResourceToolResult,
    CityDemand,
    CityLane,
    Facility,
    PlanningDataset,
    PlanningSource,
    Point,
    ServicePolicy,
    WarehouseCityCoverage,
    ValidationResult,
)
from .resource_store import PublishedResource, ResourceStore
from .workspace_intake import (
    discover,
    flatten_record,
    inspect,
    propose_mapping,
    read_rows,
    trusted_workspace_root,
    workspace_source_metadata,
)

MCP_SERVER_NAME = "supply_chain_data"
RESOURCE_URI_PREFIX = "supply-chain-data://resources/"
MAX_SOURCE_CATALOG_ENTRIES = 500
SANDBOX_STATE_META_CAPABILITY = "codex/sandbox-state-meta"

mcp = FastMCP(
    "Supply Chain Data",
    instructions=(
        "This is a read-only enterprise data boundary for the supply-chain Data Agent. "
        "Inputs are opaque source references resolved under the trusted Turn Workspace; "
        "never request or accept organization IDs, Profile IDs, credentials, arbitrary SQL, "
        "filesystem paths, or write statements. Discover and inspect the complete authorized "
        "Workspace before proposing mappings. Inspection returns exact record counts plus a "
        "head preview; preview rows are examples only and never the full source. Never use "
        "the preview row count as the source row count. The normalization tool rereads the "
        "complete source files and publishes "
        "planning-dataset.v2 as an immutable "
        "MCP Resource with source range, units, row counts, promotion share, delivery baseline, "
        "and data-quality limitations. Copy data_ref unchanged. Validate the Resource before "
        "handing it to the Network Planning Agent. Do not paste unbounded source rows into "
        "messages and do not choose a warehouse-network solution."
    ),
    json_response=True,
)

_workspace_root = Path.cwd().resolve()
_profile_state_root = Path(os.environ.get("CODEX_HOME", _workspace_root / ".codex")).resolve()
_resource_store: ResourceStore | None = None
_CONTRACT_PATH = (
    Path(__file__).resolve().parents[1] / "contracts" / "warehouse-network-planning-1.0.0.json"
)


def _store() -> ResourceStore:
    global _resource_store
    if _resource_store is None:
        resource_root = Path(
            os.environ.get(
                "SUPPLY_CHAIN_DATA_RESOURCE_DIR",
                _profile_state_root / "mcp-state" / "supply-chain-data" / "resources",
            )
        ).resolve()
        _resource_store = ResourceStore(
            resource_root,
            uri_prefix=RESOURCE_URI_PREFIX,
        )
    return _resource_store


@mcp.resource(
    "supply-chain-data://resources/{resource_id}",
    name="supply_chain_planning_dataset",
    title="Supply-chain planning dataset",
    mime_type="application/json",
)
def read_planning_dataset_resource(resource_id: str) -> str:
    """Read a planning dataset previously published by the read-only data MCP."""
    return _store().read(resource_id)


def _data_ref(published: PublishedResource) -> DataAgentRef:
    return DataAgentRef(
        server=MCP_SERVER_NAME,
        uri=published.uri,
        resource_schema=published.schema,
    )


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
    "data_requirement_profile.v1",
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
    return trusted_workspace_root(ctx.request_context.meta)


def _publish_json(schema: str, payload: dict[str, Any], summary: str) -> CallToolResult:
    published = _store().publish(schema, payload)
    content: list[Any] = [
        TextContent(type="text", text=summary),
        _resource_link(published, summary),
    ]
    envelope = _bounded_intake_envelope(published, payload)
    if envelope is not None:
        content.append(envelope)
    return CallToolResult(
        content=content,
        structuredContent={
            "summary": summary,
            "resource_name": published.resource_id,
            "data_ref": _data_ref(published).model_dump(mode="json"),
        },
    )


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
    source_refs: list[str],
    ctx: Context,
) -> dict[str, Any]:
    """Return exact source counts plus explicitly marked head previews.

    Preview rows are examples for schema inspection only. They are never a
    complete source snapshot and must not be used as the source row count.
    """
    if not source_refs or len(source_refs) > MAX_SOURCE_CATALOG_ENTRIES:
        raise ValueError("source_refs must contain 1-100 opaque source references")
    root = _workspace(ctx)
    return {
        "schema": "source_profile.v1",
        "sources": [inspect(root, source_ref) for source_ref in source_refs],
        **workspace_source_metadata(root),
    }


@mcp.tool(structured_output=True)
def publish_source_profile(
    source_refs: list[str],
    ctx: Context,
) -> Annotated[CallToolResult, DataAgentResourceToolResult]:
    """Persist an immutable bounded source_profile.v1 Resource."""
    profile = _wrap_intake_payload(
        "source_profile.v1",
        inspect_workspace_sources(source_refs, ctx),
    )
    summary = f"Profiled {len(profile['sources'])} authorized Workspace sources."
    result = _publish_json("source_profile.v1", profile, summary)
    return result


@mcp.tool(structured_output=True)
def publish_mapping_proposal(
    source_profile_ref: DataAgentRef,
    requirement_profile: dict[str, Any],
) -> Annotated[CallToolResult, DataAgentResourceToolResult]:
    """Publish fuzzy field candidates from the canonical source Profile.

    The model passes only the immutable Resource reference.  Loading the
    Profile here keeps its bounded structures, provenance and source digest
    authoritative instead of asking the model to copy the Profile into a new
    tool argument.
    """
    source_profile = _load_source_profile(source_profile_ref)
    proposal_body = propose_mapping(source_profile["sources"], requirement_profile)
    if not proposal_body["candidates"]:
        raise ValueError("mapping_candidates_empty")
    proposal = _wrap_intake_payload(
        "mapping_proposal.v1",
        proposal_body,
        source_hash=hashlib.sha256(
            json.dumps(source_profile, sort_keys=True, separators=(",", ":")).encode()
        ).hexdigest(),
    )
    summary = (
        f"Generated {len(proposal['candidates'])} mapping candidates. "
        "The complete mapping revision requires user confirmation."
    )
    return _publish_json("mapping_proposal.v1", proposal, summary)


def _load_source_profile(resource_ref: DataAgentRef) -> dict[str, Any]:
    """Load and validate the canonical source_profile.v1 Resource.

    Workspace source references and MCP Resource references are different
    contracts.  This function accepts only the latter and never attempts to
    resolve a Workspace path from a model-provided value.
    """
    if resource_ref.server != MCP_SERVER_NAME:
        raise ValueError("source_profile_ref must identify supply_chain_data")
    if resource_ref.resource_schema != "source_profile.v1":
        raise ValueError("source_profile_ref must identify source_profile.v1")
    profile = _store().load_uri(resource_ref.uri)
    if profile.get("schemaVersion") != "source_profile.v1":
        raise ValueError("source_profile_resource_schema_mismatch")
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
        or profile_payload.get("schemaVersion") != "data_requirement_profile.v1"
    ):
        raise ValueError("requirement_profile must contain data_requirement_profile.v1")
    if confirmed_mapping.get("confirmed") is not True:
        raise ValueError("planning dataset normalization requires confirmed mapping")
    if confirmed_parameters.get("confirmed") is not True:
        raise ValueError("planning dataset normalization requires confirmed parameters")
    if not source_refs or len(source_refs) > MAX_SOURCE_CATALOG_ENTRIES:
        raise ValueError("source_refs must contain 1-100 opaque source references")
    source_profile = inspect_workspace_sources(source_refs, ctx)
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
    source_refs: list[str],
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
    for source_ref in source_refs:
        for row_index, row in enumerate(read_rows(root, source_ref)):
            flat = flatten_record(row)
            for item in mapping_items:
                if item.get("source_ref") != source_ref:
                    continue
                field = str(item.get("source_field", "")).strip()
                value = flat.get(field)
                if value in (None, ""):
                    continue
                entity_name = str(item.get("target_entity", "")).strip()
                target_field = str(item.get("target_field", "")).strip()
                key = f"{source_ref}:{row.get('__sheet_name', '')}:{row_index}:{entity_name}"
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
        if not facility_id or not city_id or facility_id not in facility_ids or city_id not in city_by_id:
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
    source_hash = hashlib.sha256(json.dumps(source_refs, sort_keys=True).encode()).hexdigest()[:16]
    source = PlanningSource(
        source_id=f"workspace-{source_hash}",
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


@mcp.tool(structured_output=True)
def validate_planning_dataset(
    resource_ref: DataAgentRef,
) -> ValidationResult:
    """Validate planning-dataset structure, totals, handoff projection, and quality state."""
    if resource_ref.server != MCP_SERVER_NAME:
        raise ValueError(f"data_ref.server must be {MCP_SERVER_NAME}")
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
            or profile.get("schemaVersion") != "data_requirement_profile.v1"
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
    parser.add_argument(
        "--workspace-root",
        type=Path,
        default=Path.cwd(),
        help="Plugin root used for default fixture and Profile Resource locations",
    )
    parser.add_argument("--transport", choices=("stdio",), default="stdio")
    args = parser.parse_args()

    global _workspace_root, _profile_state_root, _resource_store
    _workspace_root = args.workspace_root.resolve()
    _profile_state_root = Path(os.environ.get("CODEX_HOME", _workspace_root / ".codex")).resolve()
    resource_root = Path(
        os.environ.get(
            "SUPPLY_CHAIN_DATA_RESOURCE_DIR",
            _profile_state_root / "mcp-state" / "supply-chain-data" / "resources",
        )
    ).resolve()
    _resource_store = ResourceStore(
        resource_root,
        uri_prefix=RESOURCE_URI_PREFIX,
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
