"""Typed supply-chain inspection and preparation MCP for the Data Agent."""

from __future__ import annotations

import argparse
import asyncio
import json
import os
import re
from pathlib import Path
from typing import Annotated, Any

from mcp.server.fastmcp import Context, FastMCP
from mcp.server.stdio import stdio_server
from mcp.types import (
    CallToolResult,
    TextContent,
    ToolAnnotations,
)
from open_web_codex_provider import (
    MAX_WORKSPACE_FILE_BYTES,
    McpResourceRuntime,
    ProviderContractError,
)
from pydantic import Field
from supply_chain_planner.data import workspace_intake as _workspace_intake
from supply_chain_planner.data.geography import enrich_network_geography
from supply_chain_planner.data.geography import (
    load_administrative_catalog as _load_administrative_catalog,
)
from supply_chain_planner.data.mapping import (
    REQUIRED_FIELDS,
    FieldObservation,
    SourceRole,
    SuggestedRoleAssessment,
    TransformSpec,
    assess_role_mappings,
    suggest_role_mappings,
)
from supply_chain_planner.data.normalization import (
    ConfirmedFieldMapping,
    ConfirmedSourceRows,
    normalize_confirmed_rows,
)
from supply_chain_planner.data.workspace_intake import (
    discover,
    inspect,
    read_json_document,
    read_rows,
    source_inspection_identity,
    workspace_source_metadata,
)
from supply_chain_planner.network.models import NormalizedInputBatch
from supply_chain_planner.shared.models import (
    CandidateWarehouseSummary,
    ConfirmedFieldDecision,
    ConfirmedSourceDecision,
    ConfirmedSourceInputDecision,
    DataInspectionInspected,
    DataInspectionSelectionRequired,
    DataInspectionToolResult,
    DataPreparationToolResult,
    DataSourceRequirement,
    GeographyOverride,
    InspectionLimitCounts,
    PreparedNetworkResource,
    SourceInspectionIdentity,
)
from supply_chain_planner.shared.planning_input import load_prepared_network_input
from supply_chain_planner.shared.resources import SupplyChainResources
from supply_chain_planner.shared.workspace_outputs import (
    WorkspaceOutputKind,
    prepare_workspace_output_path,
)

SANDBOX_STATE_META_CAPABILITY = "codex/sandbox-state-meta"

READ_ONLY_LOCAL_TOOL = ToolAnnotations(
    readOnlyHint=True,
    destructiveHint=False,
    idempotentHint=True,
    openWorldHint=False,
)
WORKSPACE_PREPARATION_TOOL = ToolAnnotations(
    readOnlyHint=False,
    destructiveHint=False,
    idempotentHint=False,
    openWorldHint=False,
)


