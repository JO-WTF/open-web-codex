"""FastMCP entry point for supply-chain network planning."""

from __future__ import annotations

import argparse
import asyncio
import os
import urllib.parse
from pathlib import Path
from typing import Annotated, Literal

from mcp.server.fastmcp import Context, FastMCP
from mcp.server.stdio import stdio_server
from mcp.types import CallToolResult, TextContent, ToolAnnotations
from open_web_codex_provider import (
    MAX_WORKSPACE_FILE_BYTES,
    GeoJsonResourceRef,
    McpResourceRuntime,
    ProviderContractError,
    ResourceRef,
    ResourceStore,
    derive_geojson_profile,
)
from pydantic import Field, ValidationError
from supply_chain_planner.data.workspace_intake import read_json_document
from supply_chain_planner.delivery.map_service import (
    NetworkComparisonGeoJson,
    NetworkComparisonMapBundle,
    NetworkCoverageGeoJson,
    NetworkDistributionGeoJson,
    build_network_comparison_map_bundle,
    build_network_coverage_geojson,
    build_network_distribution_geojson,
)
from supply_chain_planner.delivery.report_service import (
    NETWORK_PLANNING_MARKDOWN_SCHEMA,
    build_network_baseline_assessment_report_bundle,
    build_network_planning_report_bundle,
    render_network_baseline_assessment_markdown,
    render_network_planning_report_markdown,
)
from supply_chain_planner.network.matrix import build_cost_matrix as _build_composable_cost_matrix
from supply_chain_planner.network.matrix import (
    build_navigation_matrix_request,
    build_route_matrix_with_reuse,
    derive_observed_quote_mean_cost_policy,
)
from supply_chain_planner.network.matrix import (
    build_provided_route_matrix as _build_provided_route_matrix,
)
from supply_chain_planner.network.matrix import (
    register_navigation_route_matrix as _register_composable_navigation_matrix,
)
from supply_chain_planner.network.matrix import (
    validate_route_matrix as _validate_route_matrix_model,
)
from supply_chain_planner.network.matrix_models import (
    CostCalculationPolicy,
    CostMatrix,
    CostPolicySelection,
    ExplicitCostPolicy,
    NavigationMatrixResult,
    ObservedQuoteMeanCostPolicy,
)
from supply_chain_planner.network.matrix_models import RouteMatrix as ComposableRouteMatrix
from supply_chain_planner.network.models import (
    DemandCityRecord,
    NormalizedInputBatch,
    PlanningInputIdentity,
    WarehouseRecord,
)
from supply_chain_planner.network.optimization_models import (
    AssignmentComparison,
    AssignmentResult,
    BaselineResult,
    CostSummary,
    CoverageMetricSummary,
    ExistingWarehousePolicy,
    PMedianSolution,
    ScenarioResult,
    ScenarioSpec,
    ServiceCoverageConstraint,
    ServiceMetric,
    WarehouseChanges,
)
from supply_chain_planner.network.solver import (
    SolverUnavailable,
    compare_assignments,
    coverage_metrics,
    enumerate_p_median,
    service_metrics,
    solve_assignment,
    solve_current_assignment,
    summarize_assignment_cost,
)
from supply_chain_planner.shared.models import (
    AssignmentResultResourceRef,
    ComparableNetworkResultRef,
    CostMatrixPlanningToolResult,
    FacilityChangeAssessmentToolResult,
    FacilityChangeCostComparison,
    NavigationMatrixRequestToolResult,
    NetworkBaselineResourceToolResult,
    NetworkFinalArtifactDescriptor,
    NetworkFinalArtifactToolResult,
    NetworkPlanComparisonResource,
    NetworkPlanComparisonResourceRef,
    NetworkReportInput,
    NetworkScenarioResourceRef,
    PMedianSolutionToolResult,
    PreparedNetworkResource,
    RouteMatrixPreparationToolResult,
    UncoveredCitySummary,
)
from supply_chain_planner.shared.planning_input import (
    load_prepared_network_input,
    require_matching_input,
)
from supply_chain_planner.shared.resources import SupplyChainResources
from supply_chain_planner.shared.workspace_outputs import (
    WorkspaceOutputKind,
    prepare_workspace_output_path,
)

McpResourceContractError = ProviderContractError

SANDBOX_STATE_META_CAPABILITY = "codex/sandbox-state-meta"
RESOURCE_URI_PREFIX = "supply-chain://resources/"

CONTENT_ADDRESSED_RESOURCE_TOOL = ToolAnnotations(
    readOnlyHint=False,
    destructiveHint=False,
    idempotentHint=True,
    openWorldHint=False,
)
WORKSPACE_REQUEST_TOOL = ToolAnnotations(
    readOnlyHint=False,
    destructiveHint=False,
    idempotentHint=False,
    openWorldHint=False,
)
BOUNDED_LOCAL_COMPUTE_TOOL = ToolAnnotations(
    readOnlyHint=False,
    destructiveHint=False,
    idempotentHint=False,
    openWorldHint=False,
)
FINAL_WORKSPACE_DELIVERY_TOOL = ToolAnnotations(
    readOnlyHint=False,
    destructiveHint=False,
    idempotentHint=False,
    openWorldHint=True,
)


def _bounded_id_summary(values: list[str], *, limit: int = 4) -> str:
    unique = list(dict.fromkeys(values))
    shown: list[str] = []
    rendered_chars = 0
    for value in unique[:limit]:
        bounded = value if len(value) <= 64 else f"{value[:61]}..."
        separator_chars = 2 if shown else 0
        if shown and rendered_chars + separator_chars + len(bounded) > 160:
            break
        shown.append(bounded)
        rendered_chars += separator_chars + len(bounded)
    if not shown:
        return "none"
    remaining = len(unique) - len(shown)
    suffix = f", +{remaining} more" if remaining else ""
    return f"{', '.join(shown)}{suffix}"


def _service_metric_summary(metrics: list[ServiceMetric], *, limit: int = 8) -> str:
    shown = [
        f"{metric.target_hours:g}h demand-weighted={metric.coverage_rate:.1%}"
        for metric in metrics[:limit]
    ]
    if not shown:
        return "none"
    remaining = len(metrics) - len(shown)
    suffix = f", +{remaining} more" if remaining else ""
    return f"{', '.join(shown)}{suffix}"


def _baseline_coverage_projection(
    assignment: AssignmentResult,
    demand_cities: list[DemandCityRecord],
    warehouses: list[WarehouseRecord],
    targets: list[float],
) -> tuple[list[CoverageMetricSummary], float, list[UncoveredCitySummary]]:
    coverage = coverage_metrics(assignment, targets)
    detail_target = max(targets)
    demand_by_id = {item.city_id: item for item in demand_cities}
    warehouse_by_id = {item.warehouse_id: item for item in warehouses}
    uncovered: list[UncoveredCitySummary] = []
    for row in sorted(assignment.rows, key=lambda item: item.demand_city_id):
        if row.duration_hours is not None and row.duration_hours <= detail_target:
            continue
        demand = demand_by_id[row.demand_city_id]
        warehouse = warehouse_by_id.get(row.warehouse_id) if row.warehouse_id else None
        uncovered.append(
            UncoveredCitySummary(
                demand_city_id=row.demand_city_id,
                demand_city_name=demand.city_name,
                warehouse_id=row.warehouse_id,
                warehouse_name=warehouse.warehouse_name if warehouse else None,
                duration_hours=row.duration_hours,
                demand_quantity=row.demand_quantity,
            )
        )
    return coverage, detail_target, uncovered


def _coverage_metric_summary(metrics: list[CoverageMetricSummary], *, limit: int = 8) -> str:
    shown = [
        f"{metric.target_hours:g}h city-count={metric.city_coverage_rate:.1%} "
        f"({metric.covered_city_count}/{metric.total_city_count}), "
        f"demand-weighted={metric.demand_weighted_coverage_rate:.1%}"
        for metric in metrics[:limit]
    ]
    if not shown:
        return "none"
    remaining = len(metrics) - len(shown)
    suffix = f", +{remaining} more" if remaining else ""
    return f"{'; '.join(shown)}{suffix}"


