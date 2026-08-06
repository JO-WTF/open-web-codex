from __future__ import annotations

import json
from decimal import Decimal
from pathlib import Path

import pytest

from supply_chain_planner.core import (
    compare_scenarios,
    create_route_matrix,
    create_snapshot,
    evaluate_current_assignment,
    evaluate_optimized_network,
    solve_location_candidates,
)
from supply_chain_planner.decision_core import (
    build_risk_register,
    evaluate_financial_case,
)
from supply_chain_planner.models import (
    DataRef,
    NetworkInput,
    RiskItem,
    RouteEntry,
)

ROOT = Path(__file__).resolve().parents[1]


@pytest.fixture
def network() -> NetworkInput:
    payload = json.loads((ROOT / "examples" / "network-input.json").read_text())
    return NetworkInput.model_validate(payload)


@pytest.fixture
def planning_state(network: NetworkInput):
    snapshot = create_snapshot(network, source_name="test")
    payload = json.loads((ROOT / "examples" / "route-matrix-input.json").read_text())
    matrix = create_route_matrix(
        snapshot,
        provider=payload["provider"],
        method=payload["method"],
        entries=[RouteEntry.model_validate(item) for item in payload["entries"]],
    )
    return snapshot, matrix


def test_current_and_existing_footprint_coverage_are_distinct(planning_state) -> None:
    snapshot, matrix = planning_state

    actual = evaluate_current_assignment(snapshot, matrix)
    optimized = evaluate_optimized_network(
        snapshot,
        matrix,
        active_facility_ids=["warehouse-shanghai", "warehouse-nanjing"],
        scenario_id="baseline",
    )

    assert actual.metrics.covered_demand_units == 75
    assert actual.metrics.coverage_ratio == pytest.approx(0.75)
    assert any("exceeds capacity" in issue for issue in actual.issues)
    assert optimized.metrics.covered_demand_units == 70
    assert optimized.metrics.coverage_ratio == pytest.approx(0.70)
    assert sum(item.units for item in optimized.allocations) == 100


def test_add_warehouse_scenario_and_comparison(planning_state) -> None:
    snapshot, matrix = planning_state
    baseline = evaluate_optimized_network(
        snapshot,
        matrix,
        active_facility_ids=["warehouse-shanghai", "warehouse-nanjing"],
        scenario_id="baseline",
    )
    candidate = evaluate_optimized_network(
        snapshot,
        matrix,
        active_facility_ids=[
            "warehouse-shanghai",
            "warehouse-nanjing",
            "candidate-hangzhou",
        ],
        scenario_id="add-hangzhou",
    )

    comparison = compare_scenarios(baseline, candidate)

    assert candidate.metrics.coverage_ratio == pytest.approx(0.95)
    assert comparison.coverage_ratio_delta == pytest.approx(0.25)
    assert comparison.covered_demand_units_delta == 25


def test_location_solver_finds_fewest_then_lowest_cost_candidate(planning_state) -> None:
    snapshot, matrix = planning_state

    result, selected, evaluated, feasible = solve_location_candidates(
        snapshot,
        matrix,
        target_coverage_ratio=0.90,
    )

    assert feasible is True
    assert selected == ["candidate-hangzhou"]
    assert result.metrics.coverage_ratio == pytest.approx(0.95)
    assert evaluated == 3


def test_location_solver_returns_best_plan_when_target_is_infeasible(
    planning_state,
) -> None:
    snapshot, matrix = planning_state
    for facility in snapshot.facilities:
        if not facility.is_existing:
            facility.capacity_units = 0

    result, selected, evaluated, feasible = solve_location_candidates(
        snapshot,
        matrix,
        target_coverage_ratio=0.90,
    )

    assert feasible is False
    assert selected == []
    assert result.metrics.coverage_ratio == pytest.approx(0.70)
    assert evaluated == 4


