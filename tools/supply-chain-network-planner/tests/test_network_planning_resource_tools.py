from __future__ import annotations

import asyncio
import json
from pathlib import Path
from types import SimpleNamespace

import pytest
from _network_fixtures import (
    indonesia_current_assignments,
    indonesia_network_fixture,
    indonesia_route_quotes,
)
from pydantic import ValidationError

from supply_chain_planner import server
from supply_chain_planner.map_service import NetworkComparisonMapBundle
from supply_chain_planner.matrix import build_cost_matrix, build_haversine_route_matrix
from supply_chain_planner.matrix_models import CostCalculationPolicy, DemandUnitCostRule
from supply_chain_planner.mcp_contracts import ResourceRef
from supply_chain_planner.mcp_resources import (
    McpResourceContractError,
    McpResourceRuntime,
)
from supply_chain_planner.models import (
    NetworkFinalArtifactToolResult,
    PreparedNetworkResource,
)
from supply_chain_planner.network_models import RouteQuoteRecord
from supply_chain_planner.optimization_models import (
    AssignmentComparison,
    BaselineResult,
    PMedianSolution,
    ScenarioResult,
    ScenarioSpec,
    ServiceCoverageConstraint,
)
from supply_chain_planner.report_service import NetworkPlanningReportBundle
from supply_chain_planner.resource_store import ResourceStore

BEKASI_ID = "WH-CROSS_DOCKING-BEKASI"


def _runtime(tmp_path: Path, monkeypatch) -> tuple[Path, ResourceStore]:
    workspace = tmp_path / "workspace"
    workspace.mkdir()
    store = ResourceStore(
        tmp_path / "profile" / "resources",
        uri_prefix=server.RESOURCE_URI_PREFIX,
    )
    monkeypatch.setattr(
        server,
        "_mcp_resource_runtime",
        McpResourceRuntime(
            workspace,
            tmp_path / "profile",
            server.MCP_SERVER_NAME,
            "supply-chain://resources/",
            store=store,
        ),
    )
    return workspace, store


def _context(workspace: Path) -> SimpleNamespace:
    meta = SimpleNamespace(
        model_extra={
            "codex/sandbox-state-meta": {"sandboxCwd": workspace.as_uri()}
        }
    )
    return SimpleNamespace(request_context=SimpleNamespace(meta=meta))


def _cost_policy() -> CostCalculationPolicy:
    return CostCalculationPolicy(
        rules=[
            DemandUnitCostRule(
                layer=layer,
                currency="IDR",
                fixed_cost_per_demand_unit=40_000,
                cost_per_km_per_demand_unit=1_500,
            )
            for layer in ("last_mile", "linehaul")
        ]
    )


def _published_network(
    store: ResourceStore,
) -> tuple[ResourceRef, ResourceRef, ResourceRef, set[str], set[str]]:
    fixture = indonesia_network_fixture()
    route_quotes = [
        RouteQuoteRecord.model_validate(quote.model_dump(mode="python"))
        for quote in indonesia_route_quotes()
    ]
    prepared = PreparedNetworkResource(
        country_code="ID",
        state="ready",
        demand_cities=fixture.demand,
        warehouses=fixture.warehouses,
        current_assignments=indonesia_current_assignments(),
        route_quotes=route_quotes,
    )
    routes = build_haversine_route_matrix(
        fixture.demand,
        fixture.warehouses,
        detour_coefficient=1.2,
        average_speed_kph=42,
    )
    costs = build_cost_matrix(
        fixture.demand,
        fixture.warehouses,
        route_quotes,
        _cost_policy(),
        routes,
        warehouse_scope="all_warehouses",
    )
    existing_ids = {
        warehouse.warehouse_id
        for warehouse in fixture.warehouses
        if warehouse.is_existing
    }
    candidate_ids = {
        warehouse.warehouse_id
        for warehouse in fixture.warehouses
        if not warehouse.is_existing
    }
    return (
        server.resource_ref(store.publish(prepared.schema_version, prepared)),
        server.resource_ref(store.publish(routes.schema_version, routes)),
        server.resource_ref(store.publish(costs.schema_version, costs)),
        existing_ids,
        candidate_ids,
    )


def _result_ref(result) -> ResourceRef:
    assert result.structuredContent is not None
    return ResourceRef.model_validate(result.structuredContent["resource_ref"])


