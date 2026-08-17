"""Read-only supply-chain data MCP for the Data Agent."""

from __future__ import annotations

import argparse
import asyncio
import os
import re
from pathlib import Path
from typing import Annotated, Any, Literal

from mcp.server.fastmcp import Context, FastMCP
from mcp.server.stdio import stdio_server
from mcp.types import (
    CallToolResult,
    ToolAnnotations,
)
from open_web_codex_provider import (
    McpResourceRuntime,
    ResourceRef,
)
from pydantic import BaseModel, ConfigDict, Field
from supply_chain_planner.data.geography import enrich_network_geography
from supply_chain_planner.data.geography import (
    load_administrative_catalog as _load_administrative_catalog,
)
from supply_chain_planner.data.mapping import (
    FieldObservation,
    SourceRole,
    TransformSpec,
    suggest_role_mappings,
)
from supply_chain_planner.data.normalization import (
    ConfirmedFieldMapping,
    ConfirmedSourceRows,
    NetworkInputValidator,
    normalize_confirmed_rows,
)
from supply_chain_planner.data.workspace_intake import (
    discover,
    inspect,
    read_json_document,
    read_rows,
    workspace_source_metadata,
)
from supply_chain_planner.network.models import NormalizedInputBatch
from supply_chain_planner.shared.models import (
    CandidateWarehouseDeltaRef,
    CandidateWarehouseDeltaResource,
    ConfirmedSourceDecision,
    DataAgentResourceToolResult,
    GeographyOverride,
    PreparedNetworkInputRef,
    PreparedNetworkResource,
)
from supply_chain_planner.shared.resources import SupplyChainResources

RESOURCE_URI_PREFIX = "supply-chain://resources/"
MAX_SOURCE_CATALOG_ENTRIES = 500
SANDBOX_STATE_META_CAPABILITY = "codex/sandbox-state-meta"

READ_ONLY_LOCAL_TOOL = ToolAnnotations(
    readOnlyHint=True,
    destructiveHint=False,
    idempotentHint=True,
    openWorldHint=False,
)
CONTENT_ADDRESSED_RESOURCE_TOOL = ToolAnnotations(
    readOnlyHint=False,
    destructiveHint=False,
    idempotentHint=True,
    openWorldHint=False,
)


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
        "the preview row count as the source row count. The initial normalization tool rereads "
        "the complete explicitly confirmed source files and preserves every confirmed candidate "
        "warehouse in normalized_network_input.v1. Candidate-only changes use the delta and "
        "derive tools; source facts such as demand, existing warehouses, assignments, or route "
        "facts require full normalization. Copy every "
        "returned Resource reference unchanged. Validate the Resource before handing it to "
        "the Network Planning Agent. Do not paste unbounded source rows into messages and do "
        "not choose a warehouse-network solution."
    ),
    json_response=True,
)

_workspace_root = Path.cwd().resolve()
_profile_state_root = Path(os.environ.get("CODEX_HOME", _workspace_root / ".codex")).resolve()
_supply_chain_resources: SupplyChainResources | None = None


def _runtime() -> McpResourceRuntime:
    global _supply_chain_resources
    if _supply_chain_resources is None:
        _supply_chain_resources = SupplyChainResources(_workspace_root, _profile_state_root)
    return _supply_chain_resources.data


@mcp.resource(
    "supply-chain://resources/{resource_id}",
    name="supply_chain_resource",
    title="Supply-chain data Resource",
    mime_type="application/json",
)
def read_data_resource(resource_id: str) -> str:
    """Read one immutable data Resource by its opaque Resource name."""
    return _runtime().read(resource_id)


def _workspace(ctx: Context) -> Path:
    return _runtime().require_workspace(ctx)


def _publish_json(
    schema: str,
    payload: BaseModel | dict[str, Any],
    summary: str,
) -> CallToolResult:
    return _runtime().publish(schema, payload, summary)


@mcp.tool(structured_output=True, annotations=READ_ONLY_LOCAL_TOOL)
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


@mcp.tool(structured_output=True, annotations=CONTENT_ADDRESSED_RESOURCE_TOOL)
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