mcp = FastMCP(
    "Supply Chain Data",
    instructions=(
        "This is a typed enterprise data boundary for the supply-chain Data Agent. Inspection "
        "tools are read-only; preparation tools may only create new files under the declared "
        "warehouse-network output directory. "
        "Inputs are validated Workspace-relative paths resolved under the trusted Turn Workspace; "
        "never request or accept organization IDs, Profile IDs, credentials, arbitrary SQL, "
        "filesystem paths, or write statements. Discover and inspect the complete authorized "
        "Workspace before confirming mappings. Inspection returns one bounded inline profile and "
        "workspace_source_inspection.v1 identity; Data does not publish a source Resource. The "
        "head preview has separate "
        "preview_sample_count, total_count, and total_count_exact fields; preview rows are examples "
        "only and never the full source. Never use the preview sample count as the source row count. "
        "Inspection reports independent source-unit role assessments; partial or ambiguous units "
        "do not block inspection. Only the selected sources are validated during preparation. "
        "The initial normalization tool rereads "
        "the complete explicitly confirmed source files and preserves every confirmed candidate "
        "warehouse in a user-visible prepared_network_input.v1 Workspace JSON file. Source facts "
        "such as demand, existing warehouses, assignments, routes, costs, or candidates always "
        "require a complete new preparation. Return the exact Workspace-relative output path and "
        "its content identity to the Network Planning Agent. Do not paste unbounded source rows "
        "into messages and do not choose a warehouse-network solution."
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


def _workspace(ctx: Context) -> Path:
    return _runtime().require_workspace(ctx)


def _write_prepared_input(
    prepared: PreparedNetworkResource,
    output_relative_path: str,
    ctx: Context,
    summary: str,
) -> DataPreparationToolResult:
    output_relative_path = prepare_workspace_output_path(
        _workspace(ctx),
        output_relative_path,
        WorkspaceOutputKind.PREPARED_INPUT,
    )
    created = _runtime().create_workspace_model(
        ctx,
        output_relative_path,
        prepared,
        max_bytes=MAX_WORKSPACE_FILE_BYTES,
    )
    persisted, identity = load_prepared_network_input(
        _workspace(ctx),
        created.relative_path,
    )
    candidates = sorted(
        (warehouse for warehouse in persisted.warehouses if not warehouse.is_existing),
        key=lambda warehouse: warehouse.warehouse_id,
    )
    next_action = {
        "ready": "handoff",
        "needs_input": "request_user_input",
        "needs_geography": "prepare_geography",
    }[persisted.state]
    return DataPreparationToolResult(
        outcome="prepared",
        summary=summary,
        next_action=next_action,
        retryable=False,
        requirements=[],
        prepared_input_relative_path=created.relative_path,
        input_identity=identity,
        state=persisted.state,
        issue_count=len(persisted.issues),
        issues=persisted.issues[:64],
        issues_truncated=len(persisted.issues) > 64,
        candidate_warehouse_count=len(candidates),
        candidate_warehouses=[
            CandidateWarehouseSummary(
                warehouse_id=warehouse.warehouse_id,
                warehouse_name=warehouse.warehouse_name,
                city_name=warehouse.city_name,
            )
            for warehouse in candidates[:64]
        ],
        candidate_warehouses_truncated=len(candidates) > 64,
    )


def _needs_input_result(
    requirements: list[DataSourceRequirement],
) -> DataPreparationToolResult:
    summary = " ".join(requirement.question for requirement in requirements)
    return DataPreparationToolResult(
        outcome="needs_input",
        summary=(
            f"{summary} Do not retry prepare_network_input until the user supplies the "
            "requested business input or uploads corrected source data."
        ),
        next_action="request_user_input",
        retryable=False,
        requirements=requirements,
        prepared_input_relative_path=None,
        input_identity=None,
        state="needs_input",
        issue_count=0,
        issues=[],
        issues_truncated=False,
        candidate_warehouse_count=0,
        candidate_warehouses=[],
        candidate_warehouses_truncated=False,
    )


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


@mcp.tool(structured_output=True, annotations=READ_ONLY_LOCAL_TOOL)
def inspect_workspace_sources(
    relative_paths: list[str],
    ctx: Context,
) -> Annotated[CallToolResult, DataInspectionToolResult]:
    """Inspect selected Workspace files and return one bounded inline profile.

    Preview rows are examples for schema inspection only. They are never a
    complete source snapshot and must not be used as the source row count.
    """
    inspected = _inspect_workspace_sources(relative_paths, ctx)
    if isinstance(inspected, DataInspectionSelectionRequired):
        summary = inspected.summary
        result = DataInspectionToolResult(root=inspected)
        return CallToolResult(
            content=[TextContent(type="text", text=summary)],
            structuredContent=result.model_dump(mode="json", by_alias=True),
        )
    profile = inspected
    content_sha256, source_count = source_inspection_identity(_workspace(ctx), relative_paths)
    identity = SourceInspectionIdentity(
        content_sha256=content_sha256,
        source_count=source_count,
    )
    summary = (
        f"Inspected {source_count} authorized Workspace sources into "
        f"{sum(len(source.get('units', [])) for source in profile.get('sources', []))} "
        "source units. Select the units required for the planning goal; partial or "
        "ambiguous role assessments are not global blockers."
    )
    result = DataInspectionToolResult(
        root=DataInspectionInspected(
            outcome="inspected",
            schemaVersion="workspace_source_profile.v2",
            summary=summary,
            next_action="confirm_sources",
            retryable=False,
            source_profile=profile,
            inspection_identity=identity,
            inspected_relative_paths=sorted(relative_paths),
        )
    )
    return CallToolResult(
        content=[TextContent(type="text", text=summary)],
        structuredContent=result.model_dump(mode="json", by_alias=True),
    )


def _inspect_workspace_sources(
    relative_paths: list[str],
    ctx: Context,
) -> dict[str, Any] | DataInspectionSelectionRequired:
    if not relative_paths:
        raise ValueError("relative_paths must contain at least one Workspace-relative path")
    if len(relative_paths) > _workspace_intake.MAX_INSPECTION_FILES:
        return _selection_required_result(
            summary="Select fewer Workspace source files before inspecting them.",
            observed=InspectionLimitCounts(
                files=min(len(relative_paths), 500), units=0, bytes=0
            ),
        )
    if len(set(relative_paths)) != len(relative_paths):
        raise ValueError("relative_paths must not contain duplicates")
    sources: list[dict[str, Any]] = []
    unit_count = 0
    for relative_path in relative_paths:
        source = inspect(_workspace(ctx), relative_path)
        structure = source.pop("structure")
        units = _mapping_analysis(str(source["relative_path"]), structure)
        source["units"] = units
        unit_count += len(units)
        if unit_count > _workspace_intake.MAX_INSPECTION_UNITS:
            return _selection_required_result(
                summary="Select files with fewer source units before inspecting them.",
                observed=InspectionLimitCounts(
                    files=len(sources) + 1, units=unit_count, bytes=0
                ),
            )
        sources.append(source)
    profile = _bound_agent_previews(
        {
            "schemaVersion": "workspace_source_profile.v2",
            "sources": sources,
            "source_count": len(sources),
            "unit_count": unit_count,
        }
    )
    profile_bytes = len(json.dumps(profile, ensure_ascii=False, separators=(",", ":")).encode())
    if profile_bytes > _workspace_intake.MAX_INSPECTION_BYTES:
        return _selection_required_result(
            summary="Select fewer Workspace sources; the bounded inspection profile is too large.",
            observed=InspectionLimitCounts(
                files=len(sources), units=unit_count, bytes=profile_bytes
            ),
        )
    return profile


def _selection_required_result(
    *, summary: str, observed: InspectionLimitCounts
) -> DataInspectionSelectionRequired:
    return DataInspectionSelectionRequired(
        outcome="selection_required",
        schemaVersion="workspace_source_profile.v2",
        summary=summary,
        code="inspection_selection_required",
        next_action="select_fewer_sources",
        retryable=False,
        observed=observed,
        limit=InspectionLimitCounts(
            files=_workspace_intake.MAX_INSPECTION_FILES,
            units=_workspace_intake.MAX_INSPECTION_UNITS,
            bytes=_workspace_intake.MAX_INSPECTION_BYTES,
        ),
    )


def _unit_definitions(structure: dict[str, Any]) -> list[dict[str, Any]]:
    kind = structure.get("kind")
    if kind == "table":
        return [{"unit_ref": "table", **structure}]
    if kind == "workbook":
        return [
            {"unit_ref": sheet.get("unit_ref", f"sheet:{sheet.get('sheet', '')}"), **sheet}
            for sheet in structure.get("sheets", [])
        ]
    if kind == "json":
        return [
            {"unit_ref": array.get("unit_ref", array.get("path", "$")), **array}
            for array in structure.get("arrays", [])
        ]
    return []


def _unit_observations(unit: dict[str, Any]) -> list[FieldObservation]:
    fields = unit.get("fields")
    if isinstance(fields, list):
        observations: list[FieldObservation] = []
        for field in fields:
            if not isinstance(field, dict) or not str(field.get("name", "")).strip():
                continue
            values = field.get("representative_values", [])
            observations.append(
                FieldObservation(
                    name=str(field["name"]).strip(),
                    sample_values=tuple(str(value)[:256] for value in values[:3]),
                )
            )
        if observations:
            return observations

    observations = []
    columns = unit.get("columns", [])
    rows = unit.get("preview", {}).get("rows", [])
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
    return observations


def _mapping_suggestion_payload(suggestion: Any) -> dict[str, Any]:
    return {
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


def _role_assessment_payload(assessment: SuggestedRoleAssessment) -> dict[str, Any]:
    return {
        "role": assessment.role.value,
        "state": assessment.state,
        "confidence": assessment.confidence,
        "ambiguous": assessment.ambiguous,
        "matched_required_fields": list(assessment.matched_required_fields),
        "missing_required_fields": list(assessment.missing_required_fields),
        "field_mappings": [
            {
                "target_field": mapping.target_field,
                "source_fields": list(mapping.source_fields),
                "transform": mapping.transform.model_dump(mode="json"),
                "score": mapping.score,
                "reason_code": mapping.reason_code,
            }
            for mapping in assessment.field_mappings
        ],
    }


def _administrative_assessment(structure: dict[str, Any]) -> dict[str, Any] | None:
    if structure.get("kind") != "json":
        return None
    keys = {
        key
        for values in structure.get("object_keys", {}).values()
        for key in values
    }
    if "admin_level" not in keys:
        return None
    return {
        "role": SourceRole.ADMINISTRATIVE_CATALOG.value,
        "state": "complete",
        "confidence": 1.0,
        "ambiguous": False,
        "matched_required_fields": [],
        "missing_required_fields": [],
        "field_mappings": [],
    }


def _mapping_analysis(
    relative_path: str,
    structure: dict[str, Any],
) -> list[dict[str, Any]]:
    del relative_path
    units: list[dict[str, Any]] = []
    unit_definitions = _unit_definitions(structure)
    administrative = _administrative_assessment(structure)
    for index, unit in enumerate(unit_definitions):
        observations = _unit_observations(unit)
        suggestions = [
            _mapping_suggestion_payload(item) for item in suggest_role_mappings(observations)
        ]
        assessments = [
            _role_assessment_payload(item) for item in assess_role_mappings(observations)
        ]
        if administrative is not None and index == 0:
            assessments.append(administrative)
        locator = {
            key: unit[key]
            for key in ("sheet", "path", "array_prefix")
            if unit.get(key) is not None
        }
        unit_kind = {
            "table": "table",
            "workbook": "sheet",
            "json": "json_array",
        }.get(structure.get("kind"), structure.get("kind"))
        unit_profile = {
            "unit_ref": str(unit.get("unit_ref", "table")),
            "kind": unit_kind,
            "locator": locator,
            "fields": unit.get("fields", []),
            "preview": unit.get("preview", {}),
            "record_count": unit.get("record_count", unit.get("length", 0)),
            "record_count_exact": unit.get(
                "record_count_exact", unit.get("length_exact", True)
            ),
            "role_assessments": assessments,
            "mapping_suggestions": suggestions,
        }
        units.append(unit_profile)
    return units


def _missing_fields_question(relative_path: str, missing_fields: tuple[str, ...]) -> str:
    if missing_fields == ("warehouse_type",):
        return (
            f"文件 {relative_path} 缺少仓型字段 warehouse_type。请在源数据中补充该列，"
            "每行使用 center 或 cross_docking，然后再继续。"
        )
    fields = ", ".join(missing_fields)
    return f"文件 {relative_path} 缺少必需字段：{fields}。请补充源数据后再继续。"


def _bound_agent_previews(profile: dict[str, Any]) -> dict[str, Any]:
    """Keep structural evidence while preventing sample rows from entering context."""

    def trim(value: Any) -> Any:
        if isinstance(value, dict):
            result = {key: trim(item) for key, item in value.items()}
            preview = result.get("preview")
            if isinstance(preview, dict) and isinstance(preview.get("rows"), list):
                preview["rows"] = preview["rows"][:3]
                preview["preview_sample_count"] = len(preview["rows"])
                preview["limit"] = min(int(preview.get("limit", 3)), 3)
            return result
        if isinstance(value, list):
            return [trim(item) for item in value]
        return value

    return trim(profile)


@mcp.tool(structured_output=True, annotations=WORKSPACE_PREPARATION_TOOL)
def prepare_network_input(
    inspection_identity: SourceInspectionIdentity,
    inspected_relative_paths: list[str],
    confirmed_sources: Annotated[
        list[ConfirmedSourceInputDecision],
        Field(
            description=(
                "Confirmed demand, warehouse, assignment, or route sources only. "
                "For an unambiguous inline source_profile suggestion, provide only relative_path "
                "and role and omit mappings; the Tool resolves the exact suggested mappings. "
                "Provide mappings only after explicit confirmation of an ambiguous suggestion. "
                "Never include an administrative catalog here."
            )
        ),
    ],
    country_code: Annotated[str, Field(pattern=r"^[A-Za-z]{2}$")],
    output_relative_path: Annotated[
        str,
        Field(
            min_length=1,
            max_length=1024,
            description=(
                "Create-new JSON path directly under outputs/warehouse-network/prepared/."
            ),
        ),
    ],
    ctx: Context,
    administrative_catalog_relative_path: Annotated[
        str | None,
        Field(
            description=(
                "Optional exact Workspace-relative administrative JSON path used atomically "
                "for geography enrichment. The same file must not appear in confirmed_sources."
            )
        ),
    ] = None,
    overrides: list[GeographyOverride] | None = None,
) -> DataPreparationToolResult:
    """Prepare one complete, validated Workspace input from confirmed raw sources.

    When an administrative catalog is supplied, geography enrichment is part
    of this same create-new operation and the returned path is the terminal
    Data-to-Network handoff.
    """
    inspection_identity = SourceInspectionIdentity.model_validate(inspection_identity)
    confirmed_sources = [_source_input_decision(decision) for decision in confirmed_sources]
    profile = _inspect_workspace_sources(inspected_relative_paths, ctx)
    if isinstance(profile, DataInspectionSelectionRequired):
        raise ProviderContractError("inspection_selection_required")
    content_sha256, source_count = source_inspection_identity(
        _workspace(ctx), inspected_relative_paths
    )
    if (
        inspection_identity.schema_version != "workspace_source_inspection.v1"
        or inspection_identity.content_sha256 != content_sha256
        or inspection_identity.source_count != source_count
    ):
        raise ProviderContractError("source_inspection_changed")
    country = country_code.strip().upper()
    if not re.fullmatch(r"[A-Z]{2}", country):
        raise ValueError("country_code_required_iso_alpha2")
    available = {str(source["relative_path"]): source for source in profile.get("sources", [])}
    inspected_set = set(inspected_relative_paths)
    if administrative_catalog_relative_path is not None:
        if administrative_catalog_relative_path not in inspected_set:
            raise ValueError("administrative_catalog_not_in_inspected_paths")
    if not confirmed_sources or len(confirmed_sources) > len(available):
        raise ValueError("confirmed_sources_must_select_profile_sources")
    selected_paths = [decision.relative_path for decision in confirmed_sources]
    if len(set(selected_paths)) != len(selected_paths):
        raise ValueError("confirmed_source_relative_paths_must_be_unique")
    if not set(selected_paths) <= inspected_set or not set(selected_paths) <= set(available):
        raise ValueError("confirmed_source_not_in_inspected_paths")
    requirements = [
        requirement
        for decision in confirmed_sources
        if (
            requirement := _confirmed_source_requirement(
                decision,
                available[decision.relative_path],
            )
        )
        is not None
    ]
    if requirements:
        return _needs_input_result(requirements)
    resolved_sources = [
        _resolve_confirmed_source_decision(decision, available[decision.relative_path])
        for decision in confirmed_sources
    ]
    normalized_sources = []
    for decision in resolved_sources:
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
        confirmed_sources=resolved_sources,
        **batch.model_dump(mode="json"),
    )
    if administrative_catalog_relative_path is not None:
        payload, geography_summary = _enrich_prepared_geography(
            payload,
            administrative_catalog_relative_path,
            ctx,
            overrides,
            parent_input_identity=None,
        )
        return _write_prepared_input(
            payload,
            output_relative_path,
            ctx,
            f"Normalized {len(resolved_sources)} confirmed Workspace sources and "
            f"{geography_summary}",
        )
    return _write_prepared_input(
        payload,
        output_relative_path,
        ctx,
        f"Normalized {len(resolved_sources)} confirmed Workspace sources; "
        f"{len(batch.demand_cities)} demand cities, {len(batch.warehouses)} warehouses, "
        f"{len(batch.current_assignments)} current assignments, "
        f"{len(batch.route_quotes)} route quotes, and "
        f"{len(batch.provided_route_facts)} provided route facts; state is {state}.",
    )


def _confirmed_source_requirement(
    decision: ConfirmedSourceInputDecision,
    source: dict[str, Any],
) -> DataSourceRequirement | None:
    if decision.mappings:
        missing_fields = tuple(
            sorted(
                REQUIRED_FIELDS[decision.role]
                - {mapping.target_field for mapping in decision.mappings}
            )
        )
        if not missing_fields:
            return None
        return DataSourceRequirement(
            code="required_fields_missing",
            relative_path=decision.relative_path,
            candidate_roles=[decision.role],
            missing_required_fields=list(missing_fields),
            question=_missing_fields_question(decision.relative_path, missing_fields),
        )

    suggestions = _source_unit_evidence(source, decision.role, "mapping_suggestions")
    if len(suggestions) == 1 and suggestions[0].get("ambiguous") is False:
        return None

    assessments = _source_unit_evidence(source, decision.role, "role_assessments")
    for assessment in assessments:
        missing_fields = tuple(
            sorted(str(field) for field in assessment.get("missing_required_fields", []))
        )
        if assessment.get("state") == "partial" and missing_fields:
            return DataSourceRequirement(
                code="required_fields_missing",
                relative_path=decision.relative_path,
                candidate_roles=[decision.role],
                missing_required_fields=list(missing_fields),
                question=_missing_fields_question(decision.relative_path, missing_fields),
            )

    if suggestions:
        return DataSourceRequirement(
            code="source_role_confirmation_required",
            relative_path=decision.relative_path,
            candidate_roles=[decision.role],
            missing_required_fields=[],
            question=(
                f"文件 {decision.relative_path} 的业务角色存在歧义。"
                f"请确认它是否属于 {decision.role.value}，并确认字段映射后再继续。"
            ),
        )
    return DataSourceRequirement(
        code="field_mapping_confirmation_required",
        relative_path=decision.relative_path,
        candidate_roles=[decision.role],
        missing_required_fields=[],
        question=(
            f"文件 {decision.relative_path} 无法完整映射为 {decision.role.value}。"
            "请补充缺失字段或明确提供完整字段映射后再继续。"
        ),
    )


def _source_unit_evidence(
    source: dict[str, Any], role: SourceRole, key: str
) -> list[dict[str, Any]]:
    """Read evidence only from individual units; never merge sheets or arrays."""

    evidence: list[dict[str, Any]] = []
    for unit in source.get("units", []):
        for item in unit.get(key, []):
            if item.get("role") == role.value:
                evidence.append({"unit_ref": unit.get("unit_ref"), **item})
    return evidence


def _resolve_confirmed_source_decision(
    decision: ConfirmedSourceInputDecision,
    source: dict[str, Any],
) -> ConfirmedSourceDecision:
    suggestions = _source_unit_evidence(source, decision.role, "mapping_suggestions")
    if decision.mappings:
        if len(suggestions) == 1 and suggestions[0].get("ambiguous") is False:
            raise ValueError(
                f"confirmed_mappings_not_allowed_for_unambiguous_source:"
                f"{decision.relative_path}:{decision.role.value}"
            )
        return ConfirmedSourceDecision.model_validate(decision.model_dump(mode="json"))
    if len(suggestions) != 1 or suggestions[0].get("ambiguous") is not False:
        raise ValueError(
            f"confirmed_mappings_required:{decision.relative_path}:{decision.role.value}"
        )
    mappings = []
    for mapping in suggestions[0].get("field_mappings", []):
        source_fields = mapping.get("source_fields")
        transform = mapping.get("transform")
        if not isinstance(source_fields, list) or len(source_fields) != 1:
            raise ValueError(
                f"confirmed_mapping_source_invalid:{decision.relative_path}:{decision.role.value}"
            )
        if not isinstance(transform, dict) or not isinstance(transform.get("kind"), str):
            raise ValueError(
                f"confirmed_mapping_transform_invalid:{decision.relative_path}:{decision.role.value}"
            )
        mappings.append(
            ConfirmedFieldDecision(
                source_field=str(source_fields[0]),
                target_field=str(mapping["target_field"]),
                transform=transform["kind"],
                factor=transform.get("factor"),
            )
        )
    return ConfirmedSourceDecision(
        relative_path=decision.relative_path,
        role=decision.role,
        mappings=mappings,
    )


def _source_input_decision(
    decision: ConfirmedSourceInputDecision | ConfirmedSourceDecision | dict[str, Any],
) -> ConfirmedSourceInputDecision:
    if isinstance(decision, ConfirmedSourceInputDecision):
        return ConfirmedSourceInputDecision.model_validate(decision.model_dump(mode="json"))
    return ConfirmedSourceInputDecision.model_validate(decision)


def _enrich_prepared_geography(
    payload: PreparedNetworkResource,
    administrative_catalog_relative_path: str,
    ctx: Context,
    overrides: list[GeographyOverride] | None,
    parent_input_identity: Any,
) -> tuple[PreparedNetworkResource, str]:
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
    catalog_document = read_json_document(_workspace(ctx), administrative_catalog_relative_path)
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
    missing_coordinates = sum(
        item.longitude is None or item.latitude is None
        for item in [*prepared.demand_cities, *prepared.warehouses]
    )
    state = (
        "needs_input"
        if any(issue.severity == "error" for issue in prepared.issues)
        else "needs_geography"
        if missing_coordinates
        else "ready"
    )
    return (
        PreparedNetworkResource(
            country_code=country_code,
            state=state,
            confirmed_sources=payload.confirmed_sources,
            parent_input_identity=parent_input_identity,
            **prepared.model_dump(mode="json"),
        ),
        f"enriched geography from the catalog's {admin_level.strip()} level; "
        f"{len(prepared.demand_cities)} demand cities and "
        f"{len(prepared.warehouses)} warehouses; missing-coordinate records "
        f"{missing_coordinates}; state is {state}",
    )


@mcp.tool(structured_output=True, annotations=WORKSPACE_PREPARATION_TOOL)
def prepare_network_geography(
    prepared_input_relative_path: Annotated[str, Field(min_length=1, max_length=1024)],
    administrative_catalog_relative_path: str,
    output_relative_path: Annotated[
        str,
        Field(
            min_length=1,
            max_length=1024,
            description=(
                "Create-new JSON path directly under outputs/warehouse-network/prepared/."
            ),
        ),
    ],
    ctx: Context,
    overrides: list[GeographyOverride] | None = None,
) -> DataPreparationToolResult:
    """Create a new prepared Workspace input enriched by one exact boundary catalog."""
    payload, parent_identity = load_prepared_network_input(
        _workspace(ctx), prepared_input_relative_path
    )
    prepared, summary = _enrich_prepared_geography(
        payload,
        administrative_catalog_relative_path,
        ctx,
        overrides,
        parent_input_identity=parent_identity,
    )
    return _write_prepared_input(
        prepared,
        output_relative_path,
        ctx,
        f"Prepared network geography and {summary}.",
    )


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Typed supply-chain inspection and preparation MCP server"
    )
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
