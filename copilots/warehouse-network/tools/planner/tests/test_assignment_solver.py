from __future__ import annotations

import pytest
from _network_fixtures import (
    complete_cost_matrix,
    indonesia_current_assignments,
    indonesia_network_fixture,
    indonesia_route_quotes,
    network_case,
    route_matrix,
)
from supply_chain_planner.network.matrix import build_cost_matrix, build_haversine_route_matrix
from supply_chain_planner.network.matrix_models import (
    CostCalculationPolicy,
    CostMatrix,
    DemandUnitCostRule,
)
from supply_chain_planner.network.models import CurrentAssignmentRecord
from supply_chain_planner.network.optimization_models import (
    AssignmentResult,
    AssignmentRow,
    BaselineResult,
    ScenarioResult,
)
from supply_chain_planner.network.solver import (
    compare_assignments,
    solve_assignment,
    solve_current_assignment,
    summarize_assignment_cost,
)


def test_assignment_solver_selects_the_requested_objective() -> None:
    case = network_case()
    result = solve_assignment(
        case.demand,
        case.warehouses,
        route_matrix(case),
        complete_cost_matrix(case),
        "min_cost",
        {warehouse.warehouse_id for warehouse in case.warehouses},
    )

    assert result.unassigned_demand == 0
    assert {row.warehouse_id for row in result.rows} == {"center-a", "cross-b"}