def _cost_metric_summary(cost: CostSummary | None) -> str:
    if cost is None:
        return "unavailable"
    state = "complete" if cost.complete else "incomplete"
    return f"{cost.total:.2f} {cost.currency} ({state})"


mcp = FastMCP(
    "Supply Chain Network Planner",
    instructions=(
        "仓网工具以平台发布的 Resource 引用作为输入和输出，不把原始数据、矩阵行或"
        "内部 Work State 标识复制到 Agent 消息。球面距离必须明确提供绕路系数和"
        "平均速度；导航必须先报告路线数量和费用风险并获得用户许可。没有当前覆盖"
        "关系时，结果必须标记为现有仓优化基线，不能称为当前实际方案。"
    ),
    json_response=True,
)

_workspace_root = Path.cwd().resolve()
_profile_state_root = Path(os.environ.get("CODEX_HOME", _workspace_root / ".codex")).resolve()
_supply_chain_resources: SupplyChainResources | None = None


def _store() -> ResourceStore:
    return _runtime().store


def _runtime() -> McpResourceRuntime:
    global _supply_chain_resources
    if _supply_chain_resources is None:
        _supply_chain_resources = SupplyChainResources(_workspace_root, _profile_state_root)
    return _supply_chain_resources.network


@mcp.resource(
    "supply-chain://resources/{resource_id}",
    name="supply_chain_resource",
    title="Supply-chain planning resource",
    mime_type="application/json",
)
def read_supply_chain_resource(resource_id: str) -> str:
    """Read an immutable JSON planning resource created by this MCP server."""
    return _store().read(resource_id)


def _load_comparable_resource(
    resource_ref: ComparableNetworkResultRef,
) -> tuple[AssignmentResult, set[str], PlanningInputIdentity]:
    """Load one supported comparison subject through the provider runtime."""
    if resource_ref.resource_schema == "network_baseline.v2":
        result = _runtime().load_model(
            resource_ref,
            "network_baseline.v2",
            BaselineResult,
        )
    elif resource_ref.resource_schema == "network_scenario.v2":
        result = _runtime().load_model(
            resource_ref,
            "network_scenario.v2",
            ScenarioResult,
        )
    elif resource_ref.resource_schema == "facility_location_solution.v3":
        result = _runtime().load_model(
            resource_ref,
            "facility_location_solution.v3",
            PMedianSolution,
        )
        if result.assignment is None:
            raise McpResourceContractError("comparable_assignment_unavailable")
    else:
        raise McpResourceContractError("comparison_subject_schema_invalid")
    return result.assignment, set(result.active_warehouse_ids), result.input_identity


@mcp.tool(structured_output=True, annotations=CONTENT_ADDRESSED_RESOURCE_TOOL)
def compare_network_scenarios(
    prepared_input_relative_path: Annotated[str, Field(min_length=1, max_length=1024)],
    before_ref: ComparableNetworkResultRef,
    after_ref: ComparableNetworkResultRef,
    service_targets: Annotated[list[float], Field(min_length=1, max_length=32)],
    ctx: Context,
) -> CallToolResult:
    """Compare two typed network results in either direction."""
    _prepared, input_identity = _load_ready_network(prepared_input_relative_path, ctx)
    try:
        before_ref = ComparableNetworkResultRef.model_validate(before_ref.model_dump(mode="json"))
        after_ref = ComparableNetworkResultRef.model_validate(after_ref.model_dump(mode="json"))
    except ValidationError as error:
        raise McpResourceContractError("comparison_subject_schema_invalid") from error
    if any(target <= 0 for target in service_targets):
        raise McpResourceContractError("comparison_service_targets_invalid")
    before_assignment, before_active_ids, before_identity = _load_comparable_resource(before_ref)
    after_assignment, after_active_ids, after_identity = _load_comparable_resource(after_ref)
    try:
        require_matching_input(input_identity, before_identity)
        require_matching_input(input_identity, after_identity)
    except ValueError as error:
        raise McpResourceContractError("comparison_input_identity_mismatch") from error
    comparison: AssignmentComparison = compare_assignments(
        before_assignment,
        after_assignment,
        service_targets,
        before_active_ids,
        after_active_ids,
    )
    coverage_summary = (
        ", ".join(
            f"{metric.target_hours:g}h city-count "
            f"{metric.before.city_coverage_rate:.1%}→"
            f"{metric.after.city_coverage_rate:.1%} "
            f"({metric.delta.city_coverage_rate:+.1%}), demand-weighted "
            f"{metric.before.demand_weighted_coverage_rate:.1%}→"
            f"{metric.after.demand_weighted_coverage_rate:.1%} "
            f"({metric.delta.demand_weighted_coverage_rate:+.1%})"
            for metric in comparison.coverage[:8]
        )
        or "none"
    )
    if len(comparison.coverage) > 8:
        coverage_summary = f"{coverage_summary}, +{len(comparison.coverage) - 8} more"
    cost_summary = (
        "unavailable"
        if comparison.before_cost is None or comparison.after_cost is None
        else f"{comparison.before_cost:.2f}→{comparison.after_cost:.2f} "
        f"({comparison.cost_delta or 0:+.2f})"
    )
    plan_comparison = NetworkPlanComparisonResource(
        prepared_input_relative_path=prepared_input_relative_path,
        input_identity=input_identity,
        before_ref=before_ref,
        after_ref=after_ref,
        comparison=comparison,
    )
    return _runtime().publish(
        plan_comparison.schema_version,
        plan_comparison,
        f"Compared {len(comparison.city_changes)} city assignments; selected "
        f"[{_bounded_id_summary(comparison.selected_warehouse_ids)}], removed "
        f"[{_bounded_id_summary(comparison.removed_warehouse_ids)}], affected "
        f"{len(comparison.affected_city_ids)}, reassigned "
        f"{len(comparison.reassigned_city_ids)}; cost {cost_summary}; coverage "
        f"{coverage_summary}.",
    )


def _load_ready_network(
    prepared_input_relative_path: str,
    ctx: Context,
) -> tuple[PreparedNetworkResource, PlanningInputIdentity]:
    prepared, identity = load_prepared_network_input(
        _runtime().require_workspace(ctx),
        prepared_input_relative_path,
    )
    if prepared.state != "ready":
        raise McpResourceContractError("prepared_network_input_not_ready")
    return prepared, identity


def _load_assignment_coverage_result(
    resource_ref: AssignmentResultResourceRef,
) -> tuple[
    AssignmentResult,
    list[str],
    str,
    Literal["baseline", "scenario", "facility"],
    PlanningInputIdentity,
]:
    """Adapt one solved domain result without choosing or recomputing it."""
    if resource_ref.resource_schema == "network_baseline.v2":
        baseline = _runtime().load_model(resource_ref, "network_baseline.v2", BaselineResult)
        return (
            baseline.assignment,
            baseline.active_warehouse_ids,
            baseline.label,
            "baseline",
            baseline.input_identity,
        )
    if resource_ref.resource_schema == "network_scenario.v2":
        scenario = _runtime().load_model(resource_ref, "network_scenario.v2", ScenarioResult)
        return (
            scenario.assignment,
            scenario.active_warehouse_ids,
            "scenario",
            "scenario",
            scenario.input_identity,
        )
    facility = _runtime().load_model(resource_ref, "facility_location_solution.v3", PMedianSolution)
    if facility.assignment is None:
        raise McpResourceContractError("coverage_assignment_required")
    if facility.status not in {"optimal", "feasible"}:
        raise McpResourceContractError("coverage_solution_not_deliverable")
    return (
        facility.assignment,
        facility.active_warehouse_ids,
        facility.status,
        "facility",
        facility.input_identity,
    )


