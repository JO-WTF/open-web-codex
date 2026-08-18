from __future__ import annotations

import asyncio
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
from supply_chain_planner.network import server
from supply_chain_planner.network.matrix_models import CostCalculationPolicy, DemandUnitCostRule
from supply_chain_planner.network.models import RouteQuoteRecord
from supply_chain_planner.network.optimization_models import KeepAllExistingWarehousePolicy, ScenarioSpec
from supply_chain_planner.shared.models import (
    NetworkComparisonReportInput,
    PreparedNetworkResource,
)
from supply_chain_planner.shared.resources import SupplyChainResources


def _runtime(tmp_path: Path, monkeypatch) -> tuple[Path, ResourceStore]:
    workspace = tmp_path / "workspace"
    workspace.mkdir()
    resources = SupplyChainResources(workspace, tmp_path / "profile")
    monkeypatch.setattr(server, "_supply_chain_resources", resources)
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
    assert "prepared_input_relative_path" in tools["prepare_route_matrix"].inputSchema[
        "properties"
    ]


def test_prepared_input_drives_baseline_optimization_map_and_report(tmp_path: Path, monkeypatch) -> None:
    workspace, _store = _runtime(tmp_path, monkeypatch)
    (workspace / "outputs").mkdir()
    ctx = _context(workspace)
    prepared_path = _prepared_input(workspace)

    routes = _ref(
        server.prepare_route_matrix(
            prepared_path,
            "haversine",
            ctx,
            warehouse_scope="all_warehouses",
            detour_coefficient=1.2,
            average_speed_kph=42,
        )
    )
    costs = _ref(
        server.plan_cost_matrix(
            prepared_path,
            "all_warehouses",
            ctx,
            calculation_policy=_cost_policy(),
            route_matrix_ref=routes,
        )
    )
    baseline = _ref(
        server.evaluate_network_baseline(
            prepared_path,
            routes,
            "min_cost",
            [6, 12, 18],
            "actual_current",
            ctx,
            costs,
        )
    )
    facility = _ref(
        server.solve_p_median(
            prepared_path,
            routes,
            costs,
            2,
            KeepAllExistingWarehousePolicy(),
            [6, 12, 18],
            30,
            ctx,
        )
    )
    comparison = _ref(
        server.compare_network_scenarios(
            prepared_path,
            baseline,
            facility,
            [6, 12, 18],
            ctx,
        )
    )

    map_result = server.prepare_network_comparison_map(comparison, ctx)
    assert map_result.structuredContent is not None
    assert map_result.structuredContent["data_ref"]["resource_schema"] == "network_comparison_geojson.v1"

    coverage = server.prepare_network_coverage_map(prepared_path, facility, ctx)
    assert coverage.structuredContent is not None
    assert coverage.structuredContent["feature_count"] > 0

    report = server.publish_network_planning_report(
        NetworkComparisonReportInput(plan_comparison_ref=comparison),
        "outputs/network-report.md",
        ctx,
    )
    assert report.structuredContent is not None
    assert (workspace / "outputs/network-report.md").is_file()
    assert "network_planning_report_markdown.v1" in (workspace / "outputs/network-report.md").read_text()


def test_scenario_validation_stops_before_loading_missing_workspace_inputs() -> None:
    unknown = ResourceRef(
        server="supply_chain",
        uri="supply-chain://resources/not-loaded",
        resource_schema="route_matrix.v2",
    )
    fake_context = SimpleNamespace(request_context=SimpleNamespace(meta=SimpleNamespace(model_extra={})))
    with pytest.raises(ProviderContractError, match="scenario_service_targets_invalid"):
        server._load_facility_scenario_inputs(
            "not-loaded.json",
            unknown,
            None,
            ScenarioSpec(
                objective="min_time",
                service_targets=[float(target) for target in range(1, 34)],
            ),
            fake_context,
        )
