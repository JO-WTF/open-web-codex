from __future__ import annotations

import asyncio
import json
import sys
from contextlib import asynccontextmanager
from pathlib import Path
from types import SimpleNamespace

import pytest
from _network_fixtures import network_case
from open_web_codex_provider import (
    GeoJsonResourceRef,
    ProviderContractError,
    PublishedResource,
    ResourceRef,
    ResourceStore,
)
from pydantic import ValidationError
from supply_chain_planner.network import server
from supply_chain_planner.network.matrix import build_haversine_route_matrix
from supply_chain_planner.network.matrix_models import (
    CostCalculationPolicy,
    CostMatrix,
    DemandUnitCostRule,
    NavigationRouteMatrixStats,
    RouteMatrix,
)
from supply_chain_planner.network.models import ProvidedRouteFactRecord
from supply_chain_planner.shared.models import PreparedNetworkInputRef, PreparedNetworkResource
from supply_chain_planner.shared.resource_identity import NETWORK_MCP_SERVER_NAME
from supply_chain_planner.shared.resources import SupplyChainResources

McpResourceContractError = ProviderContractError


def _resource_ref(
    published: PublishedResource,
    *,
    server_name: str = NETWORK_MCP_SERVER_NAME,
) -> ResourceRef:
    return ResourceRef(
        server=server_name,
        resource_schema=published.schema,
        uri=published.uri,
    )


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


def _prepared_ref(
    store: ResourceStore,
    *,
    state: str = "ready",
    provided_route_facts: list[ProvidedRouteFactRecord] | None = None,
) -> ResourceRef:
    fixture = network_case()
    prepared = PreparedNetworkResource(
        country_code="ID",
        state=state,
        demand_cities=fixture.demand,
        warehouses=fixture.warehouses,
        current_assignments=[],
        route_quotes=[],
        provided_route_facts=provided_route_facts or [],
    )
    return _resource_ref(
        store.publish(prepared.schema_version, prepared),
        server_name=server.DATA_MCP_SERVER_NAME,
    )


def _result_ref(result) -> ResourceRef:
    assert result.structuredContent is not None
    assert set(result.structuredContent) in (
        {"summary", "resource_ref"},
        {"summary", "resource_ref", "state"},
    )
    ref = ResourceRef.model_validate(result.structuredContent["resource_ref"])
    assert ref.server == "supply_chain"
    return ref


def _cost_policy() -> CostCalculationPolicy:
    return CostCalculationPolicy(
        rules=[
            DemandUnitCostRule(
                layer=layer,
                currency="IDR",
                fixed_cost_per_demand_unit=5,
                cost_per_km_per_demand_unit=2,
            )
            for layer in ("last_mile", "linehaul")
        ]
    )