def test_route_matrix_rejects_duplicate_pairs(network: NetworkInput) -> None:
    snapshot = create_snapshot(network)
    duplicate = RouteEntry(
        origin_city_id="city-shanghai",
        destination_city_id="city-shanghai",
        distance_meters=1,
        travel_seconds=1,
    )

    with pytest.raises(ValueError, match="duplicate route pair"):
        create_route_matrix(
            snapshot,
            provider="test",
            method="navigation",
            entries=[duplicate, duplicate],
        )


def test_network_requires_complete_rate_resolution() -> None:
    payload = json.loads((ROOT / "examples" / "network-input.json").read_text())
    payload["transport_rates"] = payload["transport_rates"][:-1]

    with pytest.raises(ValueError, match="missing transport rate"):
        NetworkInput.model_validate(payload)


def test_city_lane_is_reused_by_multiple_demand_points() -> None:
    payload = json.loads((ROOT / "examples" / "network-input.json").read_text())
    second_demand = dict(payload["demand_points"][0])
    second_demand["demand_id"] = "demand-shanghai-02"
    second_demand["demand_units"] = 1
    payload["demand_points"].append(second_demand)
    network = NetworkInput.model_validate(payload)
    snapshot = create_snapshot(network)
    route_payload = json.loads((ROOT / "examples" / "route-matrix-input.json").read_text())

    matrix = create_route_matrix(
        snapshot,
        provider=route_payload["provider"],
        method=route_payload["method"],
        entries=[RouteEntry.model_validate(item) for item in route_payload["entries"]],
    )

    assert len(matrix.entries) == 12
    assert {(entry.origin_city_id, entry.destination_city_id) for entry in matrix.entries} == {
        (facility.city_id, demand.city_id)
        for facility in snapshot.facilities
        for demand in snapshot.demand_points
    }


def test_financial_and_risk_resources_preserve_evidence_lineage(
    planning_state,
) -> None:
    snapshot, matrix = planning_state
    for facility in snapshot.facilities:
        if facility.facility_id == "candidate-hangzhou":
            facility.opening_cost = Decimal("1000")
    baseline = evaluate_optimized_network(
        snapshot,
        matrix,
        active_facility_ids=["warehouse-shanghai", "warehouse-nanjing"],
        scenario_id="baseline",
    )
    candidate = evaluate_optimized_network(
        snapshot,
        matrix,
        active_facility_ids=[
            "warehouse-shanghai",
            "warehouse-nanjing",
            "candidate-hangzhou",
        ],
        scenario_id="candidate",
    )
    snapshot_ref = DataRef(
        uri="supply-chain://resources/network_snapshot.v1-test",
        resource_schema="network_snapshot.v1",
    )
    baseline_ref = DataRef(
        uri="supply-chain://resources/network_scenario_result.v1-baseline",
        resource_schema="network_scenario_result.v1",
    )
    candidate_ref = DataRef(
        uri="supply-chain://resources/network_scenario_result.v1-candidate",
        resource_schema="network_scenario_result.v1",
    )

    finance = evaluate_financial_case(
        snapshot,
        baseline,
        candidate,
        snapshot_ref=snapshot_ref,
        baseline_result_ref=baseline_ref,
        candidate_result_ref=candidate_ref,
        horizon_years=5,
        discount_rate=0.1,
        annual_growth_rate=0.02,
    )
    assert finance.schema_version == "financial_evaluation.v1"
    assert finance.opening_investment == Decimal("1000")
    assert finance.input_refs == [snapshot_ref, baseline_ref, candidate_ref]

    risk_register = build_risk_register(
        decision_scope="Test candidate decision",
        risks=[
            RiskItem(
                risk_id="demand-volatility",
                category="demand",
                statement="Promotion demand may not recur.",
                likelihood=4,
                impact=4,
                mitigation="Use a normalized demand case before approval.",
                trigger="Two quarters of normalized demand below plan.",
                evidence_refs=[candidate_ref],
            )
        ],
    )
    assert risk_register.schema_version == "risk_register.v1"
    assert risk_register.unresolved_risk_count == 1
