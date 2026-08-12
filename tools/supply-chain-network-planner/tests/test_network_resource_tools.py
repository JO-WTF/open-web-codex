from __future__ import annotations

import asyncio
import json
import sys
from contextlib import asynccontextmanager
from pathlib import Path
from types import SimpleNamespace

import pytest
from _network_fixtures import network_case

from supply_chain_planner import server
from supply_chain_planner.matrix import build_haversine_route_matrix
from supply_chain_planner.matrix_models import (
    CostCalculationPolicy,
    CostMatrix,
    DemandUnitCostRule,
    RouteMatrix,
)
from supply_chain_planner.mcp_contracts import MapResourceRef, ResourceRef
from supply_chain_planner.mcp_resources import (
    McpResourceContractError,
    McpResourceRuntime,
)
from supply_chain_planner.models import PreparedNetworkResource
from supply_chain_planner.network_models import ProvidedRouteFactRecord
from supply_chain_planner.resource_store import PublishedResource, ResourceStore


def _resource_ref(published: PublishedResource) -> ResourceRef:
    return ResourceRef(
        server=server.MCP_SERVER_NAME,
        resource_schema=published.schema,
        uri=published.uri,
    )


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
    return _resource_ref(store.publish(prepared.schema_version, prepared))


def _result_ref(result) -> ResourceRef:
    assert result.structuredContent is not None
    assert set(result.structuredContent) == {"summary", "resource_ref"}
    return ResourceRef.model_validate(result.structuredContent["resource_ref"])


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
        "plan_route_matrix",
        "build_haversine_route_matrix",
        "build_provided_route_matrix",
        "validate_route_matrix",
        "register_navigation_route_matrix",
        "plan_cost_matrix",
    ):
        schema = tools[name].inputSchema
        assert "ctx" not in schema["properties"]
        assert "normalized_input_ref" in schema["required"]

    plan = tools["plan_route_matrix"].inputSchema
    assert "prior_route_matrix_ref" not in plan["properties"]
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
    haversine = tools["build_haversine_route_matrix"].inputSchema
    assert "route_plan_ref" not in haversine["properties"]
    assert {"detour_coefficient", "average_speed_kph"}.issubset(haversine["required"])
    provided = tools["build_provided_route_matrix"].inputSchema
    assert "warehouse_scope" in provided["required"]
    assert "provided_route_facts" not in provided["properties"]
    navigation = tools["register_navigation_route_matrix"].inputSchema
    assert "navigation_result_relative_path" in navigation["required"]
    assert "navigation_result_ref" not in navigation["properties"]
    cost = tools["plan_cost_matrix"].inputSchema
    assert "warehouse_scope" in cost["required"]
    assert "calculation_policy" not in cost["required"]

    distribution = tools["prepare_network_distribution_map"].inputSchema
    assert "ctx" not in distribution["properties"]
    assert distribution["required"] == ["normalized_input_ref"]


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
    for name in ("_workspace_root", "_profile_state_root", "_mcp_resource_runtime"):
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
        "resource_ref",
        "data_ref",
        "feature_count",
        "layer_counts",
        "map_card_handoff",
    }
    data_ref = MapResourceRef.model_validate(result.structuredContent["data_ref"])
    assert data_ref.server == "supply_chain"
    assert data_ref.format == "geojson"
    resource_ref = ResourceRef.model_validate(result.structuredContent["resource_ref"])
    payload = store.load(resource_ref)
    assert payload["type"] == "FeatureCollection"
    kinds = [feature["properties"]["kind"] for feature in payload["features"]]
    fixture = network_case()
    expected_demands = len(fixture.demand)
    expected_existing = sum(warehouse.is_existing for warehouse in fixture.warehouses)
    assert kinds.count("demand") == expected_demands
    assert kinds.count("warehouse") == expected_existing
    assert result.structuredContent["layer_counts"] == {
        "demand": expected_demands,
        "existing_warehouses": expected_existing,
        "candidate_warehouses": 0,
    }
    handoff = result.structuredContent["map_card_handoff"]
    assert handoff["schemaVersion"] == "network_distribution_map_card_handoff.v1"
    assert handoff["tool"] == {
        "server": "map_utils",
        "name": "create_map_card",
    }
    arguments = handoff["arguments"]
    assert arguments["sources"] == {
        "network-distribution": {
            "type": "geojson",
            "data_ref": result.structuredContent["data_ref"],
        }
    }
    assert [layer["id"] for layer in arguments["layers"]] == [
        "demand-cities",
        "center-warehouses",
        "cross-docking-warehouses",
        "warehouse-labels",
    ]
    assert [item["label"] for item in arguments["extensions"]["legend"]["items"]] == [
        "Demand city",
        "Center warehouse",
        "Cross-docking warehouse",
    ]
    assert not list(workspace.rglob("*.json"))

    with_candidates = server.prepare_network_distribution_map(
        prepared_ref,
        ctx,
        include_candidates=True,
    )
    assert with_candidates.structuredContent is not None
    expected_candidates = sum(not warehouse.is_existing for warehouse in fixture.warehouses)
    assert (
        with_candidates.structuredContent["layer_counts"]["candidate_warehouses"]
        == expected_candidates
    )
    candidate_handoff = with_candidates.structuredContent["map_card_handoff"]
    assert "candidate-warehouses" in [
        layer["id"] for layer in candidate_handoff["arguments"]["layers"]
    ]
    assert candidate_handoff["arguments"]["extensions"]["legend"]["items"][-1] == {
        "label": "Candidate warehouse",
        "color": "#16A34A",
        "type": "circle",
    }