def test_matrix_tools_expose_composable_resource_schemas() -> None:
    tools = {tool.name: tool for tool in asyncio.run(server.mcp.list_tools())}
    for name in (
        "prepare_route_matrix",
        "register_navigation_route_matrix",
        "plan_cost_matrix",
    ):
        schema = tools[name].inputSchema
        assert "ctx" not in schema["properties"]
        assert "normalized_input_ref" in schema["required"]

    plan = tools["prepare_route_matrix"].inputSchema
    prepared_ref_schema = plan["$defs"]["PreparedNetworkInputRef"]
    assert prepared_ref_schema["properties"]["server"]["const"] == "supply_chain_data"
    assert prepared_ref_schema["properties"]["resource_schema"]["const"] == (
        "normalized_network_input.v1"
    )
    for name in (
        "prepare_route_matrix",
        "register_navigation_route_matrix",
        "plan_cost_matrix",
        "prepare_network_distribution_map",
        "evaluate_network_baseline",
        "assess_facility_change",
        "solve_p_median",
        "compare_network_scenarios",
    ):
        assert tools[name].inputSchema["properties"]["normalized_input_ref"]["$ref"].endswith(
            "/PreparedNetworkInputRef"
        )
    valid_ref = PreparedNetworkInputRef.model_validate(
        {
            "server": "supply_chain_data",
            "uri": "supply-chain://resources/normalized-input",
            "resource_schema": "normalized_network_input.v1",
        }
    )
    assert valid_ref.server == "supply_chain_data"
    with pytest.raises(ValidationError):
        PreparedNetworkInputRef.model_validate(
            valid_ref.model_dump() | {"server": "supply_chain"}
        )
    assert "prior_route_matrix_ref" in plan["properties"]
    assert "Route method" in plan["properties"]["route_method"]["description"]
    assert "both detour_coefficient and average_speed_kph" in plan["properties"][
        "route_method"
    ]["description"]
    assert "Required when route_method is haversine" in plan["properties"][
        "detour_coefficient"
    ]["description"]
    assert "Required when route_method is haversine" in plan["properties"][
        "average_speed_kph"
    ]["description"]
    assert "warehouse_scope" in plan["properties"]
    assert "provided_route_facts" not in plan["properties"]
    navigation = tools["register_navigation_route_matrix"].inputSchema
    assert "navigation_result_relative_path" in navigation["required"]
    assert "navigation_result_ref" not in navigation["properties"]
    cost = tools["plan_cost_matrix"].inputSchema
    assert "warehouse_scope" in cost["required"]
    assert "calculation_policy" not in cost["required"]

    distribution = tools["prepare_network_distribution_map"].inputSchema
    assert "ctx" not in distribution["properties"]
    assert distribution["required"] == ["normalized_input_ref"]
    assert "baseline_ref" in distribution["properties"]


def test_network_stdio_advertises_native_workspace_metadata(monkeypatch) -> None:
    captured = {}

    @asynccontextmanager
    async def fake_stdio_server():
        yield object(), object()

    async def fake_run(_reader, _writer, initialization_options):
        captured["options"] = initialization_options

    monkeypatch.setattr(server, "stdio_server", fake_stdio_server)
    monkeypatch.setattr(server.mcp._mcp_server, "run", fake_run)

    asyncio.run(server.run_stdio())

    options = captured["options"]
    assert server.SANDBOX_STATE_META_CAPABILITY in options.capabilities.experimental


def test_main_starts_stdio_without_case_repository(tmp_path: Path, monkeypatch) -> None:
    workspace = tmp_path / "workspace"
    workspace.mkdir()
    profile = tmp_path / "profile"
    profile.mkdir()
    monkeypatch.chdir(workspace)
    monkeypatch.setenv("CODEX_HOME", str(profile))
    monkeypatch.setattr(sys, "argv", ["supply-chain-planner", "--transport", "stdio"])
    for name in ("_workspace_root", "_profile_state_root", "_supply_chain_resources"):
        monkeypatch.setattr(server, name, getattr(server, name))
    captured: dict[str, bool] = {}

    @asynccontextmanager
    async def fake_stdio_server():
        captured["stdio_entered"] = True
        yield object(), object()

    async def fake_run(_reader, _writer, _initialization_options):
        captured["runtime_called"] = True

    monkeypatch.setattr(server, "stdio_server", fake_stdio_server)
    monkeypatch.setattr(server.mcp._mcp_server, "run", fake_run)

    server.main()

    assert captured == {"stdio_entered": True, "runtime_called": True}
    assert not (profile / "mcp-state" / "supply-chain-network" / "cases.sqlite3").exists()


