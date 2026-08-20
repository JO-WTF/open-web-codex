from __future__ import annotations

import pytest
from _network_fixtures import (
    TEST_INPUT_IDENTITY,
    complete_cost_matrix,
    indonesia_network_fixture,
    network_case,
    route_matrix,
)
from pydantic import ValidationError
from supply_chain_planner.network.matrix import build_cost_matrix, build_haversine_route_matrix
from supply_chain_planner.network.matrix_models import CostCalculationPolicy, DemandUnitCostRule
from supply_chain_planner.network.optimization_models import (
    ExactOpeningPolicy,
    PMedianRequest,
    PMedianSolution,
    ScenarioSpec,
)
from supply_chain_planner.network.solver import (
    enumerate_p_median,
    service_metrics,
    summarize_assignment_cost,
)


def _with_identity(function):
    def call(*args, **kwargs):
        kwargs.setdefault("input_identity", TEST_INPUT_IDENTITY)
        return function(*args, **kwargs)

    return call


build_cost_matrix = _with_identity(build_cost_matrix)
build_haversine_route_matrix = _with_identity(build_haversine_route_matrix)


def test_p_median_keeps_fixed_existing_warehouses_and_opens_requested_candidates() -> None:
    case = network_case()
    best, _, timed_out = enumerate_p_median(
        case.demand,
        case.warehouses,
        route_matrix(case),
        complete_cost_matrix(case),
        number_to_open=1,
        fixed_existing_ids={"center-a", "cross-b"},
        optional_existing_ids=set(),
        time_limit_seconds=5,
    )

    assert timed_out is False
    assert best is not None
    assert {"center-a", "cross-b"}.issubset(best.active_warehouse_ids)
    assert best.opened_candidate_ids == ["candidate-c"]
    assert best.closed_existing_ids == []
    assert best.assignment.unassigned_demand == 0


def test_p_median_requires_complete_explicit_existing_policy() -> None:
    case = network_case()

    with pytest.raises(ValueError, match="existing_policy_incomplete"):
        enumerate_p_median(
            case.demand,
            case.warehouses,
            route_matrix(case),
            complete_cost_matrix(case),
            number_to_open=1,
            fixed_existing_ids=set(),
            optional_existing_ids=set(),
            time_limit_seconds=5,
        )


def test_sample2_opens_exactly_two_candidates_with_existing_sites_explicitly_fixed() -> None:
    fixture = indonesia_network_fixture()
    routes = build_haversine_route_matrix(fixture.demand, fixture.warehouses, 1.2, 42)
    costs = build_cost_matrix(
        fixture.demand,
        fixture.warehouses,
        [],
        CostCalculationPolicy(
            rules=[
                DemandUnitCostRule(
                    layer=layer,
                    currency="IDR",
                    fixed_cost_per_demand_unit=40_000,
                    cost_per_km_per_demand_unit=1_500,
                )
                for layer in ("last_mile", "linehaul")
            ]
        ),
        routes,
        warehouse_scope="all_warehouses",
    )
    fixed_existing = {
        warehouse.warehouse_id for warehouse in fixture.warehouses if warehouse.is_existing
    }

    result, _, timed_out = enumerate_p_median(
        fixture.demand,
        fixture.warehouses,
        routes,
        costs,
        number_to_open=2,
        fixed_existing_ids=fixed_existing,
        optional_existing_ids=set(),
        time_limit_seconds=10,
    )

    assert timed_out is False
    assert result is not None
    assert len(result.opened_candidate_ids) == 2
    assert fixed_existing.issubset(result.active_warehouse_ids)
    assert result.closed_existing_ids == []
    solution = PMedianSolution(
        status="optimal",
        active_warehouse_ids=result.active_warehouse_ids,
        opened_candidate_ids=result.opened_candidate_ids,
        closed_existing_ids=result.closed_existing_ids,
        assignment=result.assignment,
        objective_value=result.objective_value,
        cost=summarize_assignment_cost(result.assignment, costs),
        input_identity=TEST_INPUT_IDENTITY,
        service=service_metrics(result.assignment, [6, 12, 18]),
        optimality="proven",
        opening_policy=ExactOpeningPolicy(number_to_open=2),
    )
    assert solution.cost is not None
    assert solution.cost.complete is True
    assert solution.cost.total == solution.cost.linehaul + solution.cost.last_mile
    assert solution.cost.by_warehouse
    assert solution.cost.linehaul_by_warehouse
    assert solution.cost.last_mile_by_warehouse
    assert [metric.target_hours for metric in solution.service] == [6, 12, 18]


def test_planning_objective_and_existing_policy_have_no_hidden_defaults() -> None:
    with pytest.raises(ValidationError):
        ScenarioSpec()
    with pytest.raises(ValidationError):
        PMedianRequest(opening_policy={"kind": "exact"})
    with pytest.raises(ValidationError):
        PMedianRequest(
            opening_policy=ExactOpeningPolicy(number_to_open=1),
            existing_warehouse_policy={"mode": "allow_closure"},
        )
