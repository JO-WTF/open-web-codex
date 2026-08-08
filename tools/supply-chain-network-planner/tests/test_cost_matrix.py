from __future__ import annotations

from _network_fixtures import network_case, route_matrix

from supply_chain_planner.matrix import build_cost_matrix
from supply_chain_planner.matrix_models import RouteCostQuote


def test_cost_matrix_preserves_linehaul_quotes_and_reports_missing_last_mile() -> None:
    case = network_case()
    quotes = [
        RouteCostQuote(
            origin_id="center-a",
            destination_id="city-a",
            layer="last_mile",
            price_per_vehicle=100,
            currency="IDR",
        ),
        RouteCostQuote(
            origin_id="center-a",
            destination_id="cross-b",
            layer="linehaul",
            price_per_vehicle=500,
            currency="IDR",
        ),
    ]

    matrix = build_cost_matrix(case.demand, case.warehouses, quotes, None)

    assert ("center-a", "cross-b", "linehaul") not in matrix.missing_routes
    assert any(row.layer == "linehaul" for row in matrix.rows)
    assert ("cross-b", "city-a", "last_mile") in matrix.missing_routes


def test_cost_matrix_calculates_missing_quote_only_from_explicit_rule() -> None:
    case = network_case()
    matrix = build_cost_matrix(
        case.demand,
        case.warehouses,
        [],
        {"currency": "IDR", "fixed_price": 5, "price_per_km": 2},
        route_matrix(case),
    )

    assert matrix.missing_routes == []
    assert all(row.source == "calculated" for row in matrix.rows)
    assert all(row.cost_per_demand_unit > 0 for row in matrix.rows)