def test_distribution_map_publishes_geojson_for_map_card_only(
    tmp_path: Path,
    monkeypatch,
) -> None:
    workspace, store = _runtime(tmp_path, monkeypatch)
    ctx = _context(workspace)
    prepared_ref = _prepared_ref(store)

    result = server.prepare_network_distribution_map(prepared_ref, ctx)

    assert result.structuredContent is not None
    assert set(result.structuredContent) == {
        "summary",
        "data_ref",
        "feature_count",
        "feature_counts",
    }
    data_ref = GeoJsonResourceRef.model_validate(result.structuredContent["data_ref"])
    assert data_ref.server == "supply_chain"
    assert data_ref.format == "geojson"
    assert data_ref.profile.feature_count == result.structuredContent["feature_count"]
    assert data_ref.profile.discriminator_property == "kind"
    assert [item.value for item in data_ref.profile.feature_types] == [
        "demand",
        "warehouse",
    ]
    demand_profile = data_ref.profile.feature_types[0]
    assert set(demand_profile.properties) >= {
        "city_name",
        "kind",
    }
    assert "duration_hours" not in demand_profile.properties
    payload = store.load(data_ref)
    assert payload["type"] == "FeatureCollection"
    kinds = [feature["properties"]["kind"] for feature in payload["features"]]
    fixture = network_case()
    expected_demands = len(fixture.demand)
    expected_existing = sum(warehouse.is_existing for warehouse in fixture.warehouses)
    assert kinds.count("demand") == expected_demands
    assert kinds.count("warehouse") == expected_existing
    assert result.structuredContent["feature_counts"] == {
        "demand": expected_demands,
        "existing_warehouses": expected_existing,
        "candidate_warehouses": 0,
    }
    demand_properties = [
        feature["properties"]
        for feature in payload["features"]
        if feature["properties"]["kind"] == "demand"
    ]
    assert all("duration_hours" not in item for item in demand_properties)
    assert "layers" not in payload
    assert "extensions" not in payload
    assert not list(workspace.rglob("*.json"))

    route_ref = _result_ref(
        server.prepare_route_matrix(prepared_ref, "haversine", ctx, detour_coefficient=1.2, average_speed_kph=40)
    )
    baseline_result = server.evaluate_network_baseline(
        prepared_ref,
        route_ref,
        "min_time",
        [12],
        "optimized_existing_footprint",
        ctx,
    )
    assert baseline_result.structuredContent is not None
    baseline_ref = ResourceRef.model_validate(
        baseline_result.structuredContent["resource_ref"]
    )
    with_baseline = server.prepare_network_distribution_map(
        prepared_ref,
        ctx,
        baseline_ref=baseline_ref,
    )
    assert with_baseline.structuredContent is not None
    baseline_payload = store.load(
        GeoJsonResourceRef.model_validate(with_baseline.structuredContent["data_ref"])
    )
    timed_demands = [
        feature["properties"]
        for feature in baseline_payload["features"]
        if feature["properties"]["kind"] == "demand"
    ]
    assert all(item["assigned_warehouse_id"] for item in timed_demands)
    assert all(item["distance_km"] is not None for item in timed_demands)
    assert all(item["duration_hours"] is not None for item in timed_demands)

    with_candidates = server.prepare_network_distribution_map(
        prepared_ref,
        ctx,
        include_candidates=True,
    )
    assert with_candidates.structuredContent is not None
    expected_candidates = sum(not warehouse.is_existing for warehouse in fixture.warehouses)
    assert (
        with_candidates.structuredContent["feature_counts"]["candidate_warehouses"]
        == expected_candidates
    )
    candidate_payload = store.load(
        GeoJsonResourceRef.model_validate(with_candidates.structuredContent["data_ref"])
    )
    candidate_features = [
        feature
        for feature in candidate_payload["features"]
        if feature["properties"]["kind"] == "warehouse"
        and not feature["properties"]["is_existing"]
    ]
    assert len(candidate_features) == expected_candidates