def _sample2_resource_refs(
    store: ResourceStore,
    ctx: SimpleNamespace,
) -> tuple[ResourceRef, ResourceRef, ResourceRef, ResourceRef]:
    prepared_ref, route_ref, cost_ref, existing_ids, _candidate_ids = _published_network(
        store
    )
    baseline_ref = _result_ref(
        server.evaluate_network_baseline(
            prepared_ref,
            route_ref,
            "min_cost",
            [6, 12, 18],
            "actual_current",
            ctx,
            cost_ref,
        )
    )
    facility_ref = _result_ref(
        server.solve_p_median(
            prepared_ref,
            route_ref,
            cost_ref,
            2,
            sorted(existing_ids),
            [],
            [6, 12, 18],
            30,
            ctx,
        )
    )
    comparison_ref = _result_ref(
        server.compare_network_scenarios(
            baseline_ref,
            facility_ref,
            [6, 12, 18],
            ctx,
        )
    )
    return prepared_ref, baseline_ref, facility_ref, comparison_ref


def test_network_planning_tools_require_explicit_parameters_and_hide_context() -> None:
    tools = {tool.name: tool for tool in asyncio.run(server.mcp.list_tools())}

    baseline = tools["evaluate_network_baseline"].inputSchema
    assert "ctx" not in baseline["properties"]
    assert {
        "normalized_input_ref",
        "route_matrix_ref",
        "objective",
        "service_targets",
        "coverage_mode",
    }.issubset(baseline["required"])

    scenario = tools["evaluate_facility_scenario"].inputSchema
    assert "ctx" not in scenario["properties"]
    assert {"normalized_input_ref", "route_matrix_ref", "scenario"}.issubset(
        scenario["required"]
    )

    p_median = tools["solve_p_median"].inputSchema
    assert "ctx" not in p_median["properties"]
    assert {
        "normalized_input_ref",
        "route_matrix_ref",
        "cost_matrix_ref",
        "number_to_open",
        "fixed_existing_ids",
        "optional_existing_ids",
        "service_targets",
        "time_limit_seconds",
    }.issubset(p_median["required"])
    assert "service_constraints" not in p_median["required"]

    comparison = tools["compare_network_scenarios"].inputSchema
    assert "ctx" not in comparison["properties"]
    assert {"baseline_ref", "candidate_ref", "service_targets"}.issubset(
        comparison["required"]
    )

    final_required = {
        "normalized_input_ref",
        "baseline_ref",
        "facility_location_ref",
        "comparison_ref",
        "output_relative_path",
    }
    for name in (
        "render_network_comparison_map",
        "publish_network_planning_report",
    ):
        final_schema = tools[name].inputSchema
        assert "ctx" not in final_schema["properties"]
        assert set(final_schema["required"]) == final_required


