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
from pydantic import Field, ValidationError

from .map_service import (
    NetworkComparisonGeoJson,
    NetworkComparisonMapBundle,
    build_network_comparison_map_bundle,
    build_network_comparison_map_card_handoff,
    build_network_distribution_geojson,
    build_network_distribution_map_card_handoff,
)
from .matrix import build_cost_matrix as _build_composable_cost_matrix
from .matrix import build_provided_route_matrix as _build_provided_route_matrix
from .matrix import build_route_matrix_with_reuse
from .matrix import plan_route_matrix as _plan_composable_route_matrix
from .matrix import register_navigation_route_matrix as _register_composable_navigation_matrix
from .matrix import validate_route_matrix as _validate_route_matrix_model
from .matrix_models import CostCalculationPolicy, CostMatrix
from .matrix_models import RouteMatrix as ComposableRouteMatrix
from .mcp_contracts import MapResourceRef, ResourceRef
from .mcp_resources import McpResourceContractError, McpResourceRuntime, bind_runtime
from .models import (
    NetworkBaselineResourceToolResult,
    NetworkFinalArtifactDescriptor,
    NetworkFinalArtifactToolResult,
    NetworkReportInput,
    PreparedNetworkResource,
    UncoveredCitySummary,
)
from .network_models import (
    DemandCityRecord,
    NormalizedInputBatch,
    WarehouseRecord,
)
from .optimization_models import (
    AssignmentComparison,
    AssignmentResult,
    BaselineResult,
    CostSummary,
    CoverageMetricSummary,
    PMedianSolution,
    ScenarioResult,
    ScenarioSpec,
    ServiceCoverageConstraint,
    ServiceMetric,
)
from .report_service import (
    NETWORK_PLANNING_MARKDOWN_SCHEMA,
    build_network_baseline_assessment_report_bundle,
    build_network_planning_report_bundle,
    render_network_baseline_assessment_markdown,
    render_network_planning_report_markdown,
)
from .resource_store import ResourceStore
from .solver import (
    SolverUnavailable,
    compare_assignments,
    coverage_metrics,
    enumerate_p_median,
    service_metrics,
    solve_assignment,
    solve_current_assignment,
    summarize_assignment_cost,
)
from .workspace_files import MAX_WORKSPACE_FILE_BYTES
from .workspace_intake import read_json_document

SANDBOX_STATE_META_CAPABILITY = "codex/sandbox-state-meta"
MCP_SERVER_NAME = "supply_chain"
RESOURCE_URI_PREFIX = "supply-chain://resources/"

CONTENT_ADDRESSED_RESOURCE_TOOL = ToolAnnotations(
    readOnlyHint=False,
    destructiveHint=False,
    idempotentHint=True,
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
_mcp_resource_runtime: McpResourceRuntime | None = None


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
    title="Supply-chain planning resource",
    mime_type="application/json",
)
def read_supply_chain_resource(resource_id: str) -> str:
    """Read an immutable JSON planning resource created by this MCP server."""
    return _store().read(resource_id)


