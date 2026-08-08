from __future__ import annotations

import pytest
from _network_fixtures import network_case

from supply_chain_planner.matrix import (
    build_haversine_route_matrix,
    plan_route_matrix,
    register_navigation_route_matrix,
    validate_route_matrix,
)
from supply_chain_planner.matrix_models import RouteMatrixRow


def test_haversine_plan_and_matrix_use_explicit_parameters() -> None:
    case = network_case()
    plan = plan_route_matrix(case.demand, case.warehouses, "haversine", 1.2, 40)
    matrix = build_haversine_route_matrix(case.demand, case.warehouses, 1.2, 40)

    assert plan.route_count == 6
    assert plan.estimated_billable_calls == 0
    assert len(matrix.rows) == plan.route_count
    assert matrix.rows[0].duration_hours > 0
    assert matrix.validation["detour_coefficient"] == 1.2


def test_haversine_requires_speed_and_detour_coefficient() -> None:
    with pytest.raises(ValueError, match="haversine_requires"):
        case = network_case()
        plan_route_matrix(case.demand, case.warehouses, "haversine", None, 40)


def test_navigation_registration_reports_missing_routes_without_filling_them() -> None:
    case = network_case()
    rows = [
        RouteMatrixRow(
            origin_id="center-a",
            destination_id="city-a",
            distance_km=1,
            duration_hours=0.1,
            method="navigation",
        )
    ]
    matrix = register_navigation_route_matrix(case.demand, case.warehouses, rows)
    validation = validate_route_matrix(case.demand, case.warehouses, matrix)

    assert len(matrix.missing_routes) == 5
    assert validation["valid"] is False
    assert validation["missing_routes"] == matrix.missing_routes