def test_s2_final_tools_create_exact_self_contained_json_artifacts(
    tmp_path: Path, monkeypatch
) -> None:
    workspace, store = _runtime(tmp_path, monkeypatch)
    (workspace / "outputs").mkdir()
    ctx = _context(workspace)
    refs = _sample2_resource_refs(store, ctx)

    def reject_solver_recompute(*_args, **_kwargs):
        raise AssertionError("final delivery must not recompute a solver result")

    monkeypatch.setattr(server, "solve_assignment", reject_solver_recompute)
    monkeypatch.setattr(server, "solve_current_assignment", reject_solver_recompute)
    monkeypatch.setattr(server, "enumerate_p_median", reject_solver_recompute)

    map_result = server.render_network_comparison_map(
        *refs,
        "outputs/network-map.json",
        ctx,
    )
    report_result = server.publish_network_planning_report(
        *refs,
        "outputs/network-report.json",
        ctx,
    )

    assert map_result.structuredContent is not None
    assert report_result.structuredContent is not None
    map_tool_result = NetworkFinalArtifactToolResult.model_validate(
        map_result.structuredContent
    )
    report_tool_result = NetworkFinalArtifactToolResult.model_validate(
        report_result.structuredContent
    )
    assert set(map_result.structuredContent) == {"summary", "artifact"}
    assert set(map_result.structuredContent["artifact"]) == {
        "schema",
        "displayName",
        "mimeType",
        "workspaceRelativePath",
        "byteSize",
    }
    assert map_tool_result.artifact.artifact_schema == (
        "network_comparison_map_bundle.v1"
    )
    assert report_tool_result.artifact.artifact_schema == (
        "network_planning_report_bundle.v1"
    )
    assert map_tool_result.artifact.mime_type == "application/json"
    assert report_tool_result.artifact.mime_type == "application/json"

    map_path = workspace / map_tool_result.artifact.workspace_relative_path
    report_path = workspace / report_tool_result.artifact.workspace_relative_path
    assert map_path.stat().st_size == map_tool_result.artifact.byte_size
    assert report_path.stat().st_size == report_tool_result.artifact.byte_size
    map_bundle = NetworkComparisonMapBundle.model_validate(
        json.loads(map_path.read_text(encoding="utf-8"))
    )
    report_bundle = NetworkPlanningReportBundle.model_validate(
        json.loads(report_path.read_text(encoding="utf-8"))
    )
    assert map_bundle.summary.feature_count == 187
    assert map_bundle.summary.opened_candidate_ids == [
        "WH-CANDIDATE-KENDARI",
        "WH-CANDIDATE-MANADO",
    ]
    assert len(report_bundle.entities.demand_cities) == 50
    assert len(report_bundle.entities.warehouses) == 23
    assert len(report_bundle.baseline.assignment.rows) == 50
    assert len(report_bundle.facility.assignment.rows) == 50
    assert report_bundle.baseline.label == "actual_current"
    assert [metric.target_hours for metric in report_bundle.facility.service] == [
        6,
        12,
        18,
    ]

    with pytest.raises(McpResourceContractError, match="workspace_file_invalid"):
        server.render_network_comparison_map(
            *refs,
            "outputs/network-map.json",
            ctx,
        )
    for invalid_path in (
        (workspace / "absolute.json").as_posix(),
        "../escape.json",
    ):
        with pytest.raises(McpResourceContractError, match="workspace_file_invalid"):
            server.publish_network_planning_report(*refs, invalid_path, ctx)
    outside = tmp_path / "outside"
    outside.mkdir()
    (workspace / "linked").symlink_to(outside, target_is_directory=True)
    with pytest.raises(McpResourceContractError, match="workspace_file_invalid"):
        server.publish_network_planning_report(
            *refs,
            "linked/report.json",
            ctx,
        )
    assert list(outside.iterdir()) == []


def test_final_artifact_descriptor_rejects_empty_files() -> None:
    with pytest.raises(ValidationError):
        NetworkFinalArtifactToolResult.model_validate(
            {
                "summary": "empty",
                "artifact": {
                    "schema": "network_planning_report_bundle.v1",
                    "displayName": "Report",
                    "mimeType": "application/json",
                    "workspaceRelativePath": "report.json",
                    "byteSize": 0,
                },
            }
        )


def test_s3_reuses_prepared_resources_and_only_closes_bekasi(
    tmp_path: Path, monkeypatch
) -> None:
    workspace, store = _runtime(tmp_path, monkeypatch)
    ctx = _context(workspace)
    prepared_ref, route_ref, cost_ref, existing_ids, candidate_ids = _published_network(
        store
    )
    baseline_ref = _result_ref(
        server.evaluate_network_baseline(
            prepared_ref,
            route_ref,
            "min_cost",
            [6, 12, 18],
            "actual_current",
            ctx,
            cost_ref,
        )
    )
    baseline = server._runtime().load_model(
        baseline_ref,
        "network_baseline.v2",
        BaselineResult,
    )
    assert set(baseline.active_warehouse_ids) == existing_ids
    assert baseline.label == "actual_current"

    scenario_ref = _result_ref(
        server.evaluate_facility_scenario(
            prepared_ref,
            route_ref,
            ScenarioSpec(
                remove_warehouse_ids=[BEKASI_ID],
                objective="min_cost",
                service_targets=[12],
            ),
            ctx,
            cost_ref,
        )
    )
    scenario = server._runtime().load_model(
        scenario_ref,
        "network_scenario.v2",
        ScenarioResult,
    )
    assert set(scenario.active_warehouse_ids) == existing_ids - {BEKASI_ID}
    assert not set(scenario.active_warehouse_ids) & candidate_ids
    assert scenario.warehouse_changes == {"added": [], "removed": [BEKASI_ID]}
    assert len(scenario.assignment.rows) == 50
    assert scenario.cost is not None and scenario.cost.complete
    assert [metric.target_hours for metric in scenario.service] == [12]

    comparison_ref = _result_ref(
        server.compare_network_scenarios(
            baseline_ref,
            scenario_ref,
            [12],
            ctx,
        )
    )
    comparison = server._runtime().load_model(
        comparison_ref,
        "network_assignment_comparison.v1",
        AssignmentComparison,
    )
    assert comparison.selected_warehouse_ids == []
    assert comparison.removed_warehouse_ids == [BEKASI_ID]
    assert comparison.requested_service_targets == [12]
    assert len(comparison.city_changes) == 50
    assert len(comparison.affected_city_ids) == 50
    assert len(comparison.reassigned_city_ids) == 50