@mcp.tool(structured_output=True, annotations=CONTENT_ADDRESSED_RESOURCE_TOOL)
def compare_network_scenarios(
    baseline_ref: ResourceRef,
    candidate_ref: ResourceRef,
    service_targets: Annotated[list[float], Field(min_length=1, max_length=32)],
    ctx: Context,
) -> CallToolResult:
    """Compare a typed baseline with another baseline, scenario, or location result."""
    _runtime().require_workspace(ctx)
    if any(target <= 0 for target in service_targets):
        raise McpResourceContractError("comparison_service_targets_invalid")
    baseline = _runtime().load_model(
        baseline_ref,
        "network_baseline.v2",
        BaselineResult,
    )
    if candidate_ref.resource_schema == "network_scenario.v2":
        candidate = _runtime().load_model(
            candidate_ref,
            "network_scenario.v2",
            ScenarioResult,
        )
        candidate_assignment = candidate.assignment
        candidate_active_ids = set(candidate.active_warehouse_ids)
    elif candidate_ref.resource_schema == "network_baseline.v2":
        candidate = _runtime().load_model(
            candidate_ref,
            "network_baseline.v2",
            BaselineResult,
        )
        candidate_assignment = candidate.assignment
        candidate_active_ids = set(candidate.active_warehouse_ids)
    elif candidate_ref.resource_schema == "facility_location_solution.v3":
        candidate = _runtime().load_model(
            candidate_ref,
            "facility_location_solution.v3",
            PMedianSolution,
        )
        if candidate.assignment is None:
            raise McpResourceContractError("candidate_assignment_unavailable")
        candidate_assignment = candidate.assignment
        candidate_active_ids = set(candidate.active_warehouse_ids)
    else:
        raise McpResourceContractError("comparison_candidate_schema_invalid")
    comparison: AssignmentComparison = compare_assignments(
        baseline.assignment,
        candidate_assignment,
        service_targets,
        set(baseline.active_warehouse_ids),
        candidate_active_ids,
    )
    service_summary = (
        ", ".join(
            f"{metric.target_hours:g}h demand-weighted "
            f"{metric.before_coverage_rate:.1%}→"
            f"{metric.after_coverage_rate:.1%} ({metric.coverage_rate_delta:+.1%})"
            for metric in comparison.service[:8]
        )
        or "none"
    )
    if len(comparison.service) > 8:
        service_summary = f"{service_summary}, +{len(comparison.service) - 8} more"
    cost_summary = (
        "unavailable"
        if comparison.before_cost is None or comparison.after_cost is None
        else f"{comparison.before_cost:.2f}→{comparison.after_cost:.2f} "
        f"({comparison.cost_delta or 0:+.2f})"
    )
    return _runtime().publish(
        comparison.schema_version,
        comparison,
        f"Compared {len(comparison.city_changes)} city assignments; selected "
        f"[{_bounded_id_summary(comparison.selected_warehouse_ids)}], removed "
        f"[{_bounded_id_summary(comparison.removed_warehouse_ids)}], affected "
        f"{len(comparison.affected_city_ids)}, reassigned "
        f"{len(comparison.reassigned_city_ids)}; cost {cost_summary}; service "
        f"{service_summary}.",
    )


def _load_ready_network(resource_ref: ResourceRef) -> PreparedNetworkResource:
    prepared = _runtime().load_model(
        resource_ref,
        "normalized_network_input.v1",
        PreparedNetworkResource,
    )
    if prepared.state != "ready":
        raise McpResourceContractError("normalized_input_not_ready")
    return prepared


