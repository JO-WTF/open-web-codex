"""Low-level MCP server for exact Indonesia Workspace Dataset Releases."""

from __future__ import annotations

import argparse
import asyncio
import json
import os
from pathlib import Path
from typing import Any
from urllib.parse import unquote, urlparse

from mcp import types
from mcp.server.lowlevel import Server
from mcp.server.lowlevel.helper_types import ReadResourceContents
from mcp.server.stdio import stdio_server

from .indonesia_analysis import (
    build_location_optimization,
    evaluate_candidate_scenario,
    evaluate_current_network,
    evaluate_service_baseline,
    inspect_dataset_release,
    load_network_data,
    prepare_network_map,
    require_valid_indonesia_resource,
    validate_indonesia_resource,
)
from .indonesia_models import (
    INDONESIA_GEOJSON_URI_PREFIX,
    INDONESIA_RESOURCE_URI_PREFIX,
    CandidateScenarioInput,
    IndonesiaCandidateScenario,
    IndonesiaCurrentNetworkAnalysis,
    IndonesiaDataRef,
    IndonesiaDatasetInspection,
    IndonesiaDecisionReport,
    IndonesiaDecisionReportSources,
    IndonesiaDecisionReportToolResult,
    IndonesiaGeoJsonRef,
    IndonesiaLocationOptimization,
    IndonesiaMapRenderToolResult,
    IndonesiaMapToolResult,
    IndonesiaNetworkMap,
    IndonesiaOptimizationToolResult,
    IndonesiaResourceToolResult,
    IndonesiaServiceBaseline,
    IndonesiaValidationResult,
    InspectDatasetReleaseInput,
    InspectionResourceInput,
    OptimizeWarehouseInput,
    PrepareDecisionReportInput,
    PrepareMapInput,
    PrepareMapRenderInput,
    ValidateResourceInput,
)
from .indonesia_report import build_decision_report
from .resource_store import PublishedResource, ResourceStore
from .workspace_dataset import load_workspace_dataset_release

MCP_SERVER_NAME = "supply_chain_indonesia"
SANDBOX_STATE_META_CAPABILITY = "codex/sandbox-state-meta"

app = Server(
    MCP_SERVER_NAME,
    instructions=(
        "Read one exact platform-authorized Indonesia Workspace Dataset Release through "
        "the trusted Codex Turn cwd. Never accept or expose host paths, scan Workspace "
        "directories, or return raw customer rows. First inspect the bound Release. Carry "
        "the returned data_ref unchanged. Current-network tools preserve actual assignments; "
        "candidate and optimization tools use only the finite reviewed candidate set and "
        "the declared capacity, distance, time, and quote policy. The map tool returns a "
        "bounded GeoJSON Resource for map_utils.create_map_card."
    ),
)

_profile_state_root = Path(os.environ.get("CODEX_HOME", Path.cwd() / ".codex")).resolve()
_resource_store: ResourceStore | None = None
_geojson_resource_store: ResourceStore | None = None


def _store() -> ResourceStore:
    global _resource_store
    if _resource_store is None:
        resource_root = Path(
            os.environ.get(
                "SUPPLY_CHAIN_INDONESIA_RESOURCE_DIR",
                _profile_state_root / "mcp-state" / "supply-chain-indonesia" / "resources",
            )
        ).resolve()
        _resource_store = ResourceStore(
            resource_root,
            uri_prefix=INDONESIA_RESOURCE_URI_PREFIX,
        )
    return _resource_store


def _geojson_store() -> ResourceStore:
    global _geojson_resource_store
    if _geojson_resource_store is None:
        _geojson_resource_store = ResourceStore(
            _store().root / "geojson",
            uri_prefix=INDONESIA_GEOJSON_URI_PREFIX,
        )
    return _geojson_resource_store


def _tool(
    *,
    name: str,
    title: str,
    description: str,
    input_model: type,
    output_model: type,
) -> types.Tool:
    return types.Tool(
        name=name,
        title=title,
        description=description,
        inputSchema=input_model.model_json_schema(),
        outputSchema=output_model.model_json_schema(),
        annotations=types.ToolAnnotations(
            readOnlyHint=True,
            destructiveHint=False,
            idempotentHint=True,
            openWorldHint=False,
        ),
    )


