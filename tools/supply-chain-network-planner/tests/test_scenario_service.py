from __future__ import annotations

from pathlib import Path

import pytest

from supply_chain_planner.analysis_service import NetworkAnalysisService
from supply_chain_planner.case_repository import CaseRepository
from supply_chain_planner.map_service import NetworkMapService
from supply_chain_planner.matrix_service import CaseMatrixService
from supply_chain_planner.network_models import (
    DemandCityRecord,
    NormalizedInputBatch,
    WarehouseRecord,
)
from supply_chain_planner.optimization_models import ScenarioSpec, WarehouseRelocation
from supply_chain_planner.report_service import NetworkReportService
from supply_chain_planner.resource_store import ResourceStore
from supply_chain_planner.scenario_service import (
    FacilityLocationService,
    NetworkScenarioService,
)


def _ready_case(repository: CaseRepository, workspace: Path):
    case = repository.create_case(workspace, "ID", "测试场景与选址")
    batch = NormalizedInputBatch(
        demand_cities=[
            DemandCityRecord(
                city_id="city-a",
                city_name="City A",
                demand_quantity=10,
                longitude=106.8,
                latitude=-6.2,
            ),
            DemandCityRecord(
                city_id="city-b",
                city_name="City B",
                demand_quantity=20,
                longitude=110.4,
                latitude=-7.8,
            ),
        ],
        warehouses=[
            WarehouseRecord(
                warehouse_id="existing-a",
                warehouse_name="Existing A",
                warehouse_type="center",
                city_id="city-a",
                city_name="City A",
                longitude=106.8,
                latitude=-6.2,
                is_existing=True,
                is_fixed=True,
            ),
            WarehouseRecord(
                warehouse_id="candidate-b",
                warehouse_name="Candidate B",
                warehouse_type="center",
                city_id="city-b",
                city_name="City B",
                longitude=110.4,
                latitude=-7.8,
                is_existing=False,
                is_fixed=False,
            ),
        ],
        current_assignments=[],
        route_quotes=[],
    )
    lease = repository.begin_operation(case.case_id, workspace, "normalize", {"v": 1})
    repository.commit_normalized_input(lease, workspace, batch)
    matrices = CaseMatrixService(repository)
    matrices.build_haversine(case.case_id, workspace, 1.2, 40)
    matrices.build_costs(
        case.case_id,
        workspace,
        {"currency": "IDR", "fixed_price": 10_000, "price_per_km": 2_000},
        "all_warehouses",
    )
    return case


def test_relocation_requires_explicit_old_and_new_warehouse_ids(tmp_path: Path) -> None:
    repository = CaseRepository(tmp_path / "cases.sqlite3")
    workspace = tmp_path / "workspace"
    workspace.mkdir()
    case = _ready_case(repository, workspace)

    scenario, _ = NetworkScenarioService(repository).evaluate(
        case.case_id,
        workspace,
        ScenarioSpec(
            relocations=[
                WarehouseRelocation(
                    remove_warehouse_id="existing-a",
                    add_warehouse_id="candidate-b",
                )
            ],
            service_targets=[6, 12],
        ),
    )

    assert scenario.warehouse_changes == {
        "added": ["candidate-b"],
        "removed": ["existing-a"],
    }
    assert {row.warehouse_id for row in scenario.assignment.rows} == {"candidate-b"}
    stored, _ = repository.load_scenario(case.case_id, workspace)
    assert stored == scenario


def test_scenario_does_not_enable_candidates_unless_requested(tmp_path: Path) -> None:
    repository = CaseRepository(tmp_path / "cases.sqlite3")
    workspace = tmp_path / "workspace"
    workspace.mkdir()
    case = _ready_case(repository, workspace)

    scenario, _ = NetworkScenarioService(repository).evaluate(
        case.case_id,
        workspace,
        ScenarioSpec(service_targets=[6]),
    )

    assert {row.warehouse_id for row in scenario.assignment.rows} == {"existing-a"}


def test_p_median_persists_solution_and_reuses_identical_operation(tmp_path: Path) -> None:
    pytest.importorskip("ortools")
    repository = CaseRepository(tmp_path / "cases.sqlite3")
    workspace = tmp_path / "workspace"
    workspace.mkdir()
    case = _ready_case(repository, workspace)
    service = FacilityLocationService(repository)

    solution, _ = service.solve(
        case.case_id,
        workspace,
        number_to_open=1,
        fixed_existing_ids=set(),
        optional_existing_ids=set(),
        time_limit_seconds=5,
    )
    reused, operation = service.solve(
        case.case_id,
        workspace,
        number_to_open=1,
        fixed_existing_ids=set(),
        optional_existing_ids=set(),
        time_limit_seconds=5,
    )

    assert solution.status == "optimal"
    assert "candidate-b" in solution.selected_warehouse_ids
    assert reused == solution
    assert operation.operation.reused is True


def test_report_publication_exposes_summary_without_assignment_rows(tmp_path: Path) -> None:
    repository = CaseRepository(tmp_path / "cases.sqlite3")
    workspace = tmp_path / "workspace"
    workspace.mkdir()
    case = _ready_case(repository, workspace)
    NetworkScenarioService(repository).evaluate(
        case.case_id,
        workspace,
        ScenarioSpec(add_warehouse_ids=["candidate-b"], service_targets=[6]),
    )
    store = ResourceStore(tmp_path / "resources")

    published, summary, _ = NetworkReportService(repository, store).publish(
        case.case_id, workspace, "scenario"
    )
    content = store.load_uri(published.uri)

    assert content["schema_version"] == "network_planning_report.v1"
    assert content["source"] == "scenario"
    assert "assignment" not in content
    assert summary["service_metric_count"] == 1
    assert published.size < 16 * 1024


def test_comparison_map_is_published_without_returning_geojson_rows(tmp_path: Path) -> None:
    repository = CaseRepository(tmp_path / "cases.sqlite3")
    workspace = tmp_path / "workspace"
    workspace.mkdir()
    case = _ready_case(repository, workspace)
    NetworkAnalysisService(repository).evaluate_baseline(
        case.case_id,
        workspace,
        "min_cost",
        [6],
        "optimized_existing_footprint",
        True,
    )
    NetworkScenarioService(repository).evaluate(
        case.case_id,
        workspace,
        ScenarioSpec(add_warehouse_ids=["candidate-b"], service_targets=[6]),
    )
    store = ResourceStore(tmp_path / "resources")

    published, summary, _ = NetworkMapService(repository, store).publish_comparison(
        case.case_id, workspace, "scenario"
    )
    content = store.load_uri(published.uri)

    assert content["type"] == "FeatureCollection"
    assert content["schema_version"] == "network_comparison_map.v1"
    assert len(content["features"]) == summary["feature_count"]
    assert summary["candidate_active_warehouse_count"] >= 1
