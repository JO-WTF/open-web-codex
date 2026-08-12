from __future__ import annotations

import json
from collections import Counter
from dataclasses import dataclass
from pathlib import Path

import pytest
from _network_fixtures import (
    indonesia_current_assignments,
    indonesia_network_fixture,
)
from jsonschema import Draft202012Validator
from pydantic import ValidationError

from supply_chain_planner.delivery_schemas import model_schema
from supply_chain_planner.map_service import (
    NetworkComparisonMapBundle,
    build_network_comparison_map_bundle,
)
from supply_chain_planner.matrix import build_cost_matrix, build_haversine_route_matrix
from supply_chain_planner.matrix_models import CostCalculationPolicy, DemandUnitCostRule
from supply_chain_planner.network_models import NormalizedInputBatch
from supply_chain_planner.optimization_models import (
    AssignmentComparison,
    BaselineResult,
    PMedianSolution,
)
from supply_chain_planner.report_service import (
    NETWORK_PLANNING_MARKDOWN_MARKER,
    NetworkPlanningReportBundle,
    build_network_planning_report_bundle,
    render_network_planning_report_markdown,
)
from supply_chain_planner.solver import (
    compare_assignments,
    coverage_metrics,
    enumerate_p_median,
    service_metrics,
    solve_current_assignment,
    summarize_assignment_cost,
)


@dataclass(frozen=True)
class Sample2Delivery:
    normalized: NormalizedInputBatch
    baseline: BaselineResult
    facility: PMedianSolution
    comparison: AssignmentComparison


@pytest.fixture(scope="module")
def sample2_delivery() -> Sample2Delivery:
    fixture = indonesia_network_fixture()
    routes = build_haversine_route_matrix(
        fixture.demand,
        fixture.warehouses,
        detour_coefficient=1.2,
        average_speed_kph=42,
    )
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
    existing_ids = {
        warehouse.warehouse_id for warehouse in fixture.warehouses if warehouse.is_existing
    }
    current_assignments = indonesia_current_assignments()
    baseline_assignment = solve_current_assignment(
        fixture.demand,
        fixture.warehouses,
        current_assignments,
        routes,
        costs,
        "min_cost",
    )
    targets = [6, 12, 18]
    baseline = BaselineResult(
        label="actual_current",
        active_warehouse_ids=sorted(existing_ids),
        assignment=baseline_assignment,
        service=service_metrics(baseline_assignment, targets),
        coverage=coverage_metrics(baseline_assignment, targets),
        cost=summarize_assignment_cost(baseline_assignment, costs),
    )
    best, _, timed_out = enumerate_p_median(
        fixture.demand,
        fixture.warehouses,
        routes,
        costs,
        number_to_open=2,
        fixed_existing_ids=existing_ids,
        optional_existing_ids=set(),
        time_limit_seconds=10,
    )
    assert timed_out is False
    assert best is not None
    facility = PMedianSolution(
        status="optimal",
        active_warehouse_ids=best.active_warehouse_ids,
        opened_candidate_ids=best.opened_candidate_ids,
        closed_existing_ids=best.closed_existing_ids,
        assignment=best.assignment,
        objective_value=best.objective_value,
        cost=summarize_assignment_cost(best.assignment, costs),
        service=service_metrics(best.assignment, targets),
        optimality="proven",
    )
    comparison = compare_assignments(
        baseline.assignment,
        best.assignment,
        targets,
        existing_ids,
        set(best.active_warehouse_ids),
    )
    return Sample2Delivery(
        normalized=NormalizedInputBatch(
            demand_cities=fixture.demand,
            warehouses=fixture.warehouses,
            current_assignments=current_assignments,
            route_quotes=[],
        ),
        baseline=baseline,
        facility=facility,
        comparison=comparison,
    )


