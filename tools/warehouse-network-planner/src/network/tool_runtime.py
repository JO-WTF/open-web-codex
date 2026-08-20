"""Shared Runtime scope, ResourceStore access, and domain helpers."""

from __future__ import annotations

import os
from pathlib import Path

from mcp.server.fastmcp import (
    Context,
)
from mcp.types import (
    ToolAnnotations,
)
from open_web_codex_provider import (
    McpResourceRuntime,
    ProviderContractError,
    ResourceStore,
)
from supply_chain_planner.network.models import (
    PlanningInputIdentity,
)
from supply_chain_planner.network.optimization_models import (
    BaselineResult,
    ComparableNetworkView,
    CostSummary,
    CoverageMetricSummary,
    PMedianSolution,
    ScenarioResult,
    ServiceMetric,
)
from supply_chain_planner.shared.models import (
    ComparableNetworkResultRef,
    PreparedNetworkResource,
)
from supply_chain_planner.shared.planning_input import (
    load_prepared_network_input,
)
from supply_chain_planner.shared.resources import (
    SupplyChainResources,
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

_workspace_root = Path.cwd().resolve()
_profile_state_root = Path(os.environ.get("CODEX_HOME", _workspace_root / ".codex")).resolve()
_supply_chain_resources: SupplyChainResources | None = None

def configure_runtime(workspace_root: Path, profile_state_root: Path) -> None:
    global _workspace_root, _profile_state_root, _supply_chain_resources
    _workspace_root = workspace_root.resolve(strict=True)
    _profile_state_root = profile_state_root.resolve()
    _supply_chain_resources = SupplyChainResources(_workspace_root, _profile_state_root)

def _store() -> ResourceStore:
    return _runtime().store

def _runtime() -> McpResourceRuntime:
    global _supply_chain_resources
    if _supply_chain_resources is None:
        _supply_chain_resources = SupplyChainResources(_workspace_root, _profile_state_root)
    return _supply_chain_resources.network

def register_resources(mcp) -> None:
    @mcp.resource(
        "supply-chain://resources/{resource_id}",
        name="supply_chain_resource",
        title="Supply-chain planning resource",
        mime_type="application/json",
    )
    def read_supply_chain_resource(resource_id: str) -> str:
        """Read an immutable JSON planning resource created by this MCP server."""
        return _store().read(resource_id)

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




def _load_comparable_resource(
    resource_ref: ComparableNetworkResultRef,
) -> ComparableNetworkView:
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
    if resource_ref.resource_schema == "network_baseline.v2":
        return ComparableNetworkView(
            label=result.label,
            active_warehouse_ids=result.active_warehouse_ids,
            assignment=result.assignment,
            cost=result.cost,
            service=result.service,
            notice_code=result.notice_code,
            input_identity=result.input_identity,
        )
    if resource_ref.resource_schema == "network_scenario.v2":
        return ComparableNetworkView(
            label="scenario",
            active_warehouse_ids=result.active_warehouse_ids,
            assignment=result.assignment,
            cost=result.cost,
            service=result.service,
            input_identity=result.input_identity,
        )
    return ComparableNetworkView(
        label=result.status,
        active_warehouse_ids=result.active_warehouse_ids,
        assignment=result.assignment,
        cost=result.cost,
        service=result.service,
        notice_code=result.message,
        status=result.status,
        optimality=result.optimality,
        objective_value=result.objective_value,
        best_bound=result.best_bound,
        input_identity=result.input_identity,
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