def test_coverage_map_reuses_exact_normalized_input_without_workspace_parsing(
    tmp_path: Path, monkeypatch
) -> None:
    workspace, store = _runtime(tmp_path, monkeypatch)
    ctx = _context(workspace)
    prepared_ref = _prepared_ref(store)
    route_ref = _result_ref(
        server.prepare_route_matrix(
            prepared_ref,
            "haversine",
            ctx,
            detour_coefficient=1.2,
            average_speed_kph=40,
        )
    )
    baseline = server.evaluate_network_baseline(
        prepared_ref,
        route_ref,
        "min_time",
        [12],
        "optimized_existing_footprint",
        ctx,
    )
    assert baseline.structuredContent is not None
    baseline_ref = ResourceRef.model_validate(baseline.structuredContent["resource_ref"])

    def reject_workspace_parse(*_args, **_kwargs):
        raise AssertionError("coverage map must consume normalized_input_ref, not Workspace files")

    monkeypatch.setattr(server, "read_json_document", reject_workspace_parse)
    result = server.prepare_network_coverage_map(prepared_ref, baseline_ref, ctx)

    assert result.structuredContent is not None
    data_ref = GeoJsonResourceRef.model_validate(result.structuredContent["data_ref"])
    assert data_ref.resource_schema == "network_coverage_geojson.v1"
    payload = store.load(data_ref)
    kinds = [feature["properties"]["kind"] for feature in payload["features"]]
    assert kinds.count("demand") == 2
    assert kinds.count("warehouse") == 3
    assert kinds.count("last_mile_assignment") == 2
    assert kinds.count("linehaul_connection") == 1
    warehouse_features = [
        feature["properties"]
        for feature in payload["features"]
        if feature["properties"]["kind"] == "warehouse"
    ]
    candidate_features = [
        properties for properties in warehouse_features if not properties["is_existing"]
    ]
    assert len(candidate_features) == 1
    assert candidate_features[0] == {
        "kind": "warehouse",
        "warehouse_id": "candidate-c",
        "warehouse_name": "Candidate C",
        "warehouse_type": "cross_docking",
        "city_id": "city-b",
        "city_name": "City B",
        "province_id": None,
        "province_name": None,
        "is_existing": False,
        "baseline_active": False,
        "facility_active": False,
        "opened_candidate": False,
        "closed_existing": False,
    }
    warehouse_profile = next(
        feature_type
        for feature_type in data_ref.profile.feature_types
        if feature_type.value == "warehouse"
    )
    assert warehouse_profile.properties["warehouse_type"] == "string"
    assert warehouse_profile.properties["is_existing"] == "boolean"
    assert warehouse_profile.properties["opened_candidate"] == "boolean"
    assert warehouse_profile.properties["closed_existing"] == "boolean"
    demand_features = [
        feature["properties"]
        for feature in payload["features"]
        if feature["properties"]["kind"] == "demand"
    ]
    assert all(
        "baseline_duration_hours" not in properties
        and "facility_duration_hours" not in properties
        for properties in demand_features
    )
    assert all(isinstance(properties["demand_quantity"], (int, float)) for properties in demand_features)
    demand_profile = next(
        feature_type
        for feature_type in data_ref.profile.feature_types
        if feature_type.value == "demand"
    )
    assert demand_profile.properties["duration_hours"] == "number"
    assert demand_profile.properties["demand_quantity"] == "number"
    assert "baseline_duration_hours" not in demand_profile.properties
    assert all(
        feature["geometry"]["type"] == "LineString"
        for feature in payload["features"]
        if feature["properties"]["kind"] in {"last_mile_assignment", "linehaul_connection"}
    )
    assert not list(workspace.rglob("*.json"))


