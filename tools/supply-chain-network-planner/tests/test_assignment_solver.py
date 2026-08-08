from __future__ import annotations

from _network_fixtures import complete_cost_matrix, network_case, route_matrix

from supply_chain_planner.case_models import CurrentAssignment
from supply_chain_planner.solver import solve_assignment, solve_current_assignment


def test_assignment_solver_selects_the_requested_objective() -> None:
    case = network_case()
    result = solve_assignment(
        case.demand,
        case.warehouses,
        route_matrix(case),
        complete_cost_matrix(case),
        "min_cost",
    )

    assert result.unassigned_demand == 0
    assert {row.warehouse_id for row in result.rows} == {"center-a", "cross-b"}


def test_current_assignment_is_evaluated_without_reoptimization() -> None:
    case = network_case()
    current = [
        CurrentAssignment(demand_city_id="city-a", serving_warehouse_id="cross-b"),
        CurrentAssignment(demand_city_id="city-b", serving_warehouse_id="center-a"),
    ]
    result = solve_current_assignment(
        case.demand,
        case.warehouses,
        current,
        route_matrix(case),
        complete_cost_matrix(case),
        "min_cost",
    )

    assert [row.warehouse_id for row in result.rows] == ["cross-b", "center-a"]
    assert result.unassigned_demand == 0