def test_current_assignment_is_evaluated_without_reoptimization() -> None:
    case = network_case()
    current = [
        CurrentAssignmentRecord(demand_city_id="city-a", serving_warehouse_id="cross-b"),
        CurrentAssignmentRecord(demand_city_id="city-b", serving_warehouse_id="center-a"),
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
    assert result.rows[0].upstream_center_id == "center-a"
    assert result.rows[0].cost == 105
    assert result.unassigned_demand == 0


def test_min_cost_includes_crossdock_upstream_linehaul() -> None:
    case = network_case()
    costs = complete_cost_matrix(case)
    adjusted = []
    for row in costs.rows:
        key = (row.origin_id, row.destination_id, row.layer)
        if key == ("cross-b", "city-a", "last_mile"):
            adjusted.append(row.model_copy(update={"cost_per_demand_unit": 1}))
        elif key == ("center-a", "cross-b", "linehaul"):
            adjusted.append(row.model_copy(update={"cost_per_demand_unit": 100}))
        else:
            adjusted.append(row)
    result = solve_assignment(
        case.demand,
        case.warehouses,
        route_matrix(case),
        CostMatrix(currency="IDR", warehouse_scope="all_warehouses", rows=adjusted),
        "min_cost",
        {"center-a", "cross-b"},
    )

    city_a = next(row for row in result.rows if row.demand_city_id == "city-a")
    assert city_a.warehouse_id == "center-a"
    assert city_a.cost == 10


def test_cost_summary_ignores_matrix_gaps_for_unassigned_candidate_pairs() -> None:
    case = network_case()
    costs = complete_cost_matrix(case).model_copy(
        update={"missing_routes": [("candidate-c", "city-a", "last_mile")]}
    )
    assignment = solve_assignment(
        case.demand,
        case.warehouses,
        route_matrix(case),
        costs,
        "min_cost",
        {"center-a", "cross-b"},
    )

    summary = summarize_assignment_cost(assignment, costs)

    assert summary.complete is True
    assert summary.missing_routes == []


def test_current_assignment_rejects_conflicting_upstream_fact() -> None:
    case = network_case()
    current = [
        CurrentAssignmentRecord(
            demand_city_id="city-a",
            serving_warehouse_id="cross-b",
            upstream_center_id="wrong-center",
        )
    ]

    with pytest.raises(ValueError, match="current_assignment_upstream_mismatch"):
        solve_current_assignment(
            case.demand,
            case.warehouses,
            current,
            route_matrix(case),
            complete_cost_matrix(case),
            "min_cost",
        )


def test_sample3_comparison_only_closes_bekasi() -> None:
    before = AssignmentResult(
        objective="min_cost",
        rows=[
            AssignmentRow(
                demand_city_id="IDN-CITY-001",
                warehouse_id="WH-CENTER-JAKARTA",
                demand_quantity=10,
                duration_hours=1,
                cost=10,
            ),
            AssignmentRow(
                demand_city_id="IDN-CITY-003",
                warehouse_id="WH-CROSS_DOCKING-BEKASI",
                upstream_center_id="WH-CENTER-JAKARTA",
                demand_quantity=20,
                duration_hours=2,
                cost=20,
            ),
        ],
        total_demand=30,
        unassigned_demand=0,
    )
    after = AssignmentResult(
        objective="min_cost",
        rows=[
            before.rows[0],
            before.rows[1].model_copy(
                update={
                    "warehouse_id": "WH-CENTER-JAKARTA",
                    "upstream_center_id": None,
                    "duration_hours": 3,
                    "cost": 18,
                }
            ),
        ],
        total_demand=30,
        unassigned_demand=0,
    )

    comparison = compare_assignments(
        before,
        after,
        [2, 4],
        {"WH-CENTER-JAKARTA", "WH-CROSS_DOCKING-BEKASI"},
        {"WH-CENTER-JAKARTA"},
    )

    assert comparison.selected_warehouse_ids == []
    assert comparison.removed_warehouse_ids == ["WH-CROSS_DOCKING-BEKASI"]
    assert comparison.affected_city_ids == ["IDN-CITY-003"]
    assert comparison.reassigned_city_ids == ["IDN-CITY-003"]
    assert comparison.cost_delta == -40
    assert comparison.requested_service_targets == [2, 4]


def test_real_indonesia_sample3_only_closes_bekasi_without_candidates() -> None:
    fixture = indonesia_network_fixture()
    routes = build_haversine_route_matrix(fixture.demand, fixture.warehouses, 1.2, 42)
    costs = build_cost_matrix(
        fixture.demand,
        fixture.warehouses,
        indonesia_route_quotes(),
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
    before_active = {
        warehouse.warehouse_id
        for warehouse in fixture.warehouses
        if warehouse.is_existing
    }
    bekasi_id = "WH-CROSS_DOCKING-BEKASI"
    after_active = before_active - {bekasi_id}
    before = solve_current_assignment(
        fixture.demand,
        fixture.warehouses,
        indonesia_current_assignments(),
        routes,
        costs,
        "min_cost",
    )
    after = solve_assignment(
        fixture.demand,
        fixture.warehouses,
        routes,
        costs,
        "min_cost",
        after_active,
    )
    baseline = BaselineResult(
        label="actual_current",
        active_warehouse_ids=sorted(before_active),
        assignment=before,
    )
    scenario = ScenarioResult(
        active_warehouse_ids=sorted(after_active),
        assignment=after,
        warehouse_changes={"added": [], "removed": [bekasi_id]},
    )

    comparison = compare_assignments(
        baseline.assignment,
        scenario.assignment,
        [12],
        set(baseline.active_warehouse_ids),
        set(scenario.active_warehouse_ids),
    )

    candidate_ids = {
        warehouse.warehouse_id
        for warehouse in fixture.warehouses
        if not warehouse.is_existing
    }
    assigned_after = {
        row.warehouse_id for row in after.rows if row.warehouse_id is not None
    }
    assert len(comparison.city_changes) == 50
    assert comparison.requested_service_targets == [12]
    assert comparison.selected_warehouse_ids == []
    assert comparison.removed_warehouse_ids == [bekasi_id]
    assert set(baseline.active_warehouse_ids) == before_active
    assert set(scenario.active_warehouse_ids) == after_active
    assert set(scenario.active_warehouse_ids).isdisjoint(candidate_ids)
    assert assigned_after.isdisjoint(candidate_ids)
    assert len(comparison.affected_city_ids) == 50
    assert len(comparison.reassigned_city_ids) == 50
    assert set(comparison.reassigned_city_ids).issubset(comparison.affected_city_ids)


def test_comparison_rejects_different_demand_domains() -> None:
    before = AssignmentResult(
        objective="min_cost",
        rows=[
            AssignmentRow(
                demand_city_id="city-a",
                warehouse_id="warehouse-a",
                demand_quantity=10,
                duration_hours=1,
                cost=1,
            )
        ],
        total_demand=10,
        unassigned_demand=0,
    )
    after = before.model_copy(
        update={
            "rows": [before.rows[0].model_copy(update={"demand_quantity": 11})],
            "total_demand": 11,
        }
    )

    with pytest.raises(ValueError, match="comparison_assignment_domain_mismatch"):
        compare_assignments(
            before,
            after,
            [12],
            {"warehouse-a"},
            {"warehouse-a"},
        )