def _publish_geojson(
    schema: str,
    value: NetworkComparisonGeoJson | NetworkCoverageGeoJson | NetworkDistributionGeoJson,
    description: str,
) -> CallToolResult:
    # A map source advertises only properties that exist for the exact result.
    # Keeping optional comparison fields as JSON null makes them look usable to
    # a style author even though every rendered feature will evaluate to the
    # Mapbox fallback.  The immutable GeoJSON and its derived profile must use
    # the same compact, non-null payload.
    payload = value.model_dump(mode="json", by_alias=True, exclude_none=True)
    if payload.get("type") != "FeatureCollection" or not isinstance(payload.get("features"), list):
        raise McpResourceContractError("geojson_feature_collection_required")
    result = _runtime().publish(
        schema,
        payload,
        description,
        mime_type="application/geo+json",
    )
    structured = result.structuredContent
    if structured is None:
        raise McpResourceContractError("map_data_result_missing")
    resource_ref = ResourceRef.model_validate(structured["resource_ref"])
    data_ref = GeoJsonResourceRef(
        server=resource_ref.server,
        uri=resource_ref.uri,
        resource_schema=resource_ref.resource_schema,
        profile=derive_geojson_profile(payload),
    )
    structured.pop("resource_ref", None)
    structured["data_ref"] = data_ref.model_dump(mode="json")
    return result


@mcp.tool(structured_output=True, annotations=CONTENT_ADDRESSED_RESOURCE_TOOL)
def prepare_network_distribution_map(
    prepared_input_relative_path: Annotated[str, Field(min_length=1, max_length=1024)],
    ctx: Context,
    include_candidates: bool = False,
    baseline_ref: Annotated[
        ResourceRef | None,
        Field(
            description=(
                "Optional exact network_baseline.v2 result for adding raw per-city "
                "assignment, distance, duration, and unit-cost properties."
            )
        ),
    ] = None,
) -> CallToolResult:
    """Publish raw GeoJSON facts for a separately authored map presentation."""
    prepared, input_identity = _load_ready_network(prepared_input_relative_path, ctx)
    normalized = NormalizedInputBatch(
        demand_cities=prepared.demand_cities,
        warehouses=prepared.warehouses,
        current_assignments=prepared.current_assignments,
        route_quotes=prepared.route_quotes,
        provided_route_facts=prepared.provided_route_facts,
        issues=prepared.issues,
    )
    baseline = (
        _runtime().load_model(baseline_ref, "network_baseline.v2", BaselineResult)
        if baseline_ref is not None
        else None
    )
    if baseline is not None:
        try:
            require_matching_input(input_identity, baseline.input_identity)
        except ValueError as error:
            raise McpResourceContractError("map_input_identity_mismatch") from error
    geojson = build_network_distribution_geojson(
        normalized,
        include_candidates=include_candidates,
        baseline=baseline,
    )
    demand_count = len(prepared.demand_cities)
    existing_count = sum(warehouse.is_existing for warehouse in prepared.warehouses)
    candidate_count = (
        sum(not warehouse.is_existing for warehouse in prepared.warehouses)
        if include_candidates
        else 0
    )
    summary = (
        f"Prepared interactive map data with {demand_count} demand cities, "
        f"{existing_count} existing warehouses, and {candidate_count} candidates."
    )
    result = _publish_geojson(geojson.schema_version, geojson, summary)
    structured = result.structuredContent
    if structured is None:
        raise McpResourceContractError("map_data_result_missing")
    structured.update(
        {
            "feature_count": len(geojson.features),
            "feature_counts": {
                "demand": demand_count,
                "existing_warehouses": existing_count,
                "candidate_warehouses": candidate_count,
            },
        }
    )
    return result


@mcp.tool(structured_output=True, annotations=WORKSPACE_REQUEST_TOOL)
def create_navigation_matrix_request(
    prepared_input_relative_path: Annotated[str, Field(min_length=1, max_length=1024)],
    warehouse_scope: Literal["existing_only", "all_warehouses"],
    output_relative_path: Annotated[
        str,
        Field(
            min_length=1,
            max_length=1024,
            description=(
                "Create-new JSON path directly under outputs/warehouse-network/requests/."
            ),
        ),
    ],
    ctx: Context,
    prior_route_matrix_ref: ResourceRef | None = None,
) -> NavigationMatrixRequestToolResult:
    """Write the exact billable navigation lanes for one prepared Workspace input."""
    output_relative_path = prepare_workspace_output_path(
        _runtime().require_workspace(ctx),
        output_relative_path,
        WorkspaceOutputKind.NAVIGATION_REQUEST,
    )
    prepared, input_identity = _load_ready_network(prepared_input_relative_path, ctx)
    warehouses = prepared.warehouses
    if warehouse_scope == "existing_only":
        warehouses = [warehouse for warehouse in warehouses if warehouse.is_existing]
    request = build_navigation_matrix_request(
        prepared.demand_cities,
        warehouses,
        warehouse_scope=warehouse_scope,
        input_identity=input_identity,
    )
    reused_rows = []
    if prior_route_matrix_ref is not None:
        prior = _runtime().load_model(
            prior_route_matrix_ref,
            "route_matrix.v2",
            ComposableRouteMatrix,
        )
        try:
            require_matching_input(input_identity, prior.input_identity)
        except ValueError as error:
            raise McpResourceContractError("navigation_prior_input_identity_mismatch") from error
        if prior.warehouse_scope != warehouse_scope:
            raise McpResourceContractError("navigation_route_matrix_scope_mismatch")
        reused_rows = [
            row for row in prior.rows if row.method == "navigation" and row.status == "ready"
        ]
        reusable_keys = {(row.origin_id, row.destination_id, row.layer) for row in reused_rows}
        request = request.model_copy(
            update={
                "routes": [
                    route
                    for route in request.routes
                    if (route.origin_id, route.destination_id, route.layer) not in reusable_keys
                ],
                "estimated_billable_elements": len(
                    [
                        route
                        for route in request.routes
                        if (route.origin_id, route.destination_id, route.layer) not in reusable_keys
                    ]
                ),
            }
        )
    if not request.routes:
        return NavigationMatrixRequestToolResult(
            summary="All required navigation lane facts are already available from the exact prior matrix.",
            state="ready",
            navigation_request_relative_path=None,
            input_identity=input_identity,
            warehouse_scope=warehouse_scope,
            route_count=0,
            estimated_billable_elements=0,
        )
    created = _runtime().create_workspace_model(
        ctx,
        output_relative_path,
        request,
        max_bytes=MAX_WORKSPACE_FILE_BYTES,
    )
    return NavigationMatrixRequestToolResult(
        summary=(
            f"Prepared {len(request.routes)} exact navigation lanes for {warehouse_scope}; "
            f"{len(reused_rows)} exact navigation facts reused, estimated billable route elements: "
            f"{request.estimated_billable_elements}."
        ),
        state="execution_required",
        navigation_request_relative_path=created.relative_path,
        input_identity=input_identity,
        warehouse_scope=warehouse_scope,
        route_count=len(request.routes),
        estimated_billable_elements=request.estimated_billable_elements,
    )