def test_sample2_builds_complete_self_contained_map_and_report(
    sample2_delivery: Sample2Delivery,
) -> None:
    inputs = sample2_delivery
    map_bundle = build_network_comparison_map_bundle(
        inputs.normalized,
        inputs.baseline,
        inputs.facility,
        inputs.comparison,
        country_code="ID",
    )
    report_bundle = build_network_planning_report_bundle(
        inputs.normalized,
        inputs.baseline,
        inputs.facility,
        inputs.comparison,
        country_code="ID",
    )

    feature_kinds = Counter(feature.properties.kind for feature in map_bundle.geojson.features)
    expected_linehaul = sum(
        1
        for warehouse in inputs.normalized.warehouses
        if warehouse.warehouse_type == "cross_docking"
        and warehouse.warehouse_id in set(inputs.baseline.active_warehouse_ids)
    ) + sum(
        1
        for warehouse in inputs.normalized.warehouses
        if warehouse.warehouse_type == "cross_docking"
        and warehouse.warehouse_id in set(inputs.facility.active_warehouse_ids)
    )
    assert feature_kinds == {
        "warehouse": 23,
        "demand": 50,
        "last_mile_assignment": 100,
        "linehaul_connection": expected_linehaul,
    }
    assert map_bundle.summary.feature_count == len(map_bundle.geojson.features)
    assert len({feature.id for feature in map_bundle.geojson.features}) == len(
        map_bundle.geojson.features
    )
    assert len(map_bundle.summary.opened_candidate_ids) == 2
    assert map_bundle.summary.closed_existing_ids == []
    assert inputs.baseline.label == "actual_current"
    assert len(inputs.normalized.current_assignments) == 50
    assert set(inputs.baseline.active_warehouse_ids) == {
        warehouse.warehouse_id
        for warehouse in inputs.normalized.warehouses
        if warehouse.is_existing
    }

    baseline_assigned = {
        row.warehouse_id for row in inputs.baseline.assignment.rows if row.warehouse_id is not None
    }
    zero_demand_active = set(inputs.baseline.active_warehouse_ids) - baseline_assigned
    assert zero_demand_active
    warehouse_properties = {
        feature.properties.warehouse_id: feature.properties
        for feature in map_bundle.geojson.features
        if feature.properties.kind == "warehouse"
    }
    assert all(
        warehouse_properties[warehouse_id].baseline_active for warehouse_id in zero_demand_active
    )
    assert report_bundle.scope.model_dump(mode="python") == {
        "demand_city_count": 50,
        "warehouse_count": 23,
        "existing_warehouse_count": 11,
        "candidate_warehouse_count": 12,
        "total_demand": inputs.baseline.assignment.total_demand,
    }
    assert len(report_bundle.entities.demand_cities) == 50
    assert len(report_bundle.entities.warehouses) == 23
    assert len(report_bundle.baseline.assignment.rows) == 50
    assert len(report_bundle.facility.assignment.rows) == 50
    assert len(report_bundle.facility.opened_candidate_ids) == 2
    assert report_bundle.facility.closed_existing_ids == []
    assert [metric.target_hours for metric in report_bundle.facility.service] == [
        6,
        12,
        18,
    ]
    assert report_bundle.baseline.cost is not None
    assert report_bundle.baseline.cost.complete is True
    assert report_bundle.facility.cost is not None
    assert report_bundle.facility.cost.complete is True
    assert report_bundle.facility.cost.total == (
        report_bundle.facility.cost.linehaul + report_bundle.facility.cost.last_mile
    )
    assert report_bundle.facility.cost.by_warehouse

    map_payload = map_bundle.model_dump(mode="json")
    report_payload = report_bundle.model_dump(mode="json")
    assert NetworkComparisonMapBundle.model_validate(map_payload) == map_bundle
    assert NetworkPlanningReportBundle.model_validate(report_payload) == report_bundle
    _assert_no_external_delivery_identity(map_payload)
    _assert_no_external_delivery_identity(report_payload)


def test_delivery_schema_fixtures_match_models_and_validate_complete_indonesia_bundle(
    sample2_delivery: Sample2Delivery,
) -> None:
    inputs = sample2_delivery
    bundles = {
        "network_comparison_map_bundle.v1": build_network_comparison_map_bundle(
            inputs.normalized,
            inputs.baseline,
            inputs.facility,
            inputs.comparison,
            country_code="ID",
        ),
        "network_planning_report_bundle.v1": build_network_planning_report_bundle(
            inputs.normalized,
            inputs.baseline,
            inputs.facility,
            inputs.comparison,
            country_code="ID",
        ),
    }
    fixture_root = Path(__file__).parents[1] / "contracts" / "schemas"
    for schema_name, bundle in bundles.items():
        schema = json.loads(
            (fixture_root / f"{schema_name}.schema.json").read_text(encoding="utf-8")
        )
        payload_fixture = json.loads(
            (fixture_root.parent / "fixtures" / f"{schema_name}.json").read_text(encoding="utf-8")
        )
        assert payload_fixture == bundle.model_dump(mode="json")
        assert schema == model_schema(schema_name)
        Draft202012Validator.check_schema(schema)
        Draft202012Validator(schema).validate(bundle.model_dump(mode="json"))