TOOLS = [
    _tool(
        name="inspect_indonesia_dataset_release",
        title="Inspect Indonesia Dataset Release",
        description=(
            "Verify one exact platform-bound Indonesia tutorial Dataset Release, stream "
            "all customer and assignment rows, validate province geometry and formulas, "
            "and publish only a bounded indonesia_dataset_inspection.v1 Resource."
        ),
        input_model=InspectDatasetReleaseInput,
        output_model=IndonesiaResourceToolResult,
    ),
    _tool(
        name="evaluate_indonesia_service_baseline",
        title="Evaluate Indonesia Service Baseline",
        description=(
            "Calculate current one-, two-, and three-day last-mile service coverage and "
            "province performance from actual forward-warehouse assignments. This "
            "progressive tutorial result excludes linehaul, capacity, and all cost fields. "
            "Consume the exact inspection Resource name returned by the inspection Tool; "
            "do not pass or construct a Resource URI."
        ),
        input_model=InspectionResourceInput,
        output_model=IndonesiaResourceToolResult,
    ),
    _tool(
        name="evaluate_indonesia_current_network",
        title="Evaluate Current Indonesia Network",
        description=(
            "Calculate actual one-, two-, and three-day service coverage, province "
            "performance, warehouse utilization, and two-level transport cost from a "
            "validated Indonesia Dataset Release. Actual assignments are not optimized. "
            "Consume the exact inspection Resource name returned by the inspection Tool."
        ),
        input_model=InspectionResourceInput,
        output_model=IndonesiaResourceToolResult,
    ),
    _tool(
        name="evaluate_indonesia_candidate",
        title="Evaluate One Indonesia Candidate Warehouse",
        description=(
            "Evaluate one reviewed candidate warehouse using deterministic customer "
            "reallocation, candidate and parent-center capacity, complete quotes, and "
            "explicit opening-cost amortization."
        ),
        input_model=CandidateScenarioInput,
        output_model=IndonesiaResourceToolResult,
    ),
    _tool(
        name="optimize_indonesia_new_warehouse",
        title="Select an Indonesia Candidate Warehouse",
        description=(
            "Evaluate all twenty reviewed candidates under one policy. If the current "
            "network already meets the target, return no-new-warehouse-needed; otherwise "
            "choose the lowest annual decision cost among candidates that meet the target, "
            "or the best achievable coverage when none do."
        ),
        input_model=OptimizeWarehouseInput,
        output_model=IndonesiaOptimizationToolResult,
    ),
    _tool(
        name="prepare_indonesia_network_map",
        title="Prepare Indonesia Network Comparison Map",
        description=(
            "Create a bounded GeoJSON comparison Resource from one validated current-network "
            "Resource and one compatible candidate Resource. It contains province summaries, "
            "warehouses, and linehaul links, never customer points."
        ),
        input_model=PrepareMapInput,
        output_model=IndonesiaMapToolResult,
    ),
    _tool(
        name="prepare_indonesia_map_render",
        title="Prepare Validated Indonesia Map Render Input",
        description=(
            "Resolve one exact indonesia_network_map.v1 Resource name and its exact "
            "geojson.v1 Resource name inside this server, validate their relationship, "
            "and return only the bounded fields required by map_utils.create_map_card."
        ),
        input_model=PrepareMapRenderInput,
        output_model=IndonesiaMapRenderToolResult,
    ),
    _tool(
        name="prepare_indonesia_decision_report",
        title="Prepare Validated Indonesia Decision Report",
        description=(
            "Resolve and cross-check the exact inspection, service, current-network, "
            "optimization, candidate, map and GeoJSON Resource identities, then "
            "publish deterministic evidence-backed Markdown. The caller must copy the "
            "returned report_markdown unchanged and must not add model-derived numbers "
            "or operating effects. Browser visualization Artifacts are owned and "
            "delivered separately by map_utils."
        ),
        input_model=PrepareDecisionReportInput,
        output_model=IndonesiaDecisionReportToolResult,
    ),
    _tool(
        name="validate_indonesia_resource",
        title="Validate Indonesia Planning Resource",
        description=(
            "Validate a typed Indonesia tutorial Resource and its cross-field totals before "
            "using it in a conclusion or map."
        ),
        input_model=ValidateResourceInput,
        output_model=IndonesiaValidationResult,
    ),
]


@app.list_tools()
async def list_tools() -> list[types.Tool]:
    return TOOLS


@app.list_resources()
async def list_resources() -> list[types.Resource]:
    # Resources are content-addressed handoffs. Callers receive exact links from Tools.
    return []