@mcp.tool(structured_output=True, annotations=CONTENT_ADDRESSED_RESOURCE_TOOL)
def prepare_route_matrix(
    prepared_input_relative_path: Annotated[str, Field(min_length=1, max_length=1024)],
    route_method: Annotated[
        Literal["haversine", "provided"],
        Field(
            description=(
                "Route method for the required route pairs. Haversine requires both "
                "detour_coefficient and average_speed_kph."
            )
        ),
    ],
    ctx: Context,
    warehouse_scope: Literal["existing_only", "all_warehouses"] = "all_warehouses",
    detour_coefficient: Annotated[
        float | None,
        Field(description=("Required when route_method is haversine; omit for provided routes.")),
    ] = None,
    average_speed_kph: Annotated[
        float | None,
        Field(description=("Required when route_method is haversine; omit for provided routes.")),
    ] = None,
    prior_route_matrix_ref: ResourceRef | None = None,
) -> Annotated[CallToolResult, RouteMatrixPreparationToolResult]:
    """Prepare one provided or explicitly assumed haversine route matrix."""
    prepared, input_identity = _load_ready_network(prepared_input_relative_path, ctx)
    warehouses = prepared.warehouses
    if warehouse_scope == "existing_only":
        warehouses = [warehouse for warehouse in warehouses if warehouse.is_existing]
    prior = (
        _runtime().load_model(
            prior_route_matrix_ref,
            "route_matrix.v2",
            ComposableRouteMatrix,
        )
        if prior_route_matrix_ref is not None
        else None
    )
    if route_method == "provided":
        matrix = _build_provided_route_matrix(
            prepared.demand_cities,
            warehouses,
            prepared.provided_route_facts,
            warehouse_scope=warehouse_scope,
            input_identity=input_identity,
        )
    else:
        if detour_coefficient is None or average_speed_kph is None:
            raise McpResourceContractError("haversine_route_parameters_required")
        matrix = build_route_matrix_with_reuse(
            prepared.demand_cities,
            warehouses,
            prior.rows if prior is not None else [],
            detour_coefficient,
            average_speed_kph,
            warehouse_scope=warehouse_scope,
            input_identity=input_identity,
        )
    stats = matrix.stats
    result = _runtime().publish(
        matrix.schema_version,
        matrix,
        f"Prepared {route_method} route matrix for {warehouse_scope}; "
        f"{getattr(stats, 'reused_pair_count', 0)} reused, "
        f"{getattr(stats, 'computed_pair_count', getattr(stats, 'provided_pair_count', 0))} "
        f"materialized, and {stats.missing_pair_count} missing pairs.",
    )
    if result.structuredContent is None:
        raise McpResourceContractError("route_matrix_result_missing")
    result.structuredContent["state"] = "ready"
    return result


@mcp.tool(structured_output=True, annotations=CONTENT_ADDRESSED_RESOURCE_TOOL)
def import_navigation_matrix(
    prepared_input_relative_path: Annotated[str, Field(min_length=1, max_length=1024)],
    navigation_matrix_relative_path: Annotated[
        str,
        Field(min_length=1, max_length=1024),
    ],
    ctx: Context,
    prior_route_matrix_ref: ResourceRef | None = None,
) -> CallToolResult:
    """Validate and publish provider-executed navigation facts from the Workspace."""
    workspace = _runtime().require_workspace(ctx)
    prepared, input_identity = _load_ready_network(prepared_input_relative_path, ctx)
    document = read_json_document(workspace, navigation_matrix_relative_path)
    try:
        supplied = NavigationMatrixResult.model_validate(document)
    except ValidationError as error:
        raise McpResourceContractError("navigation_matrix_result_invalid") from error
    try:
        require_matching_input(input_identity, supplied.input_identity)
    except ValueError as error:
        raise McpResourceContractError("navigation_matrix_input_identity_mismatch") from error
    prior = (
        _runtime().load_model(
            prior_route_matrix_ref,
            "route_matrix.v2",
            ComposableRouteMatrix,
        )
        if prior_route_matrix_ref is not None
        else None
    )
    if prior is not None:
        try:
            require_matching_input(input_identity, prior.input_identity)
        except ValueError as error:
            raise McpResourceContractError("navigation_prior_input_identity_mismatch") from error
        if prior.warehouse_scope != supplied.warehouse_scope:
            raise McpResourceContractError("navigation_route_matrix_scope_mismatch")
    warehouses = prepared.warehouses
    if supplied.warehouse_scope == "existing_only":
        warehouses = [warehouse for warehouse in warehouses if warehouse.is_existing]
    prior_rows = prior.rows if prior is not None else []
    matrix = _register_composable_navigation_matrix(
        prepared.demand_cities,
        warehouses,
        [*prior_rows, *supplied.rows],
        warehouse_scope=supplied.warehouse_scope,
        input_identity=input_identity,
    )
    if matrix.missing_routes:
        raise McpResourceContractError("navigation_matrix_incomplete")
    matrix = matrix.model_copy(
        update={
            "stats": matrix.stats.model_copy(
                update={
                    "reused_pair_count": len(prior_rows),
                    "registered_pair_count": len(supplied.rows),
                    "missing_pair_count": 0,
                    "complete": True,
                }
            )
        }
    )
    return _runtime().publish(
        matrix.schema_version,
        matrix,
        f"Registered navigation matrix with {len(prior_rows)} reused and "
        f"{len(supplied.rows)} provider-executed pair facts.",
    )


@mcp.tool(structured_output=True, annotations=CONTENT_ADDRESSED_RESOURCE_TOOL)
def plan_cost_matrix(
    prepared_input_relative_path: Annotated[str, Field(min_length=1, max_length=1024)],
    warehouse_scope: Literal["existing_only", "all_warehouses"],
    ctx: Context,
    cost_policy: CostPolicySelection | None = None,
    route_matrix_ref: ResourceRef | None = None,
    prior_cost_matrix_ref: ResourceRef | None = None,
) -> Annotated[CallToolResult, CostMatrixPlanningToolResult]:
    """Build quote-first costs with explicit or full-quote-mean fallback policy."""
    prepared, input_identity = _load_ready_network(prepared_input_relative_path, ctx)
    warehouses = prepared.warehouses
    if warehouse_scope == "existing_only":
        warehouses = [warehouse for warehouse in warehouses if warehouse.is_existing]
    calculation_policy = None
    calculation_rule_source = None
    calculation_rule_evidence = None
    if isinstance(cost_policy, ExplicitCostPolicy):
        calculation_policy = CostCalculationPolicy(rules=cost_policy.rules)
        calculation_rule_source = "explicit"
    elif isinstance(cost_policy, ObservedQuoteMeanCostPolicy):
        calculation_policy, calculation_rule_evidence = derive_observed_quote_mean_cost_policy(
            prepared.demand_cities,
            warehouses,
            prepared.route_quotes,
        )
        calculation_rule_source = "observed_quote_mean"
    route_matrix = (
        _runtime().load_model(
            route_matrix_ref,
            "route_matrix.v2",
            ComposableRouteMatrix,
        )
        if route_matrix_ref is not None
        else None
    )
    if route_matrix is not None:
        try:
            require_matching_input(input_identity, route_matrix.input_identity)
        except ValueError as error:
            raise McpResourceContractError("cost_route_input_identity_mismatch") from error
    prior = (
        _runtime().load_model(
            prior_cost_matrix_ref,
            "cost_matrix.v2",
            CostMatrix,
        )
        if prior_cost_matrix_ref is not None
        else None
    )
    matrix = _build_composable_cost_matrix(
        prepared.demand_cities,
        warehouses,
        prepared.route_quotes,
        calculation_policy,
        route_matrix,
        prior.rows if prior is not None else None,
        warehouse_scope=warehouse_scope,
        input_identity=input_identity,
        calculation_rule_source=calculation_rule_source,
        calculation_rule_evidence=calculation_rule_evidence,
    )
    stats = matrix.stats
    policy_summary = ""
    if calculation_rule_evidence is not None:
        policy_summary = "; observed quote means: " + ", ".join(
            f"{rule.layer}={rule.mean_cost_per_demand_unit:.6f} {rule.currency} "
            f"from {rule.quote_count} quotes"
            for rule in calculation_rule_evidence.rules
        )
    summary = (
        "Built cost matrix with "
        f"{stats.reused_pair_count} reused, "
        f"{stats.computed_pair_count} computed, and "
        f"{stats.missing_pair_count} missing lane costs{policy_summary}."
    )
    published = _runtime().publish(
        matrix.schema_version,
        matrix,
        summary,
    )
    if published.structuredContent is None:
        raise McpResourceContractError("cost_matrix_result_missing")
    resource_ref = ResourceRef.model_validate(published.structuredContent["resource_ref"])
    result = CostMatrixPlanningToolResult(
        summary=summary,
        resource_ref=resource_ref,
        input_identity=input_identity,
        calculation_rule_source=matrix.calculation_rule_source,
        calculation_rule_evidence=matrix.calculation_rule_evidence,
        expected_pair_count=stats.expected_pair_count,
        reused_pair_count=stats.reused_pair_count,
        computed_pair_count=stats.computed_pair_count,
        missing_pair_count=stats.missing_pair_count,
    )
    published.structuredContent = result.model_dump(mode="json")
    return published