def test_route_and_cost_tools_use_exact_pair_reuse(tmp_path: Path, monkeypatch) -> None:
    workspace, store = _runtime(tmp_path, monkeypatch)
    ctx = _context(workspace)
    prepared_ref = _prepared_ref(store)

    plan_ref = _result_ref(server.plan_route_matrix(prepared_ref, "haversine", ctx, 1.2, 40))
    assert plan_ref.resource_schema == "route_matrix_plan.v2"

    route_ref = _result_ref(server.build_haversine_route_matrix(prepared_ref, 1.2, 40, ctx))
    original = server._runtime().load_model(route_ref, "route_matrix.v2", RouteMatrix)
    partial = original.model_copy(update={"rows": original.rows[:-2]})
    partial_ref = _resource_ref(store.publish(partial.schema_version, partial))
    completed_ref = _result_ref(
        server.build_haversine_route_matrix(
            prepared_ref,
            1.2,
            40,
            ctx,
            prior_route_matrix_ref=partial_ref,
        )
    )
    completed = server._runtime().load_model(
        completed_ref,
        "route_matrix.v2",
        RouteMatrix,
    )
    assert completed.validation["reused_pair_count"] == len(original.rows) - 2
    assert completed.validation["computed_pair_count"] == 2
    assert completed.validation["missing_pair_count"] == 0

    validation_ref = _result_ref(server.validate_route_matrix(prepared_ref, completed_ref, ctx))
    assert store.load(validation_ref)["valid"] is True

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
    assert completed_cost.validation["reused_pair_count"] == len(original_cost.rows) - 2
    assert completed_cost.validation["computed_pair_count"] == 2
    assert completed_cost.validation["missing_pair_count"] == 0


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
        server.build_provided_route_matrix(
            prepared_ref,
            "all_warehouses",
            _context(workspace),
        )
    )
    matrix = server._runtime().load_model(result_ref, "route_matrix.v2", RouteMatrix)

    assert len(matrix.rows) == 8
    assert matrix.missing_routes == []
    assert matrix.validation["provided_pair_count"] == 8
    assert matrix.validation["source_method_counts"] == {"uploaded-estimate": 8}


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

    matrix_ref = _result_ref(server.build_provided_route_matrix(prepared_ref, "existing_only", ctx))
    matrix = server._runtime().load_model(matrix_ref, "route_matrix.v2", RouteMatrix)
    validation_ref = _result_ref(server.validate_route_matrix(prepared_ref, matrix_ref, ctx))
    validation = store.load(validation_ref)

    assert matrix.warehouse_scope == "existing_only"
    assert matrix.validation["ignored_input_pair_count"] > 0
    assert matrix.missing_routes == []
    assert validation["valid"] is True
    assert validation["missing_routes"] == []


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
    )
    prior_ref = _resource_ref(store.publish(prior.schema_version, prior))
    supplied = RouteMatrix(
        method="navigation",
        warehouse_scope="all_warehouses",
        rows=rows[3:],
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
    assert registered.validation["reused_pair_count"] == 3
    assert registered.validation["registered_pair_count"] == len(rows) - 3

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
        server.plan_route_matrix(
            _prepared_ref(store, state="needs_input"),
            "navigation",
            ctx,
        )

    other = tmp_path / "other"
    other.mkdir()
    with pytest.raises(McpResourceContractError, match="workspace_scope_mismatch"):
        server.plan_route_matrix(
            _prepared_ref(store),
            "navigation",
            _context(other),
        )