def test_route_and_cost_tools_use_exact_pair_reuse(tmp_path: Path, monkeypatch) -> None:
    workspace, store = _runtime(tmp_path, monkeypatch)
    ctx = _context(workspace)
    prepared_ref = _prepared_ref(store)

    route_ref = _result_ref(
        server.prepare_route_matrix(
            prepared_ref,
            "haversine",
            ctx,
            detour_coefficient=1.2,
            average_speed_kph=40,
        )
    )
    original = server._runtime().load_model(route_ref, "route_matrix.v2", RouteMatrix)
    partial = original.model_copy(update={"rows": original.rows[:-2]})
    partial_ref = _resource_ref(store.publish(partial.schema_version, partial))
    completed_ref = _result_ref(
        server.prepare_route_matrix(
            prepared_ref,
            "haversine",
            ctx,
            detour_coefficient=1.2,
            average_speed_kph=40,
            prior_route_matrix_ref=partial_ref,
        )
    )
    completed = server._runtime().load_model(
        completed_ref,
        "route_matrix.v2",
        RouteMatrix,
    )
    assert completed.stats.reused_pair_count == len(original.rows) - 2
    assert completed.stats.computed_pair_count == 2
    assert completed.stats.missing_pair_count == 0


    cost_ref = _result_ref(
        server.plan_cost_matrix(
            prepared_ref,
            "all_warehouses",
            ctx,
            _cost_policy(),
            route_matrix_ref=completed_ref,
        )
    )
    original_cost = server._runtime().load_model(cost_ref, "cost_matrix.v2", CostMatrix)
    partial_cost = original_cost.model_copy(update={"rows": original_cost.rows[:-2]})
    partial_cost_ref = _resource_ref(store.publish(partial_cost.schema_version, partial_cost))
    completed_cost_ref = _result_ref(
        server.plan_cost_matrix(
            prepared_ref,
            "all_warehouses",
            ctx,
            _cost_policy(),
            route_matrix_ref=completed_ref,
            prior_cost_matrix_ref=partial_cost_ref,
        )
    )
    completed_cost = server._runtime().load_model(
        completed_cost_ref,
        "cost_matrix.v2",
        CostMatrix,
    )
    assert completed_cost.warehouse_scope == "all_warehouses"
    assert completed_cost.stats.reused_pair_count == len(original_cost.rows) - 2
    assert completed_cost.stats.computed_pair_count == 2
    assert completed_cost.stats.missing_pair_count == 0


def test_provided_route_tool_consumes_only_typed_normalized_facts(
    tmp_path: Path, monkeypatch
) -> None:
    workspace, store = _runtime(tmp_path, monkeypatch)
    fixture = network_case()
    computed = build_haversine_route_matrix(fixture.demand, fixture.warehouses, 1.2, 40)
    facts = [
        ProvidedRouteFactRecord(
            origin_id=row.origin_id,
            destination_id=row.destination_id,
            layer=row.layer,
            distance_km=row.distance_km,
            duration_hours=row.duration_hours,
            source_method="uploaded-estimate",
        )
        for row in computed.rows
    ]
    prepared_ref = _prepared_ref(store, provided_route_facts=facts)

    result_ref = _result_ref(
        server.prepare_route_matrix(
            prepared_ref,
            "provided",
            _context(workspace),
            warehouse_scope="all_warehouses",
        )
    )
    matrix = server._runtime().load_model(result_ref, "route_matrix.v2", RouteMatrix)

    assert len(matrix.rows) == 8
    assert matrix.missing_routes == []
    assert matrix.stats.provided_pair_count == 8
    assert matrix.stats.source_method_counts == {"uploaded-estimate": 8}


def test_existing_only_route_matrix_validates_without_candidate_routes(
    tmp_path: Path, monkeypatch
) -> None:
    workspace, store = _runtime(tmp_path, monkeypatch)
    fixture = network_case()
    computed = build_haversine_route_matrix(fixture.demand, fixture.warehouses, 1.2, 40)
    facts = [
        ProvidedRouteFactRecord(
            origin_id=row.origin_id,
            destination_id=row.destination_id,
            layer=row.layer,
            distance_km=row.distance_km,
            duration_hours=row.duration_hours,
            source_method="uploaded-estimate",
        )
        for row in computed.rows
    ]
    prepared_ref = _prepared_ref(store, provided_route_facts=facts)
    ctx = _context(workspace)

    matrix_ref = _result_ref(
        server.prepare_route_matrix(
            prepared_ref,
            "provided",
            ctx,
            warehouse_scope="existing_only",
        )
    )
    matrix = server._runtime().load_model(matrix_ref, "route_matrix.v2", RouteMatrix)

    assert matrix.warehouse_scope == "existing_only"
    assert matrix.stats.ignored_input_pair_count > 0
    assert matrix.missing_routes == []