@app.list_resource_templates()
async def list_resource_templates() -> list[types.ResourceTemplate]:
    return []


@app.read_resource()
async def read_resource(uri: Any) -> list[ReadResourceContents]:
    resource_uri = str(uri)
    if resource_uri.startswith(INDONESIA_GEOJSON_URI_PREFIX):
        store = _geojson_store()
        mime_type = "application/geo+json"
    elif resource_uri.startswith(INDONESIA_RESOURCE_URI_PREFIX):
        store = _store()
        mime_type = "application/json"
    else:
        raise ValueError("Unsupported Indonesia planning Resource URI")
    return [
        ReadResourceContents(
            content=json.dumps(
                store.load_uri(resource_uri),
                ensure_ascii=False,
                sort_keys=True,
                separators=(",", ":"),
            ),
            mime_type=mime_type,
        )
    ]


@app.call_tool()
async def call_tool(
    name: str,
    arguments: dict[str, Any],
) -> types.CallToolResult:
    if name == "inspect_indonesia_dataset_release":
        request = InspectDatasetReleaseInput.model_validate(arguments)
        workspace_root = _trusted_workspace_root()
        await _progress(0, "Verifying the exact Dataset Release and streaming 240,000 rows.")
        inspection = await asyncio.to_thread(
            _inspect_release,
            workspace_root,
            request,
        )
        await _progress(100, "Dataset Release inspection completed.")
        return _publish_result(
            inspection,
            summary=(
                f"Verified {inspection.customer_count:,} synthetic customers across "
                f"{inspection.province_count} provinces, {inspection.central_warehouse_count} "
                f"centers, {inspection.forward_warehouse_count} forward warehouses, "
                f"{inspection.candidate_location_count} candidates, and "
                f"{inspection.quote_row_count:,} quote rows."
            ),
        )

    if name == "evaluate_indonesia_service_baseline":
        request = InspectionResourceInput.model_validate(arguments)
        await _progress(0, "Calculating current last-mile service and province metrics.")
        data = await asyncio.to_thread(
            _load_bound_data,
            request.inspection_resource_name,
        )
        baseline = await asyncio.to_thread(evaluate_service_baseline, data)
        await _progress(100, "Service-baseline evaluation completed.")
        return _publish_result(
            baseline,
            summary=(
                f"Current last-mile demand coverage is "
                f"{baseline.coverage.demand_coverage['1_day']:.2%} in one day, "
                f"{baseline.coverage.demand_coverage['2_day']:.2%} in two days, and "
                f"{baseline.coverage.demand_coverage['3_day']:.2%} in three days. "
                "This baseline excludes linehaul, capacity, and cost."
            ),
        )

    if name == "evaluate_indonesia_current_network":
        request = InspectionResourceInput.model_validate(arguments)
        await _progress(0, "Calculating actual service, province, capacity, and cost metrics.")
        data = await asyncio.to_thread(
            _load_bound_data,
            request.inspection_resource_name,
        )
        analysis = await asyncio.to_thread(evaluate_current_network, data)
        await _progress(100, "Current-network evaluation completed.")
        return _publish_result(
            analysis,
            summary=(
                f"Actual demand coverage is "
                f"{analysis.coverage.demand_coverage['1_day']:.2%} in one day, "
                f"{analysis.coverage.demand_coverage['2_day']:.2%} in two days, and "
                f"{analysis.coverage.demand_coverage['3_day']:.2%} in three days; "
                f"annual two-level transport cost is IDR "
                f"{analysis.costs.transport_total_idr:,}."
            ),
        )

    if name == "evaluate_indonesia_candidate":
        request = CandidateScenarioInput.model_validate(arguments)
        await _progress(0, f"Evaluating candidate {request.candidate_id}.")
        data = await asyncio.to_thread(
            _load_bound_data,
            request.inspection_resource_name,
        )
        scenario = await asyncio.to_thread(
            evaluate_candidate_scenario,
            data,
            request.candidate_id,
            request.opening_amortization_years,
        )
        await _progress(100, f"Candidate {request.candidate_id} evaluation completed.")
        return _publish_result(
            scenario,
            summary=(
                f"{scenario.candidate.candidate_name} takes "
                f"{scenario.candidate.selected_demand_units:,} annual demand units; "
                f"two-day demand coverage changes by "
                f"{scenario.demand_coverage_delta['2_day']:.2%}, annual transport cost "
                f"changes by IDR {scenario.transport_cost_delta_idr:,}, and annual decision "
                f"cost changes by IDR {scenario.annual_decision_cost_delta_idr:,}."
            ),
        )

    if name == "optimize_indonesia_new_warehouse":
        request = OptimizeWarehouseInput.model_validate(arguments)
        await _progress(
            0,
            "Evaluating the complete finite candidate set under one assignment policy.",
        )
        data = await asyncio.to_thread(
            _load_bound_data,
            request.inspection_resource_name,
        )
        optimization, selected_scenario, _ = await asyncio.to_thread(
            build_location_optimization,
            data,
            target_service_days=request.target_service_days,
            target_demand_coverage=request.target_demand_coverage,
            opening_amortization_years=request.opening_amortization_years,
        )
        selected_published = None
        if selected_scenario is not None:
            selected_published = _publish_validated(selected_scenario)
            optimization = optimization.model_copy(
                update={
                    "selected_scenario_resource_name": (selected_published.resource_id),
                    "selected_scenario_ref": _data_ref(selected_published),
                }
            )
        published = _publish_validated(optimization)
        await _progress(100, "Finite candidate evaluation completed.")
        selected_name = (
            data.candidates[optimization.selected_candidate_id].name
            if optimization.selected_candidate_id is not None
            else None
        )
        if optimization.status == "target_already_met":
            summary = (
                f"The actual network already meets "
                f"{request.target_demand_coverage:.2%} demand coverage within "
                f"{request.target_service_days} day(s); no new warehouse is required "
                "for this target."
            )
        else:
            summary = (
                f"Evaluated {optimization.evaluated_candidate_count} candidates; "
                f"{optimization.target_met_candidate_count} meet the target; selected "
                f"{selected_name}. Status is {optimization.status} for "
                f"{request.target_demand_coverage:.2%} demand coverage within "
                f"{request.target_service_days} day(s)."
            )
        structured = IndonesiaOptimizationToolResult(
            summary=summary,
            resource_name=published.resource_id,
            data_ref=_data_ref(published),
            evaluated_candidate_count=optimization.evaluated_candidate_count,
            target_met_candidate_count=optimization.target_met_candidate_count,
            selected_candidate_id=optimization.selected_candidate_id,
            status=optimization.status,
            selected_scenario_resource_name=(
                selected_published.resource_id if selected_published else None
            ),
            selected_scenario_ref=(_data_ref(selected_published) if selected_published else None),
        ).model_dump(mode="json")
        return _optimization_call_result(
            published,
            selected_published,
            summary=summary,
            structured=structured,
        )

    if name == "prepare_indonesia_network_map":
        request = PrepareMapInput.model_validate(arguments)
        baseline_payload = _load_validated_resource_name(
            request.baseline_resource_name,
            "indonesia_current_network_analysis.v1",
        )
        candidate_payload = _load_validated_resource_name(
            request.candidate_resource_name,
            "indonesia_candidate_scenario.v1",
        )
        baseline = IndonesiaCurrentNetworkAnalysis.model_validate(baseline_payload)
        candidate = IndonesiaCandidateScenario.model_validate(candidate_payload)
        prepared_map = prepare_network_map(
            baseline,
            candidate,
            baseline_resource_name=request.baseline_resource_name,
            candidate_resource_name=request.candidate_resource_name,
        )
        geojson_published = _geojson_store().publish(
            "geojson.v1",
            prepared_map.geojson,
        )
        map_resource = prepared_map.to_resource(
            geojson_resource_name=geojson_published.resource_id,
            geojson_ref=_geojson_ref(geojson_published),
        )
        map_published = _publish_validated(map_resource)
        structured = IndonesiaMapToolResult(
            summary=map_resource.summary,
            resource_name=map_published.resource_id,
            data_ref=_data_ref(map_published),
            geojson_resource_name=geojson_published.resource_id,
            geojson_ref=_geojson_ref(geojson_published),
        ).model_dump(mode="json")
        return _map_call_result(
            map_published,
            geojson_published,
            summary=map_resource.summary,
            structured=structured,
        )

    if name == "prepare_indonesia_map_render":
        request = PrepareMapRenderInput.model_validate(arguments)
        map_payload = _load_validated_resource_name(
            request.map_resource_name,
            "indonesia_network_map.v1",
        )
        network_map = IndonesiaNetworkMap.model_validate(map_payload)
        if network_map.geojson_resource_name != request.geojson_resource_name:
            raise ValueError("Map and GeoJSON Resource names do not match")
        expected_geojson_uri = (
            f"{INDONESIA_GEOJSON_URI_PREFIX}{request.geojson_resource_name}"
        )
        if network_map.geojson_ref.uri != expected_geojson_uri:
            raise ValueError("Map GeoJSON reference does not match its Resource name")
        geojson = _geojson_store().load_uri(expected_geojson_uri)
        features = geojson.get("features")
        if geojson.get("type") != "FeatureCollection" or not isinstance(features, list):
            raise ValueError("GeoJSON Resource is not a FeatureCollection")
        if len(features) != network_map.feature_count:
            raise ValueError("Map feature count does not match its GeoJSON Resource")
        structured = IndonesiaMapRenderToolResult(
            summary=network_map.summary,
            map_resource_name=request.map_resource_name,
            geojson_resource_name=request.geojson_resource_name,
            geojson_ref=network_map.geojson_ref,
            title=network_map.title,
            feature_count=network_map.feature_count,
            layers=network_map.layers,
            extensions=network_map.extensions,
        ).model_dump(mode="json")
        return types.CallToolResult(
            content=[
                types.TextContent(
                    type="text",
                    text=(
                        f"Validated {request.map_resource_name} with "
                        f"{request.geojson_resource_name} for map rendering."
                    ),
                )
            ],
            structuredContent=structured,
        )

    if name == "prepare_indonesia_decision_report":
        request = PrepareDecisionReportInput.model_validate(arguments)
        inspection = IndonesiaDatasetInspection.model_validate(
            _load_validated_resource_name(
                request.inspection_resource_name,
                "indonesia_dataset_inspection.v1",
            )
        )
        service = IndonesiaServiceBaseline.model_validate(
            _load_validated_resource_name(
                request.service_resource_name,
                "indonesia_service_baseline.v1",
            )
        )
        current = IndonesiaCurrentNetworkAnalysis.model_validate(
            _load_validated_resource_name(
                request.current_resource_name,
                "indonesia_current_network_analysis.v1",
            )
        )
        optimization = IndonesiaLocationOptimization.model_validate(
            _load_validated_resource_name(
                request.optimization_resource_name,
                "indonesia_location_optimization.v1",
            )
        )
        candidate = IndonesiaCandidateScenario.model_validate(
            _load_validated_resource_name(
                request.candidate_resource_name,
                "indonesia_candidate_scenario.v1",
            )
        )
        network_map = IndonesiaNetworkMap.model_validate(
            _load_validated_resource_name(
                request.map_resource_name,
                "indonesia_network_map.v1",
            )
        )
        expected_geojson_uri = (
            f"{INDONESIA_GEOJSON_URI_PREFIX}{request.geojson_resource_name}"
        )
        geojson = _geojson_store().load_uri(expected_geojson_uri)
        if (
            geojson.get("type") != "FeatureCollection"
            or len(geojson.get("features", [])) != network_map.feature_count
        ):
            raise ValueError("Decision-report GeoJSON does not match the map Resource")
        report = build_decision_report(
            inspection=inspection,
            service=service,
            current=current,
            optimization=optimization,
            candidate=candidate,
            network_map=network_map,
            sources=IndonesiaDecisionReportSources(
                **request.model_dump(mode="json"),
            ),
        )
        published = _publish_validated(report)
        summary = (
            "Published deterministic Indonesia decision-report Markdown from seven "
            "cross-checked Resource identities."
        )
        structured = IndonesiaDecisionReportToolResult(
            summary=summary,
            resource_name=published.resource_id,
            data_ref=_data_ref(published),
            report_markdown=report.markdown,
        ).model_dump(mode="json")
        return _call_result(published, summary=summary, structured=structured)

    if name == "validate_indonesia_resource":
        request = ValidateResourceInput.model_validate(arguments)
        payload = _load_ref(
            request.resource_ref,
            request.resource_ref.resource_schema,
        )
        validation = validate_indonesia_resource(payload)
        return types.CallToolResult(
            content=[
                types.TextContent(
                    type="text",
                    text=(
                        f"{validation.resource_schema} validation "
                        f"{'passed' if validation.valid else 'failed'} with "
                        f"{len(validation.errors)} errors and "
                        f"{len(validation.warnings)} warnings."
                    ),
                )
            ],
            structuredContent=validation.model_dump(mode="json"),
        )

    raise ValueError(f"Unknown Indonesia planning tool: {name}")