@mcp.tool(structured_output=True, annotations=CONTENT_ADDRESSED_RESOURCE_TOOL)
def evaluate_network_baseline(
    prepared_input_relative_path: Annotated[str, Field(min_length=1, max_length=1024)],
    route_matrix_ref: ResourceRef,
    objective: Literal["min_time", "min_cost"],
    service_targets: Annotated[list[float], Field(min_length=1, max_length=32)],
    ctx: Context,
    coverage_mode: Literal["auto", "actual_current", "optimized_existing_footprint"] = "auto",
    cost_matrix_ref: ResourceRef | None = None,
) -> Annotated[CallToolResult, NetworkBaselineResourceToolResult]:
    """Evaluate an actual or optimized-existing baseline.

    The default auto mode selects actual_current only when the prepared input
    contains current assignments; otherwise it selects the optimized existing
    footprint without making the model inspect or guess input contents.
    """
    if any(target <= 0 for target in service_targets):
        raise McpResourceContractError("baseline_service_targets_invalid")
    prepared, input_identity = _load_ready_network(prepared_input_relative_path, ctx)
    routes = _runtime().load_model(
        route_matrix_ref,
        "route_matrix.v2",
        ComposableRouteMatrix,
    )
    costs = (
        _runtime().load_model(cost_matrix_ref, "cost_matrix.v2", CostMatrix)
        if cost_matrix_ref is not None
        else None
    )
    try:
        require_matching_input(input_identity, routes.input_identity)
        if costs is not None:
            require_matching_input(input_identity, costs.input_identity)
    except ValueError as error:
        raise McpResourceContractError("baseline_input_identity_mismatch") from error
    if objective == "min_cost" and costs is None:
        raise McpResourceContractError("min_cost_baseline_requires_cost_matrix")
    active_ids = {
        warehouse.warehouse_id for warehouse in prepared.warehouses if warehouse.is_existing
    }
    resolved_coverage_mode = (
        "actual_current"
        if coverage_mode == "auto" and prepared.current_assignments
        else "optimized_existing_footprint"
        if coverage_mode == "auto"
        else coverage_mode
    )
    if resolved_coverage_mode == "actual_current":
        if not prepared.current_assignments:
            raise McpResourceContractError("current_assignments_required")
        assignment = solve_current_assignment(
            prepared.demand_cities,
            prepared.warehouses,
            prepared.current_assignments,
            routes,
            costs,
            objective,
        )
        label: Literal["actual_current", "optimized_existing_footprint"] = "actual_current"
    else:
        assignment = solve_assignment(
            prepared.demand_cities,
            prepared.warehouses,
            routes,
            costs,
            objective,
            active_ids,
        )
        label = "optimized_existing_footprint"
    ordered_targets = sorted(set(service_targets))
    coverage, detail_target, uncovered = _baseline_coverage_projection(
        assignment,
        prepared.demand_cities,
        prepared.warehouses,
        ordered_targets,
    )
    baseline = BaselineResult(
        label=label,
        active_warehouse_ids=sorted(active_ids),
        assignment=assignment,
        service=service_metrics(assignment, ordered_targets),
        coverage=coverage,
        cost=summarize_assignment_cost(assignment, costs) if costs is not None else None,
        notice_code=None,
        input_identity=input_identity,
    )
    label_text = (
        "actual current assignment"
        if label == "actual_current"
        else "optimized existing-warehouse footprint"
    )
    message = (
        f"Evaluated the {label_text}; active warehouses "
        f"{len(baseline.active_warehouse_ids)}, cost "
        f"{_cost_metric_summary(baseline.cost)}, service "
        f"{_coverage_metric_summary(coverage)}; uncovered at "
        f"{detail_target:g}h: {len(uncovered)} cities."
    )
    result = _runtime().publish(baseline.schema_version, baseline, message)
    if result.structuredContent is None:
        raise McpResourceContractError("baseline_result_missing")
    result.structuredContent.update(
        {
            "coverage_metrics": [metric.model_dump(mode="json") for metric in coverage],
            "detail_target_hours": detail_target,
            "uncovered_city_count": len(uncovered),
            "uncovered_cities": [item.model_dump(mode="json") for item in uncovered[:10]],
            "uncovered_cities_truncated": len(uncovered) > 10,
        }
    )
    return result


def _load_facility_scenario_inputs(
    prepared_input_relative_path: str,
    route_matrix_ref: ResourceRef,
    cost_matrix_ref: ResourceRef | None,
    scenario: ScenarioSpec,
    ctx: Context,
) -> tuple[
    PreparedNetworkResource,
    PlanningInputIdentity,
    ComposableRouteMatrix,
    CostMatrix | None,
    list[float],
    set[str],
    set[str],
]:
    """Load and validate immutable inputs shared by both scenario tools."""
    if (
        not scenario.service_targets
        or len(scenario.service_targets) > 32
        or any(target <= 0 for target in scenario.service_targets)
    ):
        raise McpResourceContractError("scenario_service_targets_invalid")
    prepared, input_identity = _load_ready_network(prepared_input_relative_path, ctx)
    routes = _runtime().load_model(
        route_matrix_ref,
        "route_matrix.v2",
        ComposableRouteMatrix,
    )
    costs = (
        _runtime().load_model(cost_matrix_ref, "cost_matrix.v2", CostMatrix)
        if cost_matrix_ref is not None
        else None
    )
    try:
        require_matching_input(input_identity, routes.input_identity)
        if costs is not None:
            require_matching_input(input_identity, costs.input_identity)
    except ValueError as error:
        raise McpResourceContractError("scenario_input_identity_mismatch") from error
    if scenario.objective == "min_cost" and costs is None:
        raise McpResourceContractError("min_cost_scenario_requires_cost_matrix")
    warehouse_by_id = {warehouse.warehouse_id: warehouse for warehouse in prepared.warehouses}
    add_ids = set(scenario.add_warehouse_ids)
    remove_ids = set(scenario.remove_warehouse_ids)
    for relocation in scenario.relocations:
        remove_ids.add(relocation.remove_warehouse_id)
        add_ids.add(relocation.add_warehouse_id)
    if len(add_ids) > 256 or len(remove_ids) > 256:
        raise McpResourceContractError("scenario_warehouse_change_limit_exceeded")
    unknown = (add_ids | remove_ids) - set(warehouse_by_id)
    if unknown:
        raise McpResourceContractError("scenario_unknown_warehouses")
    if any(warehouse_by_id[item].is_existing for item in add_ids):
        raise McpResourceContractError("scenario_add_requires_candidate_warehouse")
    if any(not warehouse_by_id[item].is_existing for item in remove_ids):
        raise McpResourceContractError("scenario_remove_requires_existing_warehouse")
    if add_ids & remove_ids:
        raise McpResourceContractError("scenario_add_remove_conflict")
    return (
        prepared,
        input_identity,
        routes,
        costs,
        sorted(set(scenario.service_targets)),
        add_ids,
        remove_ids,
    )


