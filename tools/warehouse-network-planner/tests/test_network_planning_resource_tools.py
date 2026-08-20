from __future__ import annotations

import asyncio
import json
from pathlib import Path
from types import SimpleNamespace

import pytest
from _network_fixtures import (
    indonesia_current_assignments,
    indonesia_network_fixture,
    indonesia_provided_route_facts,
    indonesia_route_quotes,
)
from open_web_codex_provider import ProviderContractError, ResourceRef, ResourceStore
from supply_chain_planner.network import (
    analysis_tools,
    cost_tools,
    delivery_tools,
    facility_tools,
    route_tools,
    server,
    tool_runtime,
)
from supply_chain_planner.network.matrix_models import (
    CostCalculationPolicy,
    CostMatrix,
    DemandUnitCostRule,
    ExplicitCostPolicy,
    ObservedQuoteMeanCostEvidence,
    ObservedQuoteMeanCostPolicy,
)
from supply_chain_planner.network.models import RouteQuoteRecord
from supply_chain_planner.network.optimization_models import (
    KeepAllExistingWarehousePolicy,
    MinimumFeasibleOpeningPolicy,
    ScenarioSpec,
    ServiceCoverageConstraint,
)
from supply_chain_planner.shared.models import (
    NetworkComparisonReportInput,
    PreparedNetworkResource,
)
from supply_chain_planner.shared.resources import SupplyChainResources


def _runtime(tmp_path: Path, monkeypatch) -> tuple[Path, ResourceStore]:
    workspace = tmp_path / "workspace"
    workspace.mkdir()
    resources = SupplyChainResources(workspace, tmp_path / "profile")
    monkeypatch.setattr(tool_runtime, "_supply_chain_resources", resources)
    return workspace, resources.store


def _context(workspace: Path) -> SimpleNamespace:
    meta = SimpleNamespace(
        model_extra={"codex/sandbox-state-meta": {"sandboxCwd": workspace.as_uri()}}
    )
    return SimpleNamespace(request_context=SimpleNamespace(meta=meta))


def _prepared_input(workspace: Path, path: str = "prepared-network.json") -> str:
    fixture = indonesia_network_fixture()
    quotes = [
        RouteQuoteRecord.model_validate(item.model_dump(mode="python"))
        for item in indonesia_route_quotes()
    ]
    prepared = PreparedNetworkResource(
        country_code="ID",
        state="ready",
        demand_cities=fixture.demand,
        warehouses=fixture.warehouses,
        current_assignments=indonesia_current_assignments(),
        route_quotes=quotes,
        provided_route_facts=indonesia_provided_route_facts(),
    )
    (workspace / path).write_text(prepared.model_dump_json(by_alias=True), encoding="utf-8")
    return path


def _ref(result) -> ResourceRef:
    assert result.structuredContent is not None
    return ResourceRef.model_validate(result.structuredContent["resource_ref"])


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


def test_network_planning_tools_use_workspace_input_and_keep_compute_results_as_resources() -> None:
    tools = {tool.name: tool for tool in asyncio.run(server.mcp.list_tools())}
    for name in (
        "prepare_route_matrix",
        "create_navigation_matrix_request",
        "import_navigation_matrix",
        "plan_cost_matrix",
        "prepare_network_distribution_map",
        "prepare_network_coverage_map",
        "evaluate_network_baseline",
        "assess_facility_change",
        "solve_p_median",
        "compare_network_scenarios",
    ):
        schema = tools[name].inputSchema
        assert "prepared_input_relative_path" in schema["required"]
        assert "normalized_input_ref" not in schema["properties"]

    assert "resource_ref" in tools["prepare_route_matrix"].outputSchema["properties"]
    assert "prepared_input_relative_path" in tools["prepare_route_matrix"].inputSchema["properties"]
    assert "cost_policy" in tools["plan_cost_matrix"].inputSchema["properties"]
    assert "calculation_policy" not in tools["plan_cost_matrix"].inputSchema["properties"]
    assert "calculation_rule_evidence" in tools["plan_cost_matrix"].outputSchema["properties"]
    assert "calculation_rule_evidence_path" in tools["plan_cost_matrix"].outputSchema["properties"]
    assert "coverage" in tools["solve_p_median"].outputSchema["properties"]
    assert "opening_policy" in tools["solve_p_median"].inputSchema["properties"]
    assert "number_to_open" not in tools["solve_p_median"].inputSchema["properties"]
    assert "search_attempts" in tools["solve_p_median"].outputSchema["properties"]