def _inspect_release(
    workspace_root: Path,
    request: InspectDatasetReleaseInput,
) -> IndonesiaDatasetInspection:
    release = load_workspace_dataset_release(workspace_root, request.release)
    return inspect_dataset_release(release)


def _load_bound_data(inspection_resource_name: str):
    inspection_payload = _load_validated_resource_name(
        inspection_resource_name,
        "indonesia_dataset_inspection.v1",
    )
    inspection = IndonesiaDatasetInspection.model_validate(inspection_payload)
    release = load_workspace_dataset_release(
        _trusted_workspace_root(),
        inspection.release,
    )
    return load_network_data(release, validate_customer_geometry=False)


def _trusted_workspace_root() -> Path:
    meta = app.request_context.meta
    extra = meta.model_extra if meta is not None else None
    sandbox_state = extra.get(SANDBOX_STATE_META_CAPABILITY) if isinstance(extra, dict) else None
    sandbox_cwd = sandbox_state.get("sandboxCwd") if isinstance(sandbox_state, dict) else None
    if not isinstance(sandbox_cwd, str):
        raise ValueError(
            "Codex did not provide the trusted Turn Workspace metadata required "
            "for Dataset Release access"
        )
    parsed = urlparse(sandbox_cwd)
    if parsed.scheme != "file" or parsed.netloc not in ("", "localhost"):
        raise ValueError("The trusted Turn Workspace is not a local file URI")
    path = Path(unquote(parsed.path)).resolve(strict=True)
    if not path.is_dir():
        raise ValueError("The trusted Turn Workspace is not a directory")
    return path


