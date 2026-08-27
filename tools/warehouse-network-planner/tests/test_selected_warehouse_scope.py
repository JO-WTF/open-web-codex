from __future__ import annotations

import pytest
from _network_fixtures import TEST_INPUT_IDENTITY, indonesia_network_fixture, network_case
from supply_chain_planner.network.matrix import (
    build_navigation_matrix_request,
    resolve_warehouse_scope,
)
from supply_chain_planner.network.matrix_models import SelectedWarehousesScope


def test_selected_jakarta_center_builds_only_the_exact_fifty_last_mile_lanes() -> None:
    fixture = indonesia_network_fixture()

    request = build_navigation_matrix_request(
        fixture.demand,
        fixture.warehouses,
        warehouse_scope=SelectedWarehousesScope(warehouse_ids=[" WH-CENTER-JAKARTA "]),
        input_identity=TEST_INPUT_IDENTITY,
    )

    assert request.warehouse_scope == SelectedWarehousesScope(
        warehouse_ids=["WH-CENTER-JAKARTA"]
    )
    assert request.warehouse_ids == ["WH-CENTER-JAKARTA"]
    assert len(request.routes) == 50
    assert {route.layer for route in request.routes} == {"last_mile"}
    assert {route.origin_id for route in request.routes} == {"WH-CENTER-JAKARTA"}
    assert request.estimated_billable_elements == 50


def test_selected_warehouse_scope_rejects_duplicate_and_unknown_ids() -> None:
    fixture = indonesia_network_fixture()

    with pytest.raises(ValueError, match="warehouse_scope_selected_warehouse_ids_duplicate"):
        SelectedWarehousesScope(warehouse_ids=["WH-CENTER-JAKARTA", " WH-CENTER-JAKARTA "])
    with pytest.raises(ValueError, match="warehouse_scope_selected_warehouse_unknown:WH-UNKNOWN"):
        resolve_warehouse_scope(
            fixture.warehouses,
            SelectedWarehousesScope(warehouse_ids=["WH-UNKNOWN"]),
        )


def test_selected_cross_dock_does_not_implicitly_add_its_upstream_center() -> None:
    fixture = network_case()

    with pytest.raises(ValueError, match="warehouse_upstream_center_missing:cross-b"):
        build_navigation_matrix_request(
            fixture.demand,
            fixture.warehouses,
            warehouse_scope=SelectedWarehousesScope(warehouse_ids=["cross-b"]),
            input_identity=TEST_INPUT_IDENTITY,
        )