def test_navigation_tool_merges_prior_workspace_file_and_reports_counts(
    tmp_path: Path, monkeypatch
) -> None:
    workspace, store = _runtime(tmp_path, monkeypatch)
    prepared_ref = _prepared_ref(store)
    fixture = network_case()
    haversine = build_haversine_route_matrix(fixture.demand, fixture.warehouses, 1.2, 40)
    rows = [
        row.model_copy(
            update={
                "method": "navigation",
                "tool_version": "navigation-test.v1",
                "detour_coefficient": None,
                "average_speed_kph": None,
                "navigation_provider": "test-provider",
                "navigation_profile": "truck",
            }
        )
        for row in haversine.rows
    ]
    prior = RouteMatrix(
        method="navigation",
        warehouse_scope="all_warehouses",
        rows=rows[:3],
        stats=NavigationRouteMatrixStats(
            route_count=3,
            reused_pair_count=0,
            registered_pair_count=3,
            missing_pair_count=5,
            complete=False,
        ),
    )
    prior_ref = _resource_ref(store.publish(prior.schema_version, prior))
    supplied = RouteMatrix(
        method="navigation",
        warehouse_scope="all_warehouses",
        rows=rows[3:],
        stats=NavigationRouteMatrixStats(
            route_count=len(rows) - 3,
            reused_pair_count=0,
            registered_pair_count=len(rows) - 3,
            missing_pair_count=3,
            complete=False,
        ),
    )
    (workspace / "navigation.json").write_text(
        json.dumps(supplied.model_dump(mode="json")),
        encoding="utf-8",
    )

    result_ref = _result_ref(
        server.register_navigation_route_matrix(
            prepared_ref,
            "navigation.json",
            _context(workspace),
            prior_ref,
        )
    )
    registered = server._runtime().load_model(
        result_ref,
        "route_matrix.v2",
        RouteMatrix,
    )
    assert registered.missing_routes == []
    assert registered.stats.reused_pair_count == 3
    assert registered.stats.registered_pair_count == len(rows) - 3

    mixed = supplied.model_copy(
        update={
            "rows": [
                supplied.rows[0].model_copy(update={"tool_version": "navigation-test.v2"}),
                *supplied.rows[1:],
            ]
        }
    )
    (workspace / "navigation.json").write_text(
        json.dumps(mixed.model_dump(mode="json")),
        encoding="utf-8",
    )
    with pytest.raises(ValueError, match="navigation_route_provenance_conflict"):
        server.register_navigation_route_matrix(
            prepared_ref,
            "navigation.json",
            _context(workspace),
            prior_ref,
        )


def test_navigation_tool_rejects_absolute_escape_and_symlink_paths(
    tmp_path: Path, monkeypatch
) -> None:
    workspace, store = _runtime(tmp_path, monkeypatch)
    prepared_ref = _prepared_ref(store)
    outside = tmp_path / "outside.json"
    outside.write_text("{}", encoding="utf-8")
    (workspace / "linked.json").symlink_to(outside)
    ctx = _context(workspace)

    for invalid_path, code in (
        (outside.as_posix(), "invalid_workspace_relative_path"),
        ("../outside.json", "invalid_workspace_relative_path"),
        ("linked.json", "workspace_source_symlink_rejected"),
    ):
        with pytest.raises(ValueError, match=code):
            server.register_navigation_route_matrix(
                prepared_ref,
                invalid_path,
                ctx,
            )


def test_network_tools_reject_non_ready_input_and_wrong_workspace(
    tmp_path: Path, monkeypatch
) -> None:
    workspace, store = _runtime(tmp_path, monkeypatch)
    ctx = _context(workspace)
    with pytest.raises(McpResourceContractError, match="normalized_input_not_ready"):
        server.prepare_route_matrix(
            _prepared_ref(store, state="needs_input"),
            "navigation",
            ctx,
        )

    other = tmp_path / "other"
    other.mkdir()
    with pytest.raises(McpResourceContractError, match="workspace_scope_mismatch"):
        server.prepare_route_matrix(
            _prepared_ref(store),
            "navigation",
            _context(other),
        )