def test_s2_opens_exactly_two_candidates_and_compares_actual_baseline(
    tmp_path: Path, monkeypatch
) -> None:
    workspace, store = _runtime(tmp_path, monkeypatch)
    ctx = _context(workspace)
    prepared_ref, route_ref, cost_ref, existing_ids, _candidate_ids = _published_network(
        store
    )
    baseline_ref = _result_ref(
        server.evaluate_network_baseline(
            prepared_ref,
            route_ref,
            "min_cost",
            [6, 12, 18],
            "actual_current",
            ctx,
            cost_ref,
        )
    )
    facility_ref = _result_ref(
        server.solve_p_median(
            prepared_ref,
            route_ref,
            cost_ref,
            2,
            sorted(existing_ids),
            [],
            [6, 12, 18],
            30,
            ctx,
            [
                ServiceCoverageConstraint(
                    target_hours=18,
                    minimum_coverage=0.5,
                )
            ],
        )
    )
    facility = server._runtime().load_model(
        facility_ref,
        "facility_location_solution.v3",
        PMedianSolution,
    )
    assert facility.status == "optimal"
    assert set(facility.active_warehouse_ids) == existing_ids | {
        "WH-CANDIDATE-KENDARI",
        "WH-CANDIDATE-MANADO",
    }
    assert facility.opened_candidate_ids == [
        "WH-CANDIDATE-KENDARI",
        "WH-CANDIDATE-MANADO",
    ]
    assert facility.closed_existing_ids == []
    assert facility.assignment is not None and len(facility.assignment.rows) == 50
    assert facility.cost is not None and facility.cost.complete
    assert [metric.target_hours for metric in facility.service] == [6, 12, 18]

    comparison_ref = _result_ref(
        server.compare_network_scenarios(
            baseline_ref,
            facility_ref,
            [6, 12, 18],
            ctx,
        )
    )
    comparison = server._runtime().load_model(
        comparison_ref,
        "network_assignment_comparison.v1",
        AssignmentComparison,
    )
    assert comparison.selected_warehouse_ids == facility.opened_candidate_ids
    assert comparison.removed_warehouse_ids == []
    assert comparison.requested_service_targets == [6, 12, 18]


def test_baseline_and_p_median_reject_missing_explicit_inputs(
    tmp_path: Path, monkeypatch
) -> None:
    workspace, store = _runtime(tmp_path, monkeypatch)
    ctx = _context(workspace)
    prepared_ref, route_ref, cost_ref, existing_ids, _candidate_ids = _published_network(
        store
    )
    prepared = server._runtime().load_model(
        prepared_ref,
        "normalized_network_input.v1",
        PreparedNetworkResource,
    )
    without_current = prepared.model_copy(update={"current_assignments": []})
    without_current_ref = server.resource_ref(
        store.publish(without_current.schema_version, without_current)
    )
    with pytest.raises(McpResourceContractError, match="current_assignments_required"):
        server.evaluate_network_baseline(
            without_current_ref,
            route_ref,
            "min_cost",
            [12],
            "actual_current",
            ctx,
            cost_ref,
        )

    with pytest.raises(ValueError, match="existing_policy_incomplete"):
        server.solve_p_median(
            prepared_ref,
            route_ref,
            cost_ref,
            2,
            sorted(existing_ids - {next(iter(existing_ids))}),
            [],
            [12],
            30,
            ctx,
        )

    with pytest.raises(
        McpResourceContractError,
        match="scenario_active_upstream_required",
    ):
        server.evaluate_facility_scenario(
            prepared_ref,
            route_ref,
            ScenarioSpec(
                remove_warehouse_ids=["WH-CENTER-JAKARTA"],
                objective="min_cost",
                service_targets=[12],
            ),
            ctx,
            cost_ref,
        )