def _evaluate_facility_change(
    prepared: PreparedNetworkResource,
    input_identity: PlanningInputIdentity,
    routes: ComposableRouteMatrix,
    costs: CostMatrix | None,
    scenario: ScenarioSpec,
    base_active_ids: set[str],
    add_ids: set[str],
    remove_ids: set[str],
    ordered_targets: list[float],
) -> ScenarioResult:
    """Apply one validated change to an explicit active warehouse footprint."""
    active_ids = set(base_active_ids)
    active_ids.difference_update(remove_ids)
    active_ids.update(add_ids)
    invalid_upstreams = sorted(
        warehouse.warehouse_id
        for warehouse in prepared.warehouses
        if warehouse.warehouse_id in active_ids
        and warehouse.upstream_center_id is not None
        and warehouse.upstream_center_id not in active_ids
    )
    if invalid_upstreams:
        raise McpResourceContractError("scenario_active_upstream_required")
    assignment = solve_assignment(
        prepared.demand_cities,
        prepared.warehouses,
        routes,
        costs,
        scenario.objective,
        active_ids,
    )
    return ScenarioResult(
        active_warehouse_ids=sorted(active_ids),
        assignment=assignment,
        cost=summarize_assignment_cost(assignment, costs) if costs is not None else None,
        service=service_metrics(assignment, ordered_targets),
        warehouse_changes=WarehouseChanges(
            added=sorted(add_ids),
            removed=sorted(remove_ids),
        ),
        input_identity=input_identity,
    )


@mcp.tool(structured_output=True, annotations=CONTENT_ADDRESSED_RESOURCE_TOOL)
def assess_facility_change(
    prepared_input_relative_path: Annotated[str, Field(min_length=1, max_length=1024)],
    route_matrix_ref: ResourceRef,
    before_ref: ComparableNetworkResultRef,
    scenario: ScenarioSpec,
    ctx: Context,
    cost_matrix_ref: ResourceRef | None = None,
) -> Annotated[CallToolResult, FacilityChangeAssessmentToolResult]:
    """Apply and compare one bounded facility change in a single call.

    Use this tool when a user adds, removes, or relocates facilities and the
    exact normalized input, route matrix, prior result, scenario, and optional
    cost matrix are already available. It returns both the changed scenario and
    its comparison without requiring a second model-selected Tool call.
    """
    prepared, input_identity, routes, costs, targets, add_ids, remove_ids = (
        _load_facility_scenario_inputs(
            prepared_input_relative_path,
            route_matrix_ref,
            cost_matrix_ref,
            scenario,
            ctx,
        )
    )
    before_assignment, before_active_ids, before_identity = _load_comparable_resource(before_ref)
    try:
        require_matching_input(input_identity, before_identity)
    except ValueError as error:
        raise McpResourceContractError("scenario_before_input_identity_mismatch") from error
    known_warehouse_ids = {warehouse.warehouse_id for warehouse in prepared.warehouses}
    if before_active_ids - known_warehouse_ids:
        raise McpResourceContractError("scenario_before_warehouse_unknown")
    if remove_ids - before_active_ids:
        raise McpResourceContractError("scenario_remove_requires_active_warehouse")
    if add_ids & before_active_ids:
        raise McpResourceContractError("scenario_add_requires_inactive_warehouse")
    after = _evaluate_facility_change(
        prepared,
        input_identity,
        routes,
        costs,
        scenario,
        before_active_ids,
        add_ids,
        remove_ids,
        targets,
    )
    comparison = compare_assignments(
        before_assignment,
        after.assignment,
        targets,
        before_active_ids,
        set(after.active_warehouse_ids),
    )
    coverage_summary = (
        ", ".join(
            f"{metric.target_hours:g}h city-count "
            f"{metric.before.city_coverage_rate:.1%}→{metric.after.city_coverage_rate:.1%} "
            f"({metric.delta.city_coverage_rate:+.1%}), demand-weighted "
            f"{metric.before.demand_weighted_coverage_rate:.1%}→"
            f"{metric.after.demand_weighted_coverage_rate:.1%} "
            f"({metric.delta.demand_weighted_coverage_rate:+.1%})"
            for metric in comparison.coverage[:8]
        )
        or "none"
    )
    if len(comparison.coverage) > 8:
        coverage_summary = f"{coverage_summary}, +{len(comparison.coverage) - 8} more"
    cost_complete = comparison.before_cost is not None and comparison.after_cost is not None
    cost_summary = (
        f"{comparison.before_cost:.2f}→{comparison.after_cost:.2f} "
        f"({comparison.cost_delta or 0:+.2f})"
        if cost_complete
        else "unavailable"
    )
    summary = (
        f"Assessed one facility change against the exact before result; active warehouses "
        f"{len(after.active_warehouse_ids)}, added [{_bounded_id_summary(sorted(add_ids))}], "
        f"removed [{_bounded_id_summary(sorted(remove_ids))}], affected "
        f"{len(comparison.affected_city_ids)}, reassigned "
        f"{len(comparison.reassigned_city_ids)}; cost {cost_summary}; coverage "
        f"{coverage_summary}."
    )

    scenario_published = _runtime().publish(after.schema_version, after, summary)
    if scenario_published.structuredContent is None:
        raise McpResourceContractError("facility_change_result_missing")
    scenario_ref = NetworkScenarioResourceRef.model_validate(
        scenario_published.structuredContent["resource_ref"]
    )
    plan_comparison = NetworkPlanComparisonResource(
        prepared_input_relative_path=prepared_input_relative_path,
        input_identity=input_identity,
        before_ref=before_ref,
        after_ref=ComparableNetworkResultRef.model_validate(scenario_ref.model_dump(mode="json")),
        comparison=comparison,
    )
    comparison_published = _runtime().publish(
        plan_comparison.schema_version,
        plan_comparison,
        summary,
    )
    if comparison_published.structuredContent is None:
        raise McpResourceContractError("facility_change_result_missing")
    affected_city_changes = [change for change in comparison.city_changes if change.affected]
    result = FacilityChangeAssessmentToolResult(
        summary=summary,
        scenario_ref=scenario_ref,
        plan_comparison_ref=NetworkPlanComparisonResourceRef.model_validate(
            comparison_published.structuredContent["resource_ref"]
        ),
        active_warehouse_count=len(after.active_warehouse_ids),
        added_warehouse_ids=sorted(add_ids),
        removed_warehouse_ids=sorted(remove_ids),
        cost=FacilityChangeCostComparison(
            currency=costs.currency if costs is not None else None,
            before_total=comparison.before_cost,
            after_total=comparison.after_cost,
            delta=comparison.cost_delta,
            complete=cost_complete,
        ),
        coverage=comparison.coverage,
        affected_city_count=len(comparison.affected_city_ids),
        affected_city_ids=comparison.affected_city_ids[:10],
        affected_city_ids_truncated=len(comparison.affected_city_ids) > 10,
        affected_city_changes=affected_city_changes[:10],
        affected_city_changes_truncated=len(affected_city_changes) > 10,
        reassigned_city_count=len(comparison.reassigned_city_ids),
        reassigned_city_ids=comparison.reassigned_city_ids[:10],
        reassigned_city_ids_truncated=len(comparison.reassigned_city_ids) > 10,
    )
    return CallToolResult(
        content=[
            TextContent(type="text", text=summary),
            *scenario_published.content[1:],
            *comparison_published.content[1:],
        ],
        structuredContent=result.model_dump(mode="json"),
    )


