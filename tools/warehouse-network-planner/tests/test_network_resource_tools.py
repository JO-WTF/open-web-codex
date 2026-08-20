from __future__ import annotations

import asyncio
import json
from pathlib import Path
from types import SimpleNamespace

import pytest
from _network_fixtures import network_case
from open_web_codex_provider import ProviderContractError, ResourceRef, ResourceStore
from supply_chain_planner.network import server
from supply_chain_planner.network.matrix_models import NavigationMatrixResult, RouteMatrixRow
from supply_chain_planner.network.models import CurrentAssignmentRecord
from supply_chain_planner.shared.models import PreparedNetworkResource
from supply_chain_planner.shared.planning_input import load_prepared_network_input
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


def _write_prepared_input(
    workspace: Path,
    relative_path: str,
    *,
    city_name: str = "City A",
) -> str:
    fixture = network_case()
    demand = [
        item.model_copy(update={"city_name": city_name}) if item.city_id == "city-a" else item
        for item in fixture.demand
    ]
    prepared = PreparedNetworkResource(
        country_code="ID",
        state="ready",
        demand_cities=demand,
        warehouses=fixture.warehouses,
        current_assignments=[
            CurrentAssignmentRecord(
                demand_city_id="city-a",
                serving_warehouse_id="center-a",
            ),
            CurrentAssignmentRecord(
                demand_city_id="city-b",
                serving_warehouse_id="cross-b",
                upstream_center_id="center-a",
            ),
        ],
        route_quotes=[],
    )
    (workspace / relative_path).write_text(
        prepared.model_dump_json(by_alias=True),
        encoding="utf-8",
    )
    return relative_path


def _result_ref(result) -> ResourceRef:
    assert result.structuredContent is not None
    return ResourceRef.model_validate(result.structuredContent["resource_ref"])


def test_network_tools_accept_only_prepared_workspace_inputs() -> None:
    tools = {tool.name: tool for tool in asyncio.run(server.mcp.list_tools())}
    for name in (
        "prepare_route_matrix",
        "create_navigation_matrix_request",
        "import_navigation_matrix",
        "plan_cost_matrix",
        "prepare_network_distribution_map",
        "evaluate_network_baseline",
        "assess_facility_change",
        "solve_p_median",
        "compare_network_scenarios",
        "prepare_network_coverage_map",
    ):
        schema = tools[name].inputSchema
        assert "prepared_input_relative_path" in schema["required"]
        assert "normalized_input_ref" not in schema["properties"]

    route = tools["prepare_route_matrix"].inputSchema
    assert "prior_route_matrix_ref" in route["properties"]
    assert (
        "both detour_coefficient and average_speed_kph"
        in route["properties"]["route_method"]["description"]
    )


def test_network_rejects_matrix_from_another_prepared_workspace_input(
    tmp_path, monkeypatch
) -> None:
    workspace, _store = _runtime(tmp_path, monkeypatch)
    ctx = _context(workspace)
    first_path = _write_prepared_input(workspace, "prepared-a.json")
    second_path = _write_prepared_input(workspace, "prepared-b.json", city_name="Renamed City")

    routes = server.prepare_route_matrix(
        first_path,
        "haversine",
        ctx,
        warehouse_scope="existing_only",
        detour_coefficient=1.2,
        average_speed_kph=40,
    )
    route_ref = _result_ref(routes)
    stored_routes = server._runtime().load_model(
        route_ref, "route_matrix.v2", server.ComposableRouteMatrix
    )
    _prepared, expected_identity = load_prepared_network_input(workspace, first_path)
    assert stored_routes.input_identity == expected_identity

    with pytest.raises(ProviderContractError, match="baseline_input_identity_mismatch"):
        server.evaluate_network_baseline(
            second_path,
            route_ref,
            "min_time",
            [12],
            ctx,
            coverage_mode="optimized_existing_footprint",
        )


def test_baseline_auto_uses_optimized_existing_when_assignments_are_absent(
    tmp_path, monkeypatch
) -> None:
    workspace, _store = _runtime(tmp_path, monkeypatch)
    ctx = _context(workspace)
    prepared_path = _write_prepared_input(workspace, "prepared-auto.json")
    payload = json.loads((workspace / prepared_path).read_text(encoding="utf-8"))
    payload["current_assignments"] = []
    (workspace / prepared_path).write_text(json.dumps(payload), encoding="utf-8")
    routes = server.prepare_route_matrix(
        prepared_path,
        "haversine",
        ctx,
        warehouse_scope="existing_only",
        detour_coefficient=1.2,
        average_speed_kph=40,
    )
    baseline = server.evaluate_network_baseline(
        prepared_path,
        _result_ref(routes),
        "min_time",
        [12],
        ctx,
    )
    stored = server._runtime().load_model(
        _result_ref(baseline), "network_baseline.v2", server.BaselineResult
    )
    assert stored.label == "optimized_existing_footprint"


def test_workspace_input_mutation_invalidates_existing_matrix(tmp_path, monkeypatch) -> None:
    workspace, _store = _runtime(tmp_path, monkeypatch)
    ctx = _context(workspace)
    prepared_path = _write_prepared_input(workspace, "prepared.json")
    routes = server.prepare_route_matrix(
        prepared_path,
        "haversine",
        ctx,
        warehouse_scope="existing_only",
        detour_coefficient=1.2,
        average_speed_kph=40,
    )
    route_ref = _result_ref(routes)

    payload = (workspace / prepared_path).read_text(encoding="utf-8")
    (workspace / prepared_path).write_text(
        payload.replace("City A", "Changed City", 1), encoding="utf-8"
    )
    with pytest.raises(ProviderContractError, match="baseline_input_identity_mismatch"):
        server.evaluate_network_baseline(
            prepared_path,
            route_ref,
            "min_time",
            [12],
            ctx,
            coverage_mode="optimized_existing_footprint",
        )


def test_navigation_request_and_import_use_exact_workspace_contract(tmp_path, monkeypatch) -> None:
    workspace, _store = _runtime(tmp_path, monkeypatch)
    ctx = _context(workspace)
    prepared_path = _write_prepared_input(workspace, "prepared.json")
    request_result = server.create_navigation_matrix_request(
        prepared_path,
        "existing_only",
        "outputs/warehouse-network/requests/navigation-request.json",
        ctx,
    )
    request_payload = json.loads(
        (workspace / request_result.navigation_request_relative_path).read_text()
    )
    assert request_payload["schema_version"] == "navigation_matrix_request.v1"
    result = NavigationMatrixResult(
        input_identity=request_result.input_identity,
        warehouse_scope="existing_only",
        rows=[
            RouteMatrixRow(
                origin_id=item["origin_id"],
                destination_id=item["destination_id"],
                layer=item["layer"],
                distance_km=1,
                duration_hours=0.1,
                method="navigation",
                tool_version="test-navigation.v1",
                origin_longitude=item["origin_longitude"],
                origin_latitude=item["origin_latitude"],
                destination_longitude=item["destination_longitude"],
                destination_latitude=item["destination_latitude"],
                navigation_provider="test-provider",
                navigation_profile="driving",
            )
            for item in request_payload["routes"]
        ],
    )
    (workspace / "navigation-result.json").write_text(result.model_dump_json(), encoding="utf-8")
    imported = server.import_navigation_matrix(
        prepared_path,
        "navigation-result.json",
        ctx,
    )
    route_ref = _result_ref(imported)
    route_matrix = server._runtime().load_model(
        route_ref, "route_matrix.v2", server.ComposableRouteMatrix
    )
    assert route_matrix.method == "navigation"
    assert route_matrix.input_identity == request_result.input_identity