@mcp.tool(structured_output=True, annotations=CONTENT_ADDRESSED_RESOURCE_TOOL)
def prepare_network_distribution_map(
    normalized_input_ref: ResourceRef,
    ctx: Context,
    include_candidates: bool = False,
) -> CallToolResult:
    """Publish map points and exact map_utils.create_map_card handoff arguments."""
    _runtime().require_workspace(ctx)
    prepared = _load_ready_network(normalized_input_ref)
    normalized = NormalizedInputBatch(
        demand_cities=prepared.demand_cities,
        warehouses=prepared.warehouses,
        current_assignments=prepared.current_assignments,
        route_quotes=prepared.route_quotes,
        provided_route_facts=prepared.provided_route_facts,
        issues=prepared.issues,
    )
    geojson = build_network_distribution_geojson(
        normalized,
        include_candidates=include_candidates,
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
    result = _runtime().publish_geojson(geojson.schema_version, geojson, summary)
    structured = result.structuredContent
    if structured is None:
        raise McpResourceContractError("map_data_result_missing")
    structured.update(
        {
            "feature_count": len(geojson.features),
            "layer_counts": {
                "demand": demand_count,
                "existing_warehouses": existing_count,
                "candidate_warehouses": candidate_count,
            },
            "map_card_handoff": build_network_distribution_map_card_handoff(
                MapResourceRef.model_validate(structured["data_ref"]),
                include_candidates=include_candidates,
            ).model_dump(mode="json", by_alias=True),
        }
    )
    return result


@mcp.tool(structured_output=True, annotations=CONTENT_ADDRESSED_RESOURCE_TOOL)
def plan_route_matrix(
    normalized_input_ref: ResourceRef,
    route_method: Literal["haversine", "navigation", "provided"],
    ctx: Context,
    detour_coefficient: float | None = None,
    average_speed_kph: float | None = None,
) -> CallToolResult:
    """Plan required layered route pairs without persisting workflow state."""
    _runtime().require_workspace(ctx)
    prepared = _load_ready_network(normalized_input_ref)
    plan = _plan_composable_route_matrix(
        prepared.demand_cities,
        prepared.warehouses,
        route_method,
        detour_coefficient,
        average_speed_kph,
    )
    return _runtime().publish(
        plan.schema_version,
        plan,
        f"Planned {plan.route_count} layered routes using {plan.method}; "
        f"estimated billable navigation calls: {plan.estimated_billable_calls}; "
        f"provided route facts available: {len(prepared.provided_route_facts)}.",
    )


@mcp.tool(structured_output=True, annotations=CONTENT_ADDRESSED_RESOURCE_TOOL)
def build_haversine_route_matrix(
    normalized_input_ref: ResourceRef,
    detour_coefficient: float,
    average_speed_kph: float,
    ctx: Context,
    prior_route_matrix_ref: ResourceRef | None = None,
) -> CallToolResult:
    """Build missing haversine facts and reuse only exact prior pair facts."""
    _runtime().require_workspace(ctx)
    prepared = _load_ready_network(normalized_input_ref)
    prior = (
        _runtime().load_model(
            prior_route_matrix_ref,
            "route_matrix.v2",
            ComposableRouteMatrix,
        )
        if prior_route_matrix_ref is not None
        else None
    )
    matrix = build_route_matrix_with_reuse(
        prepared.demand_cities,
        prepared.warehouses,
        prior.rows if prior is not None else [],
        detour_coefficient,
        average_speed_kph,
        warehouse_scope="all_warehouses",
    )
    validation = matrix.validation
    return _runtime().publish(
        matrix.schema_version,
        matrix,
        "Built route matrix with "
        f"{validation['reused_pair_count']} reused, "
        f"{validation['computed_pair_count']} computed, and "
        f"{validation['missing_pair_count']} missing pairs.",
    )


@mcp.tool(structured_output=True, annotations=CONTENT_ADDRESSED_RESOURCE_TOOL)
def build_provided_route_matrix(
    normalized_input_ref: ResourceRef,
    warehouse_scope: Literal["existing_only", "all_warehouses"],
    ctx: Context,
) -> CallToolResult:
    """Materialize uploaded distance and duration facts for one warehouse scope."""
    _runtime().require_workspace(ctx)
    prepared = _load_ready_network(normalized_input_ref)
    warehouses = prepared.warehouses
    if warehouse_scope == "existing_only":
        warehouses = [warehouse for warehouse in warehouses if warehouse.is_existing]
    matrix = _build_provided_route_matrix(
        prepared.demand_cities,
        warehouses,
        prepared.provided_route_facts,
        warehouse_scope=warehouse_scope,
    )
    validation = matrix.validation
    return _runtime().publish(
        matrix.schema_version,
        matrix,
        f"Built provided route matrix for {warehouse_scope} with "
        f"{validation['provided_pair_count']} supplied, "
        f"{validation['missing_pair_count']} missing, and "
        f"{validation['ignored_input_pair_count']} out-of-scope pair facts.",
    )


@mcp.tool(structured_output=True, annotations=CONTENT_ADDRESSED_RESOURCE_TOOL)
def validate_route_matrix(
    normalized_input_ref: ResourceRef,
    route_matrix_ref: ResourceRef,
    ctx: Context,
) -> CallToolResult:
    """Validate completeness and uniqueness against the normalized network."""
    _runtime().require_workspace(ctx)
    prepared = _load_ready_network(normalized_input_ref)
    matrix = _runtime().load_model(
        route_matrix_ref,
        "route_matrix.v2",
        ComposableRouteMatrix,
    )
    warehouses = prepared.warehouses
    if matrix.warehouse_scope == "existing_only":
        warehouses = [warehouse for warehouse in warehouses if warehouse.is_existing]
    validation = _validate_route_matrix_model(
        prepared.demand_cities,
        warehouses,
        matrix,
    )
    payload = {
        "schemaVersion": "route_matrix_validation.v1",
        **{key: value for key, value in validation.items() if key != "schema"},
    }
    return _runtime().publish(
        "route_matrix_validation.v1",
        payload,
        f"Route matrix validation {'passed' if validation['valid'] else 'failed'} "
        f"for {validation['route_count']} routes.",
    )


@mcp.tool(structured_output=True, annotations=CONTENT_ADDRESSED_RESOURCE_TOOL)
def register_navigation_route_matrix(
    normalized_input_ref: ResourceRef,
    navigation_result_relative_path: Annotated[
        str,
        Field(min_length=1, max_length=1024),
    ],
    ctx: Context,
    prior_route_matrix_ref: ResourceRef | None = None,
) -> CallToolResult:
    """Register navigation facts from one validated Workspace-relative JSON file."""
    workspace = _runtime().require_workspace(ctx)
    prepared = _load_ready_network(normalized_input_ref)
    document = read_json_document(workspace, navigation_result_relative_path)
    try:
        supplied = ComposableRouteMatrix.model_validate(document)
    except ValidationError as error:
        raise McpResourceContractError("navigation_result_invalid") from error
    if supplied.method != "navigation":
        raise McpResourceContractError("navigation_result_method_mismatch")
    prior = (
        _runtime().load_model(
            prior_route_matrix_ref,
            "route_matrix.v2",
            ComposableRouteMatrix,
        )
        if prior_route_matrix_ref is not None
        else None
    )
    if prior is not None and prior.warehouse_scope != supplied.warehouse_scope:
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
    )
    if matrix.missing_routes:
        raise McpResourceContractError("navigation_matrix_incomplete")
    matrix = matrix.model_copy(
        update={
            "validation": {
                **matrix.validation,
                "reused_pair_count": len(prior_rows),
                "registered_pair_count": len(supplied.rows),
                "missing_pair_count": 0,
            }
        }
    )
    return _runtime().publish(
        matrix.schema_version,
        matrix,
        f"Registered navigation matrix with {len(prior_rows)} reused and "
        f"{len(supplied.rows)} supplied pair facts.",
    )