@mcp.tool(structured_output=True, annotations=BOUNDED_LOCAL_COMPUTE_TOOL)
def solve_p_median(
    prepared_input_relative_path: Annotated[str, Field(min_length=1, max_length=1024)],
    route_matrix_ref: ResourceRef,
    cost_matrix_ref: ResourceRef,
    number_to_open: Annotated[
        int,
        Field(
            ge=0,
            description=(
                "Count of candidate new warehouses selected; excludes existing warehouses."
            ),
        ),
    ],
    existing_warehouse_policy: Annotated[
        ExistingWarehousePolicy,
        Field(
            description=(
                "Explicit existing-site policy. Use keep_all_existing to keep every "
                "existing warehouse open. Use allow_closure only with the exact existing "
                "warehouse IDs the user authorized to close; all other existing "
                "warehouses remain fixed."
            )
        ),
    ],
    service_targets: Annotated[list[float], Field(min_length=1, max_length=32)],
    time_limit_seconds: Annotated[float, Field(gt=0, le=300)],
    ctx: Context,
    service_constraints: list[ServiceCoverageConstraint] | None = None,
) -> Annotated[CallToolResult, PMedianSolutionToolResult]:
    """Solve finite-candidate min-cost p-median under explicit existing-site policy."""
    if any(target <= 0 for target in service_targets):
        raise McpResourceContractError("p_median_service_targets_invalid")
    prepared, input_identity = _load_ready_network(prepared_input_relative_path, ctx)
    routes = _runtime().load_model(
        route_matrix_ref,
        "route_matrix.v2",
        ComposableRouteMatrix,
    )
    costs = _runtime().load_model(
        cost_matrix_ref,
        "cost_matrix.v2",
        CostMatrix,
    )
    try:
        require_matching_input(input_identity, routes.input_identity)
        require_matching_input(input_identity, costs.input_identity)
    except ValueError as error:
        raise McpResourceContractError("p_median_input_identity_mismatch") from error
    route_validation = _validate_route_matrix_model(
        prepared.demand_cities,
        prepared.warehouses,
        routes,
    )
    if not route_validation.valid:
        raise McpResourceContractError("p_median_route_matrix_incomplete")
    if costs.warehouse_scope != "all_warehouses" or costs.missing_routes:
        raise McpResourceContractError("p_median_cost_matrix_incomplete")
    constraints = [
        (constraint.target_hours, constraint.minimum_coverage)
        for constraint in service_constraints or []
    ]
    existing_ids = {
        warehouse.warehouse_id for warehouse in prepared.warehouses if warehouse.is_existing
    }
    if existing_warehouse_policy.mode == "keep_all_existing":
        fixed_existing_ids = existing_ids
        optional_existing_ids: set[str] = set()
    else:
        if len(existing_warehouse_policy.closable_existing_ids) != len(
            set(existing_warehouse_policy.closable_existing_ids)
        ):
            raise McpResourceContractError("existing_policy_closable_ids_duplicate")
        optional_existing_ids = set(existing_warehouse_policy.closable_existing_ids)
        fixed_existing_ids = existing_ids - optional_existing_ids
    try:
        solved, _branches, timed_out = enumerate_p_median(
            prepared.demand_cities,
            prepared.warehouses,
            routes,
            costs,
            number_to_open,
            fixed_existing_ids,
            optional_existing_ids,
            time_limit_seconds,
            constraints,
        )
    except SolverUnavailable as error:
        solved = None
        timed_out = False
        unavailable_message = str(error)
    else:
        unavailable_message = None
    solution_coverage: list[CoverageMetricSummary] = []
    if solved is None:
        status = (
            "timeout" if timed_out else ("unavailable" if unavailable_message else "infeasible")
        )
        solution = PMedianSolution(
            status=status,
            active_warehouse_ids=[],
            opened_candidate_ids=[],
            closed_existing_ids=[],
            optimality="not_available",
            message=unavailable_message or "No feasible p-median solution was found.",
            input_identity=input_identity,
        )
    else:
        solution_coverage = coverage_metrics(
            solved.assignment,
            sorted(set(service_targets)),
        )
        solution = PMedianSolution(
            status="timeout" if timed_out else "optimal",
            active_warehouse_ids=solved.active_warehouse_ids,
            opened_candidate_ids=solved.opened_candidate_ids,
            closed_existing_ids=solved.closed_existing_ids,
            assignment=solved.assignment,
            objective_value=solved.objective_value,
            cost=summarize_assignment_cost(solved.assignment, costs),
            service=service_metrics(
                solved.assignment,
                sorted(set(service_targets)),
            ),
            optimality="feasible_only" if timed_out else "proven",
            message="The solver returned a feasible solution before the time limit."
            if timed_out
            else None,
            input_identity=input_identity,
        )
    summary = (
        f"p-median status is {solution.status}; active warehouses "
        f"{len(solution.active_warehouse_ids)}, opened "
        f"[{_bounded_id_summary(solution.opened_candidate_ids)}], closed "
        f"[{_bounded_id_summary(solution.closed_existing_ids)}]; cost "
        f"{_cost_metric_summary(solution.cost)}, coverage "
        f"{_coverage_metric_summary(solution_coverage) if solution.assignment is not None else 'none'}."
    )
    published = _runtime().publish(
        solution.schema_version,
        solution,
        summary,
    )
    if published.structuredContent is None:
        raise McpResourceContractError("p_median_result_missing")
    resource_ref = ResourceRef.model_validate(published.structuredContent["resource_ref"])
    result = PMedianSolutionToolResult(
        summary=summary,
        resource_ref=resource_ref,
        input_identity=input_identity,
        status=solution.status,
        optimality=solution.optimality,
        active_warehouse_count=len(solution.active_warehouse_ids),
        opened_candidate_ids=solution.opened_candidate_ids,
        closed_existing_ids=solution.closed_existing_ids,
        cost=solution.cost,
        coverage=solution_coverage,
    )
    published.structuredContent = result.model_dump(mode="json")
    return published


def _load_final_delivery_inputs(
    plan_comparison_ref: NetworkPlanComparisonResourceRef,
    ctx: Context,
) -> tuple[
    PreparedNetworkResource,
    NormalizedInputBatch,
    BaselineResult,
    PMedianSolution,
    AssignmentComparison,
]:
    plan_comparison = _runtime().load_model(
        plan_comparison_ref,
        "network_plan_comparison.v1",
        NetworkPlanComparisonResource,
    )
    if plan_comparison.before_ref.resource_schema != "network_baseline.v2":
        raise McpResourceContractError("delivery_before_baseline_required")
    if plan_comparison.after_ref.resource_schema != "facility_location_solution.v3":
        raise McpResourceContractError("delivery_after_facility_solution_required")
    prepared, input_identity = _load_ready_network(
        plan_comparison.prepared_input_relative_path,
        ctx,
    )
    baseline = _runtime().load_model(
        plan_comparison.before_ref,
        "network_baseline.v2",
        BaselineResult,
    )
    facility = _runtime().load_model(
        plan_comparison.after_ref,
        "facility_location_solution.v3",
        PMedianSolution,
    )
    try:
        require_matching_input(plan_comparison.input_identity, input_identity)
        require_matching_input(input_identity, baseline.input_identity)
        require_matching_input(input_identity, facility.input_identity)
    except ValueError as error:
        raise McpResourceContractError("delivery_input_identity_mismatch") from error
    normalized = NormalizedInputBatch(
        demand_cities=prepared.demand_cities,
        warehouses=prepared.warehouses,
        current_assignments=prepared.current_assignments,
        route_quotes=prepared.route_quotes,
        provided_route_facts=prepared.provided_route_facts,
        issues=prepared.issues,
    )
    return prepared, normalized, baseline, facility, plan_comparison.comparison


def _write_final_delivery_json_bundle(
    bundle: NetworkComparisonMapBundle,
    output_relative_path: str,
    ctx: Context,
    summary: str,
) -> CallToolResult:
    output_relative_path = prepare_workspace_output_path(
        _runtime().require_workspace(ctx),
        output_relative_path,
        WorkspaceOutputKind.DELIVERY_JSON,
    )
    created = _runtime().create_workspace_model(
        ctx,
        output_relative_path,
        bundle,
        max_bytes=MAX_WORKSPACE_FILE_BYTES,
    )
    structured = NetworkFinalArtifactToolResult(
        summary=summary,
        artifact=NetworkFinalArtifactDescriptor(
            schema=bundle.schema_version,
            displayName="Warehouse network comparison data",
            mimeType="application/json",
            workspaceRelativePath=created.relative_path,
            byteSize=created.byte_size,
        ),
    )
    return CallToolResult(
        content=[TextContent(type="text", text=summary)],
        structuredContent=structured.model_dump(mode="json", by_alias=True),
    )