def test_plan_cost_matrix_derives_bounded_full_quote_means(
    tmp_path: Path, monkeypatch
) -> None:
    workspace, _store = _runtime(tmp_path, monkeypatch)
    ctx = _context(workspace)
    prepared_path = _prepared_input(workspace)
    routes = _ref(
        route_tools.prepare_route_matrix(
            prepared_path,
            "haversine",
            ctx,
            warehouse_scope="all_warehouses",
            detour_coefficient=1.2,
            average_speed_kph=42,
        )
    )

    calculation_dir = workspace / "outputs/warehouse-network/calculations"
    calculation_dir.mkdir(parents=True)
    derived = cost_tools.plan_cost_matrix(
        prepared_path,
        "all_warehouses",
        ctx,
        cost_policy=ObservedQuoteMeanCostPolicy(),
        route_matrix_ref=routes,
    )
    assert derived.structuredContent is not None
    evidence_path = "outputs/warehouse-network/calculations/quote-means.json"
    (workspace / evidence_path).write_text(
        json.dumps(derived.structuredContent["calculation_rule_evidence"]),
        encoding="utf-8",
    )

    result = cost_tools.plan_cost_matrix(
        prepared_path,
        "all_warehouses",
        ctx,
        cost_policy=ObservedQuoteMeanCostPolicy(),
        route_matrix_ref=routes,
        quote_mean_evidence_relative_path=evidence_path,
    )

    assert result.structuredContent is not None
    evidence = result.structuredContent["calculation_rule_evidence"]
    assert result.structuredContent["calculation_rule_source"] == "observed_quote_mean"
    assert result.structuredContent["input_identity"]["schema_version"] == (
        "prepared_network_input.v1"
    )
    assert evidence["considered_quote_count"] == 580
    assert evidence["total_quote_count"] == 580
    assert {rule["layer"]: rule["quote_count"] for rule in evidence["rules"]} == {
        "last_mile": 550,
        "linehaul": 30,
    }
    matrix = tool_runtime._runtime().load_model(_ref(result), "cost_matrix.v2", CostMatrix)
    assert matrix.missing_routes == []
    assert matrix.calculation_rule_source == "observed_quote_mean"
    assert matrix.calculation_rule_evidence_path == evidence_path
    assert matrix.calculation_rule_evidence == ObservedQuoteMeanCostEvidence.model_validate(evidence)


def test_plan_cost_matrix_rejects_unbound_quote_mean_evidence(
    tmp_path: Path, monkeypatch
) -> None:
    workspace, _store = _runtime(tmp_path, monkeypatch)
    ctx = _context(workspace)
    prepared_path = _prepared_input(workspace)
    routes = _ref(
        route_tools.prepare_route_matrix(
            prepared_path,
            "haversine",
            ctx,
            warehouse_scope="all_warehouses",
            detour_coefficient=1.2,
            average_speed_kph=42,
        )
    )
    derived = cost_tools.plan_cost_matrix(
        prepared_path,
        "all_warehouses",
        ctx,
        cost_policy=ObservedQuoteMeanCostPolicy(),
        route_matrix_ref=routes,
    )
    assert derived.structuredContent is not None
    payload = dict(derived.structuredContent["calculation_rule_evidence"])
    payload["total_quote_count"] = 579
    calculation_dir = workspace / "outputs/warehouse-network/calculations"
    calculation_dir.mkdir(parents=True)
    evidence_path = "outputs/warehouse-network/calculations/unbound.json"
    (workspace / evidence_path).write_text(json.dumps(payload), encoding="utf-8")
    with pytest.raises(ProviderContractError, match="cost_evidence_mismatch"):
        cost_tools.plan_cost_matrix(
            prepared_path,
            "all_warehouses",
            ctx,
            cost_policy=ObservedQuoteMeanCostPolicy(),
            route_matrix_ref=routes,
            quote_mean_evidence_relative_path=evidence_path,
        )


def test_plan_cost_matrix_rejects_duplicate_quote_mean_layers(
    tmp_path: Path, monkeypatch
) -> None:
    workspace, _store = _runtime(tmp_path, monkeypatch)
    ctx = _context(workspace)
    prepared_path = _prepared_input(workspace)
    routes = _ref(
        route_tools.prepare_route_matrix(
            prepared_path,
            "haversine",
            ctx,
            warehouse_scope="all_warehouses",
            detour_coefficient=1.2,
            average_speed_kph=42,
        )
    )
    derived = cost_tools.plan_cost_matrix(
        prepared_path,
        "all_warehouses",
        ctx,
        cost_policy=ObservedQuoteMeanCostPolicy(),
        route_matrix_ref=routes,
    )
    assert derived.structuredContent is not None
    payload = dict(derived.structuredContent["calculation_rule_evidence"])
    payload["rules"] = [payload["rules"][0], payload["rules"][0]]
    calculation_dir = workspace / "outputs/warehouse-network/calculations"
    calculation_dir.mkdir(parents=True)
    evidence_path = "outputs/warehouse-network/calculations/duplicate-layer.json"
    (workspace / evidence_path).write_text(json.dumps(payload), encoding="utf-8")
    with pytest.raises(ProviderContractError, match="cost_evidence_mismatch"):
        cost_tools.plan_cost_matrix(
            prepared_path,
            "all_warehouses",
            ctx,
            cost_policy=ObservedQuoteMeanCostPolicy(),
            route_matrix_ref=routes,
            quote_mean_evidence_relative_path=evidence_path,
        )