@mcp.tool(structured_output=True, annotations=CONTENT_ADDRESSED_RESOURCE_TOOL)
def plan_cost_matrix(
    normalized_input_ref: ResourceRef,
    warehouse_scope: Literal["existing_only", "all_warehouses"],
    ctx: Context,
    calculation_policy: CostCalculationPolicy | None = None,
    route_matrix_ref: ResourceRef | None = None,
    prior_cost_matrix_ref: ResourceRef | None = None,
) -> CallToolResult:
    """Build quote-first lane costs with an optional explicit calculation policy."""
    _runtime().require_workspace(ctx)
    prepared = _load_ready_network(normalized_input_ref)
    warehouses = prepared.warehouses
    if warehouse_scope == "existing_only":
        warehouses = [warehouse for warehouse in warehouses if warehouse.is_existing]
    route_matrix = (
        _runtime().load_model(
            route_matrix_ref,
            "route_matrix.v2",
            ComposableRouteMatrix,
        )
        if route_matrix_ref is not None
        else None
    )
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
    )
    validation = matrix.validation
    return _runtime().publish(
        matrix.schema_version,
        matrix,
        "Built cost matrix with "
        f"{validation['reused_pair_count']} reused, "
        f"{validation['computed_pair_count']} computed, and "
        f"{validation['missing_pair_count']} missing lane costs.",
    )


@mcp.tool(structured_output=True, annotations=CONTENT_ADDRESSED_RESOURCE_TOOL)
def evaluate_network_baseline(
    normalized_input_ref: ResourceRef,
    route_matrix_ref: ResourceRef,
    objective: Literal["min_time", "min_cost"],
    service_targets: Annotated[list[float], Field(min_length=1, max_length=32)],
    coverage_mode: Literal["actual_current", "optimized_existing_footprint"],
    ctx: Context,
    cost_matrix_ref: ResourceRef | None = None,
) -> Annotated[CallToolResult, NetworkBaselineResourceToolResult]:
    """Evaluate one explicitly selected actual or optimized-existing baseline."""
    _runtime().require_workspace(ctx)
    if any(target <= 0 for target in service_targets):
        raise McpResourceContractError("baseline_service_targets_invalid")
    prepared = _load_ready_network(normalized_input_ref)
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
    if objective == "min_cost" and costs is None:
        raise McpResourceContractError("min_cost_baseline_requires_cost_matrix")
    active_ids = {
        warehouse.warehouse_id for warehouse in prepared.warehouses if warehouse.is_existing
    }
    if coverage_mode == "actual_current":
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
            "uncovered_cities": [item.model_dump(mode="json") for item in uncovered[:100]],
            "uncovered_cities_truncated": len(uncovered) > 100,
        }
    )
    return result