def _write_final_delivery_markdown(
    markdown: str,
    output_relative_path: str,
    ctx: Context,
    summary: str,
) -> CallToolResult:
    output_relative_path = prepare_workspace_output_path(
        _runtime().require_workspace(ctx),
        output_relative_path,
        WorkspaceOutputKind.DELIVERY_MARKDOWN,
    )
    content = markdown.encode("utf-8")
    try:
        created = _runtime().create_workspace_file(
            ctx,
            output_relative_path,
            content,
            max_bytes=MAX_WORKSPACE_FILE_BYTES,
        )
    except McpResourceContractError:
        raise
    except (OSError, ValueError) as error:
        raise McpResourceContractError("workspace_file_invalid") from error
    structured = NetworkFinalArtifactToolResult(
        summary=summary,
        artifact=NetworkFinalArtifactDescriptor(
            schema=NETWORK_PLANNING_MARKDOWN_SCHEMA,
            displayName="Warehouse network planning report",
            mimeType="text/markdown",
            workspaceRelativePath=created.relative_path,
            byteSize=created.byte_size,
        ),
    )
    encoded_path = urllib.parse.quote(created.relative_path, safe="/-._~")
    link_text = f"正式简报已生成：[下载中文 Markdown 简报]({encoded_path})"
    return CallToolResult(
        content=[TextContent(type="text", text=link_text)],
        structuredContent=structured.model_dump(mode="json", by_alias=True),
    )


@mcp.tool(structured_output=True, annotations=CONTENT_ADDRESSED_RESOURCE_TOOL)
def prepare_network_comparison_map(
    plan_comparison_ref: NetworkPlanComparisonResourceRef,
    ctx: Context,
) -> CallToolResult:
    """Publish raw baseline-versus-plan GeoJSON for a separately authored map."""
    prepared, normalized, baseline, facility, comparison = _load_final_delivery_inputs(
        plan_comparison_ref,
        ctx,
    )
    bundle = build_network_comparison_map_bundle(
        normalized,
        baseline,
        facility,
        comparison,
        country_code=prepared.country_code,
    )
    geojson = NetworkComparisonGeoJson(features=bundle.geojson.features)
    summary = (
        f"Prepared an interactive comparison map with {len(geojson.features)} "
        "features from the validated baseline and facility result."
    )
    result = _publish_geojson(geojson.schema_version, geojson, summary)
    structured = result.structuredContent
    if structured is None:
        raise McpResourceContractError("map_data_result_missing")
    structured.update(
        {
            "feature_count": len(geojson.features),
        }
    )
    return result


@mcp.tool(structured_output=True, annotations=CONTENT_ADDRESSED_RESOURCE_TOOL)
def prepare_network_coverage_map(
    prepared_input_relative_path: Annotated[str, Field(min_length=1, max_length=1024)],
    assignment_result_ref: AssignmentResultResourceRef,
    ctx: Context,
) -> CallToolResult:
    """Publish all straight-line coverage facts for one exact solved result."""
    prepared, input_identity = _load_ready_network(prepared_input_relative_path, ctx)
    assignment, active_ids, result_label, scenario, result_identity = (
        _load_assignment_coverage_result(assignment_result_ref)
    )
    try:
        require_matching_input(input_identity, result_identity)
    except ValueError as error:
        raise McpResourceContractError("coverage_map_input_identity_mismatch") from error
    normalized = NormalizedInputBatch(
        demand_cities=prepared.demand_cities,
        warehouses=prepared.warehouses,
        current_assignments=prepared.current_assignments,
        route_quotes=prepared.route_quotes,
        provided_route_facts=prepared.provided_route_facts,
        issues=prepared.issues,
    )
    geojson = build_network_coverage_geojson(
        normalized,
        assignment,
        active_ids,
        result_label=result_label,
        scenario=scenario,
    )
    result = _publish_geojson(
        geojson.schema_version,
        geojson,
        f"Prepared {len(geojson.features)} map features, including every assigned city coverage line.",
    )
    structured = result.structuredContent
    if structured is None:
        raise McpResourceContractError("map_data_result_missing")
    structured.update({"feature_count": len(geojson.features)})
    return result


@mcp.tool(structured_output=True, annotations=FINAL_WORKSPACE_DELIVERY_TOOL)
def render_network_comparison_map(
    plan_comparison_ref: NetworkPlanComparisonResourceRef,
    output_relative_path: Annotated[
        str,
        Field(
            min_length=1,
            max_length=1024,
            description=(
                "Create-new JSON path directly under outputs/warehouse-network/deliverables/."
            ),
        ),
    ],
    ctx: Context,
) -> CallToolResult:
    """Create a self-contained baseline-versus-facility map JSON file."""
    prepared, normalized, baseline, facility, comparison = _load_final_delivery_inputs(
        plan_comparison_ref,
        ctx,
    )
    bundle = build_network_comparison_map_bundle(
        normalized,
        baseline,
        facility,
        comparison,
        country_code=prepared.country_code,
    )
    return _write_final_delivery_json_bundle(
        bundle,
        output_relative_path,
        ctx,
        "Created the self-contained warehouse network comparison map.",
    )


@mcp.tool(structured_output=True, annotations=FINAL_WORKSPACE_DELIVERY_TOOL)
def publish_network_planning_report(
    report_input: NetworkReportInput,
    output_relative_path: Annotated[
        str,
        Field(
            min_length=1,
            max_length=1024,
            description=(
                "Create-new Markdown path directly under "
                "outputs/warehouse-network/deliverables/."
            ),
        ),
    ],
    ctx: Context,
) -> CallToolResult:
    """Create a Markdown brief for a baseline assessment or plan comparison."""
    if report_input.mode == "baseline":
        prepared, input_identity = _load_ready_network(
            report_input.prepared_input_relative_path,
            ctx,
        )
        normalized = NormalizedInputBatch(
            demand_cities=prepared.demand_cities,
            warehouses=prepared.warehouses,
            current_assignments=prepared.current_assignments,
            route_quotes=prepared.route_quotes,
            provided_route_facts=prepared.provided_route_facts,
            issues=prepared.issues,
        )
        baseline = _runtime().load_model(
            report_input.baseline_ref,
            "network_baseline.v2",
            BaselineResult,
        )
        try:
            require_matching_input(input_identity, baseline.input_identity)
        except ValueError as error:
            raise McpResourceContractError("report_input_identity_mismatch") from error
        bundle = build_network_baseline_assessment_report_bundle(
            normalized,
            baseline,
            country_code=prepared.country_code,
        )
        markdown = render_network_baseline_assessment_markdown(bundle)
    else:
        prepared, normalized, baseline, facility, comparison = _load_final_delivery_inputs(
            report_input.plan_comparison_ref,
            ctx,
        )
        bundle = build_network_planning_report_bundle(
            normalized,
            baseline,
            facility,
            comparison,
            country_code=prepared.country_code,
        )
        markdown = render_network_planning_report_markdown(bundle)
    return _write_final_delivery_markdown(
        markdown,
        output_relative_path,
        ctx,
        "已生成仓网分析中文 Markdown 简报。",
    )


def main() -> None:
    parser = argparse.ArgumentParser(description="Supply-chain network planning MCP server")
    parser.add_argument(
        "--transport",
        choices=("stdio", "streamable-http"),
        default="stdio",
    )
    args = parser.parse_args()

    global _workspace_root, _profile_state_root, _supply_chain_resources
    _workspace_root = Path.cwd().resolve(strict=True)
    _profile_state_root = Path(os.environ.get("CODEX_HOME", _workspace_root / ".codex")).resolve()
    _supply_chain_resources = SupplyChainResources(_workspace_root, _profile_state_root)
    if args.transport == "stdio":
        asyncio.run(run_stdio())
    else:
        mcp.run(transport=args.transport)


async def run_stdio() -> None:
    initialization_options = mcp._mcp_server.create_initialization_options(
        experimental_capabilities={SANDBOX_STATE_META_CAPABILITY: {}},
    )
    async with stdio_server() as streams:
        await mcp._mcp_server.run(streams[0], streams[1], initialization_options)


if __name__ == "__main__":
    main()
