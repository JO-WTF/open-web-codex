from __future__ import annotations

import pytest
from _network_fixtures import (
    indonesia_network_fixture,
    indonesia_route_quotes,
    network_case,
    route_matrix,
)
from pydantic import ValidationError
from supply_chain_planner.network.matrix import build_cost_matrix, build_haversine_route_matrix
from supply_chain_planner.network.matrix_models import (
    CostCalculationPolicy,
    CostMatrix,
    DemandUnitCostRule,
    RouteCostQuote,
)


def _cost_policy(currency: str = "IDR") -> CostCalculationPolicy:
    return CostCalculationPolicy(
        rules=[
            DemandUnitCostRule(
                layer=layer,
                currency=currency,
                fixed_cost_per_demand_unit=5,
                cost_per_km_per_demand_unit=2,
            )
            for layer in ("last_mile", "linehaul")
        ]
    )


def test_cost_matrix_preserves_linehaul_quotes_and_reports_missing_last_mile() -> None:
    case = network_case()
    quotes = [
        RouteCostQuote(
            origin_id="center-a",
            destination_id="city-a",
            layer="last_mile",
            price_per_vehicle=100,
            currency="IDR",
            vehicle_capacity=1,
        ),
        RouteCostQuote(
            origin_id="center-a",
            destination_id="cross-b",
            layer="linehaul",
            price_per_vehicle=500,
            currency="IDR",
            vehicle_capacity=1,
        ),
    ]

    matrix = build_cost_matrix(
        case.demand,
        case.warehouses,
        quotes,
        None,
        warehouse_scope="all_warehouses",
    )

    assert ("center-a", "cross-b", "linehaul") not in matrix.missing_routes
    assert any(row.layer == "linehaul" for row in matrix.rows)
    assert ("cross-b", "city-a", "last_mile") in matrix.missing_routes


def test_cost_matrix_calculates_missing_quote_only_from_explicit_rule() -> None:
    case = network_case()
    matrix = build_cost_matrix(
        case.demand,
        case.warehouses,
        [],
        _cost_policy(),
        route_matrix(case),
        warehouse_scope="all_warehouses",
    )

    assert matrix.missing_routes == []
    assert all(row.source == "calculated" for row in matrix.rows)
    assert all(row.cost_per_demand_unit > 0 for row in matrix.rows)


def test_cost_reuse_is_pair_exact_and_currency_mismatch_is_rejected() -> None:
    case = network_case()
    routes = route_matrix(case)
    original = build_cost_matrix(
        case.demand,
        case.warehouses,
        [],
        _cost_policy(),
        routes,
        warehouse_scope="all_warehouses",
    )
    exact = original.rows[0]
    stale = original.rows[1].model_copy(update={"tool_version": "distance-unit-cost.v0"})
    assert original.rows[2].route_fact is not None
    stale_route_fact = original.rows[2].model_copy(
        update={
            "route_fact": original.rows[2].route_fact.model_copy(
                update={"tool_version": "haversine.v0"}
            )
        }
    )
    stale_cost = original.rows[3].model_copy(
        update={"cost_per_demand_unit": original.rows[3].cost_per_demand_unit + 1}
    )
    unrelated = original.rows[4].model_copy(
        update={"origin_id": "not-required", "destination_id": "also-not-required"}
    )

    rebuilt = build_cost_matrix(
        case.demand,
        case.warehouses,
        [],
        _cost_policy(),
        routes,
        [exact, stale, stale_route_fact, stale_cost, unrelated],
        warehouse_scope="all_warehouses",
    )

    assert rebuilt.stats.reused_pair_count == 1
    assert rebuilt.stats.stale_pair_count == 3
    assert rebuilt.stats.ignored_prior_row_count == 1
    assert rebuilt.stats.computed_pair_count == 7

    quote = RouteCostQuote(
        origin_id="center-a",
        destination_id="city-a",
        layer="last_mile",
        price_per_vehicle=100,
        currency="USD",
        vehicle_capacity=1,
    )
    with pytest.raises(ValueError, match="cost_currency_mismatch"):
        build_cost_matrix(
            case.demand,
            case.warehouses,
            [quote],
            _cost_policy("IDR"),
            routes,
            warehouse_scope="all_warehouses",
        )


def test_unrelated_quote_is_ignored_without_invalidating_required_pairs() -> None:
    case = network_case()
    extra = RouteCostQuote(
        origin_id="center-a",
        destination_id="not-required",
        layer="linehaul",
        price_per_vehicle=1,
        currency="IDR",
        vehicle_capacity=1,
    )

    matrix = build_cost_matrix(
        case.demand,
        case.warehouses,
        [extra],
        _cost_policy(),
        route_matrix(case),
        warehouse_scope="all_warehouses",
    )

    assert matrix.missing_routes == []
    assert matrix.stats.ignored_quote_count == 1


def test_linehaul_without_quote_or_explicit_layer_rule_is_typed_missing() -> None:
    case = network_case()
    policy = CostCalculationPolicy(
        rules=[
            DemandUnitCostRule(
                layer="last_mile",
                currency="IDR",
                fixed_cost_per_demand_unit=5,
                cost_per_km_per_demand_unit=2,
            )
        ]
    )

    matrix = build_cost_matrix(
        case.demand,
        case.warehouses,
        [],
        policy,
        route_matrix(case),
        warehouse_scope="all_warehouses",
    )

    assert ("center-a", "cross-b", "linehaul") in matrix.missing_routes
    assert ("center-a", "candidate-c", "linehaul") in matrix.missing_routes


def test_conflicting_prior_cost_rows_are_rejected_per_pair() -> None:
    case = network_case()
    routes = route_matrix(case)
    original = build_cost_matrix(
        case.demand,
        case.warehouses,
        [],
        _cost_policy(),
        routes,
        warehouse_scope="all_warehouses",
    )

    with pytest.raises(ValueError, match="cost_prior_duplicate_pair"):
        build_cost_matrix(
            case.demand,
            case.warehouses,
            [],
            _cost_policy(),
            routes,
            [original.rows[0], original.rows[0]],
            warehouse_scope="all_warehouses",
        )


def test_cost_currency_has_no_hidden_default() -> None:
    with pytest.raises(ValidationError):
        CostMatrix(warehouse_scope="all_warehouses", rows=[])
    with pytest.raises(ValidationError):
        CostMatrix(currency="IDR", rows=[])


def test_indonesia_extra_linehaul_quotes_do_not_invalidate_required_pairs() -> None:
    fixture = indonesia_network_fixture()
    routes = build_haversine_route_matrix(fixture.demand, fixture.warehouses, 1.2, 42)

    matrix = build_cost_matrix(
        fixture.demand,
        fixture.warehouses,
        indonesia_route_quotes(),
        _cost_policy(),
        routes,
        warehouse_scope="all_warehouses",
    )

    assert matrix.missing_routes == []
    assert matrix.stats.ignored_quote_count == 24
    assert matrix.stats.expected_pair_count == 1_168
    assert matrix.stats.computed_pair_count == 1_168