@mcp.tool(structured_output=True, annotations=CONTENT_ADDRESSED_RESOURCE_TOOL)
def normalize_network_input(
    source_profile_ref: ResourceRef,
    confirmed_sources: list[ConfirmedSourceDecision],
    country_code: Annotated[str, Field(pattern=r"^[A-Za-z]{2}$")],
    ctx: Context,
) -> Annotated[CallToolResult, DataAgentResourceToolResult]:
    """Normalize exact Workspace files using only explicit confirmed decisions."""
    profile = _load_source_profile(source_profile_ref)
    country = country_code.strip().upper()
    if not re.fullmatch(r"[A-Z]{2}", country):
        raise ValueError("country_code_required_iso_alpha2")
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
        f"Normalized {len(confirmed_sources)} confirmed Workspace sources; "
        f"{len(batch.demand_cities)} demand cities, {len(batch.warehouses)} warehouses, "
        f"{len(batch.current_assignments)} current assignments, "
        f"{len(batch.route_quotes)} route quotes, and "
        f"{len(batch.provided_route_facts)} provided route facts; state is {state}.",
    )


@mcp.tool(structured_output=True, annotations=CONTENT_ADDRESSED_RESOURCE_TOOL)
def normalize_candidate_delta(
    source_profile_ref: ResourceRef,
    confirmed_sources: list[ConfirmedSourceDecision],
    removed_candidate_ids: list[str],
    ctx: Context,
) -> Annotated[CallToolResult, DataAgentResourceToolResult]:
    """Normalize an explicit candidate-only change without reparsing base facts."""
    profile = _load_source_profile(source_profile_ref)
    available = {str(source["relative_path"]): source for source in profile.get("sources", [])}
    selected_paths = [decision.relative_path for decision in confirmed_sources]
    if len(set(selected_paths)) != len(selected_paths):
        raise ValueError("confirmed_source_relative_paths_must_be_unique")
    if not set(selected_paths) <= set(available):
        raise ValueError("confirmed_source_not_in_profile")
    if any(decision.role != SourceRole.CANDIDATE_WAREHOUSE for decision in confirmed_sources):
        raise ValueError("candidate_delta_requires_candidate_warehouse_sources")
    normalized_sources: list[ConfirmedSourceRows] = []
    for decision in confirmed_sources:
        mappings = [
            ConfirmedFieldMapping(
                target_field=mapping.target_field,
                source_field=mapping.source_field,
                transform=TransformSpec(kind=mapping.transform, factor=mapping.factor),
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
    _state, batch = normalize_confirmed_rows(normalized_sources)
    candidate_errors = [
        issue
        for issue in batch.issues
        if issue.code.startswith("candidate_warehouse") or issue.code == "warehouse_duplicate"
    ]
    if candidate_errors:
        raise ValueError("candidate_delta_invalid")
    delta = CandidateWarehouseDeltaResource(
        upsertWarehouses=batch.warehouses,
        removeWarehouseIds=removed_candidate_ids,
    )
    return _publish_json(
        "candidate_warehouse_delta.v1",
        delta,
        f"Normalized candidate delta with {len(delta.upsert_warehouses)} upserts and "
        f"{len(delta.remove_warehouse_ids)} removals.",
    )


@mcp.tool(structured_output=True, annotations=CONTENT_ADDRESSED_RESOURCE_TOOL)
def derive_normalized_network_input(
    normalized_input_ref: PreparedNetworkInputRef,
    candidate_delta_ref: CandidateWarehouseDeltaRef,
    ctx: Context,
) -> Annotated[CallToolResult, DataAgentResourceToolResult]:
    """Publish a new immutable input by applying one candidate-only delta."""
    _workspace(ctx)
    base = _runtime().load_model(
        normalized_input_ref,
        "normalized_network_input.v1",
        PreparedNetworkResource,
    )
    if base.state != "ready":
        raise ValueError("candidate_delta_base_must_be_ready")
    delta = _runtime().load_model(
        candidate_delta_ref,
        "candidate_warehouse_delta.v1",
        CandidateWarehouseDeltaResource,
    )
    existing_ids = {
        warehouse.warehouse_id for warehouse in base.warehouses if warehouse.is_existing
    }
    if set(delta.remove_warehouse_ids) & existing_ids:
        raise ValueError("candidate_delta_cannot_remove_existing_warehouse")
    if {warehouse.warehouse_id for warehouse in delta.upsert_warehouses} & existing_ids:
        raise ValueError("candidate_delta_cannot_replace_existing_warehouse")
    warehouse_by_id = {
        warehouse.warehouse_id: warehouse
        for warehouse in base.warehouses
        if warehouse.warehouse_id not in set(delta.remove_warehouse_ids)
    }
    warehouse_by_id.update(
        {warehouse.warehouse_id: warehouse for warehouse in delta.upsert_warehouses}
    )
    batch = NormalizedInputBatch(
        demand_cities=base.demand_cities,
        warehouses=[warehouse_by_id[key] for key in sorted(warehouse_by_id)],
        current_assignments=base.current_assignments,
        route_quotes=base.route_quotes,
        provided_route_facts=base.provided_route_facts,
        issues=[issue for issue in base.issues if issue.severity != "error"],
    )
    issues = NetworkInputValidator().validate(batch)
    has_missing_coordinates = any(
        item.longitude is None or item.latitude is None
        for item in [*batch.demand_cities, *batch.warehouses]
    )
    state = (
        "needs_input"
        if any(issue.severity == "error" for issue in issues)
        else "needs_geography"
        if has_missing_coordinates
        else "ready"
    )
    prepared = PreparedNetworkResource(
        country_code=base.country_code,
        state=state,
        demand_cities=batch.demand_cities,
        warehouses=batch.warehouses,
        current_assignments=batch.current_assignments,
        route_quotes=batch.route_quotes,
        provided_route_facts=batch.provided_route_facts,
        issues=issues,
        parentResourceRef=normalized_input_ref,
    )
    return _publish_json(
        "normalized_network_input.v1",
        prepared,
        f"Derived normalized input from the exact base Resource with "
        f"{len(delta.upsert_warehouses)} candidate upserts and "
        f"{len(delta.remove_warehouse_ids)} candidate removals; state is {state}.",
    )


@mcp.tool(structured_output=True, annotations=CONTENT_ADDRESSED_RESOURCE_TOOL)
def prepare_network_geography(
    normalized_input_ref: ResourceRef,
    administrative_catalog_relative_path: str,
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
                "provided_route_facts",
                "issues",
            }
        )
    )
    catalog_document = read_json_document(
        _workspace(ctx), administrative_catalog_relative_path
    )
    admin_level = catalog_document.get("admin_level")
    if not isinstance(admin_level, str) or not admin_level.strip():
        raise ValueError("administrative_catalog_level_missing")
    catalog = _load_administrative_catalog(
        country_code,
        admin_level.strip(),
        catalog_document,
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
        f"Prepared network geography from the catalog's {admin_level.strip()} level; "
        f"{len(prepared.demand_cities)} demand cities and "
        f"{len(prepared.warehouses)} warehouses; missing-coordinate records "
        f"{sum(item.longitude is None or item.latitude is None for item in [*prepared.demand_cities, *prepared.warehouses])}; "
        f"state is {state}.",
    )


def main() -> None:
    parser = argparse.ArgumentParser(description="Read-only supply-chain data MCP server")
    parser.add_argument("--transport", choices=("stdio",), default="stdio")
    parser.parse_args()

    global _workspace_root, _profile_state_root, _supply_chain_resources
    _workspace_root = Path.cwd().resolve(strict=True)
    _profile_state_root = Path(os.environ.get("CODEX_HOME", _workspace_root / ".codex")).resolve()
    _supply_chain_resources = SupplyChainResources(_workspace_root, _profile_state_root)
    asyncio.run(run_stdio())


async def run_stdio() -> None:
    initialization_options = mcp._mcp_server.create_initialization_options(
        experimental_capabilities={SANDBOX_STATE_META_CAPABILITY: {}},
    )
    async with stdio_server() as streams:
        await mcp._mcp_server.run(streams[0], streams[1], initialization_options)


if __name__ == "__main__":
    main()