@mcp.tool(structured_output=True, annotations=CONTENT_ADDRESSED_RESOURCE_TOOL)
def evaluate_facility_scenario(
    normalized_input_ref: ResourceRef,
    route_matrix_ref: ResourceRef,
    scenario: ScenarioSpec,
    ctx: Context,
    cost_matrix_ref: ResourceRef | None = None,
) -> CallToolResult:
    """Evaluate an add, remove or relocation scenario without a mutable Case."""
    _runtime().require_workspace(ctx)
    if not scenario.service_targets or any(target <= 0 for target in scenario.service_targets):
        raise McpResourceContractError("scenario_service_targets_invalid")
    prepared = _load_ready_network(normalized_input_ref)
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
    if scenario.objective == "min_cost" and costs is None:
        raise McpResourceContractError("min_cost_scenario_requires_cost_matrix")
    warehouse_by_id = {warehouse.warehouse_id: warehouse for warehouse in prepared.warehouses}
    add_ids = set(scenario.add_warehouse_ids)
    remove_ids = set(scenario.remove_warehouse_ids)
    for relocation in scenario.relocations:
        remove_ids.add(relocation.remove_warehouse_id)
        add_ids.add(relocation.add_warehouse_id)
    unknown = (add_ids | remove_ids) - set(warehouse_by_id)
    if unknown:
        raise McpResourceContractError("scenario_unknown_warehouses")
    if any(warehouse_by_id[item].is_existing for item in add_ids):
        raise McpResourceContractError("scenario_add_requires_candidate_warehouse")
    if any(not warehouse_by_id[item].is_existing for item in remove_ids):
        raise McpResourceContractError("scenario_remove_requires_existing_warehouse")
    if add_ids & remove_ids:
        raise McpResourceContractError("scenario_add_remove_conflict")
    active_ids = {
        warehouse.warehouse_id for warehouse in prepared.warehouses if warehouse.is_existing
    }
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
    result = ScenarioResult(
        active_warehouse_ids=sorted(active_ids),
        assignment=assignment,
        cost=summarize_assignment_cost(assignment, costs) if costs is not None else None,
        service=service_metrics(assignment, sorted(set(scenario.service_targets))),
        warehouse_changes={"added": sorted(add_ids), "removed": sorted(remove_ids)},
    )
    return _runtime().publish(
        result.schema_version,
        result,
        f"Evaluated a scenario with {len(result.active_warehouse_ids)} active warehouses; "
        f"added [{_bounded_id_summary(sorted(add_ids))}], removed "
        f"[{_bounded_id_summary(sorted(remove_ids))}]; cost "
        f"{_cost_metric_summary(result.cost)}, service "
        f"{_service_metric_summary(result.service)}.",
    )


@mcp.tool(structured_output=True, annotations=BOUNDED_LOCAL_COMPUTE_TOOL)
def solve_p_median(
    normalized_input_ref: ResourceRef,
    route_matrix_ref: ResourceRef,
    cost_matrix_ref: ResourceRef,
    number_to_open: Annotated[int, Field(ge=0)],
    fixed_existing_ids: list[str],
    optional_existing_ids: list[str],
    service_targets: Annotated[list[float], Field(min_length=1, max_length=32)],
    time_limit_seconds: Annotated[float, Field(gt=0, le=300)],
    ctx: Context,
    service_constraints: list[ServiceCoverageConstraint] | None = None,
) -> CallToolResult:
    """Solve finite-candidate min-cost p-median under explicit existing-site policy."""
    _runtime().require_workspace(ctx)
    if any(target <= 0 for target in service_targets):
        raise McpResourceContractError("p_median_service_targets_invalid")
    prepared = _load_ready_network(normalized_input_ref)
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
    route_validation = _validate_route_matrix_model(
        prepared.demand_cities,
        prepared.warehouses,
        routes,
    )
    if not route_validation["valid"]:
        raise McpResourceContractError("p_median_route_matrix_incomplete")
    if costs.warehouse_scope != "all_warehouses" or costs.missing_routes:
        raise McpResourceContractError("p_median_cost_matrix_incomplete")
    constraints = [
        (constraint.target_hours, constraint.minimum_coverage)
        for constraint in service_constraints or []
    ]
    try:
        solved, _branches, timed_out = enumerate_p_median(
            prepared.demand_cities,
            prepared.warehouses,
            routes,
            costs,
            number_to_open,
            set(fixed_existing_ids),
            set(optional_existing_ids),
            time_limit_seconds,
            constraints,
        )
    except SolverUnavailable as error:
        solved = None
        timed_out = False
        unavailable_message = str(error)
    else:
        unavailable_message = None
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
        )
    else:
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
        )
    return _runtime().publish(
        solution.schema_version,
        solution,
        f"p-median status is {solution.status}; active warehouses "
        f"{len(solution.active_warehouse_ids)}, opened "
        f"[{_bounded_id_summary(solution.opened_candidate_ids)}], closed "
        f"[{_bounded_id_summary(solution.closed_existing_ids)}]; cost "
        f"{_cost_metric_summary(solution.cost)}, service "
        f"{_service_metric_summary(solution.service)}.",
    )