def test_delivery_models_reject_unknown_nested_fields(
    sample2_delivery: Sample2Delivery,
) -> None:
    inputs = sample2_delivery
    bundles = [
        build_network_comparison_map_bundle(
            inputs.normalized,
            inputs.baseline,
            inputs.facility,
            inputs.comparison,
            country_code="ID",
        ).model_dump(mode="json"),
        build_network_planning_report_bundle(
            inputs.normalized,
            inputs.baseline,
            inputs.facility,
            inputs.comparison,
            country_code="ID",
        ).model_dump(mode="json"),
    ]
    bundles[0]["summary"]["unexpected"] = True
    bundles[1]["entities"]["demand_cities"][0]["unexpected"] = True
    with pytest.raises(ValidationError):
        NetworkComparisonMapBundle.model_validate(bundles[0])
    with pytest.raises(ValidationError):
        NetworkPlanningReportBundle.model_validate(bundles[1])


def test_delivery_bundles_are_deterministic_for_equivalent_input_order(
    sample2_delivery: Sample2Delivery,
) -> None:
    inputs = sample2_delivery
    reversed_normalized = inputs.normalized.model_copy(
        update={
            "demand_cities": list(reversed(inputs.normalized.demand_cities)),
            "warehouses": list(reversed(inputs.normalized.warehouses)),
        }
    )
    reversed_baseline = inputs.baseline.model_copy(
        update={
            "active_warehouse_ids": list(reversed(inputs.baseline.active_warehouse_ids)),
            "assignment": inputs.baseline.assignment.model_copy(
                update={"rows": list(reversed(inputs.baseline.assignment.rows))}
            ),
            "service": list(reversed(inputs.baseline.service)),
        }
    )
    reversed_facility = inputs.facility.model_copy(
        update={
            "active_warehouse_ids": list(reversed(inputs.facility.active_warehouse_ids)),
            "opened_candidate_ids": list(reversed(inputs.facility.opened_candidate_ids)),
            "assignment": inputs.facility.assignment.model_copy(
                update={"rows": list(reversed(inputs.facility.assignment.rows))}
            ),
            "service": list(reversed(inputs.facility.service)),
        }
    )
    reversed_comparison = inputs.comparison.model_copy(
        update={
            "requested_service_targets": list(
                reversed(inputs.comparison.requested_service_targets)
            ),
            "coverage": list(reversed(inputs.comparison.coverage)),
            "selected_warehouse_ids": list(reversed(inputs.comparison.selected_warehouse_ids)),
            "affected_city_ids": list(reversed(inputs.comparison.affected_city_ids)),
            "reassigned_city_ids": list(reversed(inputs.comparison.reassigned_city_ids)),
            "city_changes": list(reversed(inputs.comparison.city_changes)),
        }
    )

    expected_map = build_network_comparison_map_bundle(
        inputs.normalized,
        inputs.baseline,
        inputs.facility,
        inputs.comparison,
        country_code="ID",
    )
    reordered_map = build_network_comparison_map_bundle(
        reversed_normalized,
        reversed_baseline,
        reversed_facility,
        reversed_comparison,
        country_code="ID",
    )
    expected_report = build_network_planning_report_bundle(
        inputs.normalized,
        inputs.baseline,
        inputs.facility,
        inputs.comparison,
        country_code="ID",
    )
    reordered_report = build_network_planning_report_bundle(
        reversed_normalized,
        reversed_baseline,
        reversed_facility,
        reversed_comparison,
        country_code="ID",
    )

    assert reordered_map == expected_map
    assert reordered_report == expected_report
    assert render_network_planning_report_markdown(
        reordered_report
    ) == render_network_planning_report_markdown(expected_report)


