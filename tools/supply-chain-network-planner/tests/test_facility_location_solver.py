from __future__ import annotations

from _network_fixtures import complete_cost_matrix, network_case, route_matrix

from supply_chain_planner.solver import enumerate_p_median


def test_p_median_keeps_fixed_existing_warehouses_and_opens_requested_candidates() -> None:
    case = network_case()
    best, _, timed_out = enumerate_p_median(
        case.demand,
        case.warehouses,
        route_matrix(case),
        complete_cost_matrix(case),
        number_to_open=1,
        fixed_existing_ids=set(),
        optional_existing_ids=set(),
        time_limit_seconds=5,
    )

    assert timed_out is False
    assert best is not None
    _, selected, assignment = best
    assert {"center-a", "cross-b"}.issubset(selected)
    assert "candidate-c" in selected
    assert assignment.unassigned_demand == 0