async def _progress(progress: int, message: str) -> None:
    context = app.request_context
    token = context.meta.progressToken if context.meta is not None else None
    if token is not None:
        await context.session.send_progress_notification(
            progress_token=token,
            progress=progress,
            total=100,
            message=message,
            related_request_id=context.request_id,
        )


def _load_ref(
    resource_ref: IndonesiaDataRef,
    expected_schema: str,
) -> dict[str, Any]:
    if resource_ref.server != MCP_SERVER_NAME:
        raise ValueError(f"data_ref.server must be {MCP_SERVER_NAME}")
    if resource_ref.resource_schema != expected_schema:
        raise ValueError(f"Expected {expected_schema}, got {resource_ref.resource_schema}")
    payload = _store().load_uri(resource_ref.uri)
    if payload.get("schema_version") != expected_schema:
        raise ValueError("Resource payload schema does not match its data_ref")
    return payload


def _load_validated_ref(
    resource_ref: IndonesiaDataRef,
    expected_schema: str,
) -> dict[str, Any]:
    payload = _load_ref(resource_ref, expected_schema)
    require_valid_indonesia_resource(payload)
    return payload


def _load_resource_name(
    resource_name: str,
    expected_schema: str,
) -> dict[str, Any]:
    if not resource_name.startswith(f"{expected_schema}-"):
        raise ValueError(f"Expected {expected_schema} Resource name")
    payload = _store().load_uri(f"{INDONESIA_RESOURCE_URI_PREFIX}{resource_name}")
    if payload.get("schema_version") != expected_schema:
        raise ValueError("Resource payload schema does not match its Resource name")
    return payload