def test_prepared_input_drives_baseline_optimization_map_and_report(
    tmp_path: Path, monkeypatch
) -> None:
    workspace, _store = _runtime(tmp_path, monkeypatch)
    (workspace / "outputs").mkdir()
    ctx = _context(workspace)
    prepared_path = _prepared_input(workspace)

    routes = _ref(
        route_tools.prepare_route_matrix(
            prepared_path,
            "haversine",
            ctx,
            warehouse_scope="all_warehouses",
            detour_coefficient=1.2,
            average_speed_kph=42,
        )
    )
    costs = _ref(
        cost_tools.plan_cost_matrix(
            prepared_path,
            "all_warehouses",
            ctx,
            cost_policy=ExplicitCostPolicy(rules=_cost_policy().rules),
            route_matrix_ref=routes,
        )
    )
    baseline = _ref(
        analysis_tools.evaluate_network_baseline(
            prepared_path,
            routes,
            "min_cost",
            [6, 12, 18],
            ctx,
            coverage_mode="actual_current",
            cost_matrix_ref=costs,
        )
    )
    facility_result = facility_tools.solve_p_median(
        prepared_path,
        routes,
        costs,
        MinimumFeasibleOpeningPolicy(maximum_number_to_open=3),
        KeepAllExistingWarehousePolicy(),
        [6, 12, 18],
        30,
        ctx,
        service_constraints=[ServiceCoverageConstraint(target_hours=12, minimum_coverage=0.9)],
    )
    assert facility_result.structuredContent is not None
    assert facility_result.structuredContent["status"] == "optimal"
    assert facility_result.structuredContent["first_feasible_number_to_open"] == 2
    assert [attempt["number_to_open"] for attempt in facility_result.structuredContent["search_attempts"]] == [0, 1, 2]
    assert [attempt["optimality"] for attempt in facility_result.structuredContent["search_attempts"]] == [
        "proven",
        "proven",
        "proven",
    ]
    assert [metric["target_hours"] for metric in facility_result.structuredContent["coverage"]] == [
        6.0,
        12.0,
        18.0,
    ]
    facility = _ref(facility_result)
    comparison = _ref(
        analysis_tools.compare_network_scenarios(
            prepared_path,
            baseline,
            facility,
            [6, 12, 18],
            ctx,
        )
    )

    map_result = delivery_tools.prepare_network_comparison_map(comparison, ctx)
    assert map_result.structuredContent is not None
    assert (
        map_result.structuredContent["data_ref"]["resource_schema"]
        == "network_comparison_geojson.v1"
    )

    coverage = delivery_tools.prepare_network_coverage_map(prepared_path, facility, ctx)
    assert coverage.structuredContent is not None
    assert coverage.structuredContent["feature_count"] > 0

    report = delivery_tools.publish_network_planning_report(
        NetworkComparisonReportInput(plan_comparison_ref=comparison),
        "outputs/warehouse-network/deliverables/network-report.md",
        ctx,
    )
    assert report.structuredContent is not None
    assert (workspace / "outputs/warehouse-network/deliverables/network-report.md").is_file()
    assert (
        "network_planning_report_markdown.v1"
        in (workspace / "outputs/warehouse-network/deliverables/network-report.md").read_text()
    )


def test_minimum_feasible_requires_service_constraints(tmp_path: Path, monkeypatch) -> None:
    workspace, _store = _runtime(tmp_path, monkeypatch)
    ctx = _context(workspace)
    prepared_path = _prepared_input(workspace)
    routes = _ref(
        route_tools.prepare_route_matrix(
            prepared_path,
            "haversine",
            ctx,
            warehouse_scope="all_warehouses",
            detour_coefficient=1.2,
            average_speed_kph=42,
        )
    )
    costs = _ref(
        cost_tools.plan_cost_matrix(
            prepared_path,
            "all_warehouses",
            ctx,
            cost_policy=ExplicitCostPolicy(rules=_cost_policy().rules),
            route_matrix_ref=routes,
        )
    )
    with pytest.raises(
        ProviderContractError,
        match="p_median_minimum_feasible_requires_service_constraints",
    ):
        facility_tools.solve_p_median(
            prepared_path,
            routes,
            costs,
            MinimumFeasibleOpeningPolicy(maximum_number_to_open=3),
            KeepAllExistingWarehousePolicy(),
            [12],
            30,
            ctx,
        )


def test_scenario_validation_stops_before_loading_missing_workspace_inputs() -> None:
    unknown = ResourceRef(
        server="supply_chain",
        uri="supply-chain://resources/not-loaded",
        resource_schema="route_matrix.v2",
    )
    fake_context = SimpleNamespace(
        request_context=SimpleNamespace(meta=SimpleNamespace(model_extra={}))
    )
    with pytest.raises(ProviderContractError, match="scenario_service_targets_invalid"):
        facility_tools._load_facility_scenario_inputs(
            "not-loaded.json",
            unknown,
            None,
            ScenarioSpec(
                objective="min_time",
                service_targets=[float(target) for target in range(1, 34)],
            ),
            fake_context,
        )