def _load_final_delivery_inputs(
    normalized_input_ref: ResourceRef,
    baseline_ref: ResourceRef,
    facility_location_ref: ResourceRef,
    comparison_ref: ResourceRef,
) -> tuple[
    PreparedNetworkResource,
    NormalizedInputBatch,
    BaselineResult,
    PMedianSolution,
    AssignmentComparison,
]:
    prepared = _load_ready_network(normalized_input_ref)
    baseline = _runtime().load_model(
        baseline_ref,
        "network_baseline.v2",
        BaselineResult,
    )
    facility = _runtime().load_model(
        facility_location_ref,
        "facility_location_solution.v3",
        PMedianSolution,
    )
    comparison = _runtime().load_model(
        comparison_ref,
        "network_assignment_comparison.v1",
        AssignmentComparison,
    )
    normalized = NormalizedInputBatch(
        demand_cities=prepared.demand_cities,
        warehouses=prepared.warehouses,
        current_assignments=prepared.current_assignments,
        route_quotes=prepared.route_quotes,
        provided_route_facts=prepared.provided_route_facts,
        issues=prepared.issues,
    )
    return prepared, normalized, baseline, facility, comparison


def _write_final_delivery_json_bundle(
    bundle: NetworkComparisonMapBundle,
    output_relative_path: str,
    ctx: Context,
    summary: str,
) -> CallToolResult:
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
            displayName=bundle.title,
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
    if Path(output_relative_path).suffix.lower() != ".md":
        raise McpResourceContractError("report_output_requires_markdown")
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
    normalized_input_ref: ResourceRef,
    baseline_ref: ResourceRef,
    facility_location_ref: ResourceRef,
    comparison_ref: ResourceRef,
    ctx: Context,
) -> CallToolResult:
    """Publish exact baseline-versus-plan GeoJSON and map-card arguments."""
    _runtime().require_workspace(ctx)
    prepared, normalized, baseline, facility, comparison = _load_final_delivery_inputs(
        normalized_input_ref,
        baseline_ref,
        facility_location_ref,
        comparison_ref,
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
    result = _runtime().publish_geojson(geojson.schema_version, geojson, summary)
    structured = result.structuredContent
    if structured is None:
        raise McpResourceContractError("map_data_result_missing")
    structured.update(
        {
            "feature_count": len(geojson.features),
            "map_card_handoff": build_network_comparison_map_card_handoff(
                MapResourceRef.model_validate(structured["data_ref"]),
            ).model_dump(mode="json", by_alias=True),
        }
    )
    return result


@mcp.tool(structured_output=True, annotations=FINAL_WORKSPACE_DELIVERY_TOOL)
def render_network_comparison_map(
    normalized_input_ref: ResourceRef,
    baseline_ref: ResourceRef,
    facility_location_ref: ResourceRef,
    comparison_ref: ResourceRef,
    output_relative_path: Annotated[str, Field(min_length=1, max_length=1024)],
    ctx: Context,
) -> CallToolResult:
    """Create a self-contained baseline-versus-facility map JSON file."""
    _runtime().require_workspace(ctx)
    prepared, normalized, baseline, facility, comparison = _load_final_delivery_inputs(
        normalized_input_ref,
        baseline_ref,
        facility_location_ref,
        comparison_ref,
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
    output_relative_path: Annotated[str, Field(min_length=1, max_length=1024)],
    ctx: Context,
) -> CallToolResult:
    """Create a Markdown brief for a baseline assessment or plan comparison."""
    _runtime().require_workspace(ctx)
    prepared = _load_ready_network(report_input.normalized_input_ref)
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
    if report_input.mode == "baseline":
        bundle = build_network_baseline_assessment_report_bundle(
            normalized,
            baseline,
            country_code=prepared.country_code,
        )
        markdown = render_network_baseline_assessment_markdown(bundle)
    else:
        facility = _runtime().load_model(
            report_input.facility_location_ref,
            "facility_location_solution.v3",
            PMedianSolution,
        )
        comparison = _runtime().load_model(
            report_input.comparison_ref,
            "network_assignment_comparison.v1",
            AssignmentComparison,
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

    global _workspace_root, _profile_state_root, _mcp_resource_runtime
    _workspace_root = Path.cwd().resolve(strict=True)
    _profile_state_root = Path(os.environ.get("CODEX_HOME", _workspace_root / ".codex")).resolve()
    _mcp_resource_runtime = bind_runtime(
        _workspace_root,
        _profile_state_root,
        MCP_SERVER_NAME,
        RESOURCE_URI_PREFIX,
    )
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