def _load_validated_resource_name(
    resource_name: str,
    expected_schema: str,
) -> dict[str, Any]:
    payload = _load_resource_name(resource_name, expected_schema)
    require_valid_indonesia_resource(payload)
    return payload


def _data_ref(published: PublishedResource) -> IndonesiaDataRef:
    return IndonesiaDataRef(
        uri=published.uri,
        resource_schema=published.schema,
    )


def _geojson_ref(published: PublishedResource) -> IndonesiaGeoJsonRef:
    return IndonesiaGeoJsonRef(uri=published.uri)


def _publish_result(
    value: (
        IndonesiaDatasetInspection
        | IndonesiaServiceBaseline
        | IndonesiaCurrentNetworkAnalysis
        | IndonesiaCandidateScenario
        | IndonesiaLocationOptimization
        | IndonesiaNetworkMap
        | IndonesiaDecisionReport
    ),
    *,
    summary: str,
) -> types.CallToolResult:
    published = _publish_validated(value)
    structured = IndonesiaResourceToolResult(
        summary=summary,
        resource_name=published.resource_id,
        data_ref=_data_ref(published),
    ).model_dump(mode="json")
    return _call_result(published, summary=summary, structured=structured)


def _publish_validated(
    value: (
        IndonesiaDatasetInspection
        | IndonesiaServiceBaseline
        | IndonesiaCurrentNetworkAnalysis
        | IndonesiaCandidateScenario
        | IndonesiaLocationOptimization
        | IndonesiaNetworkMap
        | IndonesiaDecisionReport
    ),
) -> PublishedResource:
    require_valid_indonesia_resource(value.model_dump(mode="json"))
    return _store().publish(value.schema_version, value)


