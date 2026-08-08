from __future__ import annotations

from pathlib import Path

from supply_chain_planner.analysis_service import NetworkAnalysisService
from supply_chain_planner.case_repository import CaseRepository
from supply_chain_planner.matrix_service import CaseMatrixService
from supply_chain_planner.network_models import (
    CurrentAssignmentRecord,
    DemandCityRecord,
    NormalizedInputBatch,
    WarehouseRecord,
)


def _prepare_case(
    repository: CaseRepository,
    workspace: Path,
    *,
    with_current_assignment: bool,
):
    case = repository.create_case(workspace, "ID", "分析网络")
    batch = NormalizedInputBatch(
        demand_cities=[
            DemandCityRecord(
                city_id="city-a",
                city_name="City A",
                demand_quantity=10,
                longitude=106.8,
                latitude=-6.2,
            )
        ],
        warehouses=[
            WarehouseRecord(
                warehouse_id="warehouse-a",
                warehouse_name="Warehouse A",
                warehouse_type="center",
                city_id="city-a",
                city_name="City A",
                longitude=106.8,
                latitude=-6.2,
                is_existing=True,
                is_fixed=True,
            )
        ],
        current_assignments=(
            [
                CurrentAssignmentRecord(
                    demand_city_id="city-a",
                    serving_warehouse_id="warehouse-a",
                )
            ]
            if with_current_assignment
            else []
        ),
        route_quotes=[],
    )
    lease = repository.begin_operation(case.case_id, workspace, "normalize", {"v": 1})
    repository.commit_normalized_input(lease, workspace, batch)
    CaseMatrixService(repository).build_haversine(case.case_id, workspace, 1.2, 40)
    return case


def test_missing_current_assignment_produces_labeled_optimized_baseline(
    tmp_path: Path,
) -> None:
    repository = CaseRepository(tmp_path / "cases.sqlite3")
    workspace = tmp_path / "workspace"
    workspace.mkdir()
    case = _prepare_case(repository, workspace, with_current_assignment=False)

    baseline, result = NetworkAnalysisService(repository).evaluate_baseline(
        case.case_id,
        workspace,
        "min_time",
        [6, 12],
        "actual_if_available",
        False,
    )

    assert baseline.label == "optimized_existing_footprint"
    assert baseline.notice_code == "current_assignment_missing"
    assert [metric.coverage_rate for metric in baseline.service] == [1, 1]
    assert result.components[0].row_count == 1
    stored, _ = repository.load_baseline(case.case_id, workspace)
    assert stored == baseline


def test_current_assignment_remains_actual_and_is_not_reoptimized(tmp_path: Path) -> None:
    repository = CaseRepository(tmp_path / "cases.sqlite3")
    workspace = tmp_path / "workspace"
    workspace.mkdir()
    case = _prepare_case(repository, workspace, with_current_assignment=True)

    baseline, _ = NetworkAnalysisService(repository).evaluate_baseline(
        case.case_id,
        workspace,
        "min_time",
        [6],
        "actual_if_available",
        False,
    )

    assert baseline.label == "actual_current"
    assert baseline.notice_code is None