def test_report_markdown_is_a_bounded_brief_not_a_json_assignment_dump(
    sample2_delivery: Sample2Delivery,
) -> None:
    inputs = sample2_delivery
    bundle = build_network_planning_report_bundle(
        inputs.normalized,
        inputs.baseline,
        inputs.facility,
        inputs.comparison,
        country_code="ID",
    )

    markdown = render_network_planning_report_markdown(bundle)

    assert markdown.startswith("# 仓网规划结果简报\n\n")
    assert NETWORK_PLANNING_MARKDOWN_MARKER in markdown
    assert "## 执行摘要" in markdown
    assert "## 仓库变动" in markdown
    assert "## 时效覆盖" in markdown
    assert "基线城市覆盖率" in markdown
    assert "方案城市覆盖率" in markdown
    assert "## 受影响的需求城市" in markdown
    assert "## 成本汇总" in markdown
    assert "结构化计算结果" in markdown
    assert '"schema_version"' not in markdown
    assert '"rows"' not in markdown
    assert len(markdown.encode("utf-8")) < 32 * 1024


def test_map_rejects_missing_coordinates_without_blocking_json_report(
    sample2_delivery: Sample2Delivery,
) -> None:
    inputs = sample2_delivery
    first = inputs.normalized.warehouses[0]
    normalized = inputs.normalized.model_copy(
        update={
            "warehouses": [
                first.model_copy(update={"longitude": None}),
                *inputs.normalized.warehouses[1:],
            ]
        }
    )

    with pytest.raises(
        ValueError,
        match=f"delivery_map_coordinates_required:warehouse:{first.warehouse_id}",
    ):
        build_network_comparison_map_bundle(
            normalized,
            inputs.baseline,
            inputs.facility,
            inputs.comparison,
            country_code="ID",
        )

    report = build_network_planning_report_bundle(
        normalized,
        inputs.baseline,
        inputs.facility,
        inputs.comparison,
        country_code="ID",
    )
    assert report.scope.warehouse_count == 23


@pytest.mark.parametrize(
    ("field", "error_code"),
    [
        ("unknown_assignment", "delivery_facility_assignment_warehouse_unknown"),
        ("inconsistent_active_delta", "delivery_comparison_selected_warehouse_ids_mismatch"),
        ("incomplete_baseline_active", "delivery_baseline_active_must_equal_existing"),
        ("missing_cost", "delivery_facility_cost_required"),
        ("missing_service", "delivery_facility_service_required"),
    ],
)
def test_delivery_rejects_inconsistent_typed_results(
    sample2_delivery: Sample2Delivery,
    field: str,
    error_code: str,
) -> None:
    inputs = sample2_delivery
    baseline = inputs.baseline
    facility = inputs.facility
    comparison = inputs.comparison
    if field == "unknown_assignment":
        assignment = facility.assignment.model_copy(
            update={
                "rows": [
                    facility.assignment.rows[0].model_copy(update={"warehouse_id": "not-real"}),
                    *facility.assignment.rows[1:],
                ]
            }
        )
        facility = facility.model_copy(update={"assignment": assignment})
    elif field == "inconsistent_active_delta":
        comparison = comparison.model_copy(update={"selected_warehouse_ids": []})
    elif field == "incomplete_baseline_active":
        baseline = baseline.model_copy(
            update={"active_warehouse_ids": baseline.active_warehouse_ids[1:]}
        )
    elif field == "missing_cost":
        facility = facility.model_copy(update={"cost": None})
    else:
        facility = facility.model_copy(update={"service": []})

    with pytest.raises(ValueError, match=error_code):
        build_network_planning_report_bundle(
            inputs.normalized,
            baseline,
            facility,
            comparison,
            country_code="ID",
        )


def test_country_is_explicit_and_not_indonesia_specific(
    sample2_delivery: Sample2Delivery,
) -> None:
    inputs = sample2_delivery
    report = build_network_planning_report_bundle(
        inputs.normalized,
        inputs.baseline,
        inputs.facility,
        inputs.comparison,
        country_code="MY",
    )

    assert report.country_code == "MY"


def _assert_no_external_delivery_identity(value: object) -> None:
    forbidden_keys = {
        "case_id",
        "artifact_id",
        "artifact_ref",
        "resource_id",
        "resource_ref",
        "resource_name",
        "source_hash",
        "content_sha256",
        "revision",
        "uri",
        "path",
    }
    if isinstance(value, dict):
        assert forbidden_keys.isdisjoint(value)
        assert not any(key.endswith("_ref") for key in value)
        for nested in value.values():
            _assert_no_external_delivery_identity(nested)
    elif isinstance(value, list):
        for nested in value:
            _assert_no_external_delivery_identity(nested)
    elif isinstance(value, str):
        assert not value.startswith("/")