def _call_result(
    published: PublishedResource,
    *,
    summary: str,
    structured: dict[str, Any],
) -> types.CallToolResult:
    return types.CallToolResult(
        content=[
            types.TextContent(type="text", text=summary),
            types.ResourceLink(
                type="resource_link",
                name=published.resource_id,
                title=published.schema,
                uri=published.uri,
                description=summary,
                mimeType="application/json",
                size=published.size,
            ),
        ],
        structuredContent=structured,
    )


def _optimization_call_result(
    optimization_resource: PublishedResource,
    selected_scenario_resource: PublishedResource | None,
    *,
    summary: str,
    structured: dict[str, Any],
) -> types.CallToolResult:
    content: list[types.ContentBlock] = [
        types.TextContent(type="text", text=summary),
        types.ResourceLink(
            type="resource_link",
            name=optimization_resource.resource_id,
            title=optimization_resource.schema,
            uri=optimization_resource.uri,
            description=summary,
            mimeType="application/json",
            size=optimization_resource.size,
        ),
    ]
    if selected_scenario_resource is not None:
        content.append(
            types.ResourceLink(
                type="resource_link",
                name=selected_scenario_resource.resource_id,
                title=selected_scenario_resource.schema,
                uri=selected_scenario_resource.uri,
                description="Selected finite-candidate warehouse scenario.",
                mimeType="application/json",
                size=selected_scenario_resource.size,
            )
        )
    return types.CallToolResult(
        content=content,
        structuredContent=structured,
    )


def _map_call_result(
    map_resource: PublishedResource,
    geojson_resource: PublishedResource,
    *,
    summary: str,
    structured: dict[str, Any],
) -> types.CallToolResult:
    return types.CallToolResult(
        content=[
            types.TextContent(type="text", text=summary),
            types.ResourceLink(
                type="resource_link",
                name=map_resource.resource_id,
                title=map_resource.schema,
                uri=map_resource.uri,
                description=summary,
                mimeType="application/json",
                size=map_resource.size,
            ),
            types.ResourceLink(
                type="resource_link",
                name=geojson_resource.resource_id,
                title=geojson_resource.schema,
                uri=geojson_resource.uri,
                description="Bounded Indonesia network comparison GeoJSON.",
                mimeType="application/geo+json",
                size=geojson_resource.size,
            ),
        ],
        structuredContent=structured,
    )


async def run_stdio() -> None:
    initialization_options = app.create_initialization_options(
        experimental_capabilities={SANDBOX_STATE_META_CAPABILITY: {}},
    )
    async with stdio_server() as streams:
        await app.run(
            streams[0],
            streams[1],
            initialization_options,
        )


def main() -> None:
    parser = argparse.ArgumentParser(description="Indonesia Workspace Dataset Release MCP server")
    parser.add_argument(
        "--workspace-root",
        type=Path,
        default=Path.cwd(),
        help="Plugin root used only for the fallback Profile Resource directory",
    )
    args = parser.parse_args()

    global _profile_state_root, _resource_store, _geojson_resource_store
    plugin_root = args.workspace_root.resolve()
    _profile_state_root = Path(os.environ.get("CODEX_HOME", plugin_root / ".codex")).resolve()
    _resource_store = ResourceStore(
        Path(
            os.environ.get(
                "SUPPLY_CHAIN_INDONESIA_RESOURCE_DIR",
                _profile_state_root / "mcp-state" / "supply-chain-indonesia" / "resources",
            )
        ).resolve(),
        uri_prefix=INDONESIA_RESOURCE_URI_PREFIX,
    )
    _geojson_resource_store = ResourceStore(
        _resource_store.root / "geojson",
        uri_prefix=INDONESIA_GEOJSON_URI_PREFIX,
    )
    asyncio.run(run_stdio())


if __name__ == "__main__":
    main()
