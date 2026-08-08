from __future__ import annotations

from pathlib import Path

from supply_chain_planner.case_repository import CaseRepository
from supply_chain_planner.case_types import FacetName, FacetState
from supply_chain_planner.matrix_service import CaseMatrixService
from supply_chain_planner.network_models import (
    DemandCityRecord,
    NormalizedInputBatch,
    WarehouseRecord,
)


def _ready_case(repository: CaseRepository, workspace: Path):
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
        current_assignments=[],
        route_quotes=[],
    )
    lease = repository.begin_operation(case.case_id, workspace, "normalize", {"v": 1})
    repository.commit_normalized_input(lease, workspace, batch)
    return case


def test_case_route_matrix_is_persisted_without_exposing_rows_in_status(
    tmp_path: Path,
) -> None:
    repository = CaseRepository(tmp_path / "cases.sqlite3")
    workspace = tmp_path / "workspace"
    workspace.mkdir()
    case = _ready_case(repository, workspace)

    matrix, result = CaseMatrixService(repository).build_haversine(
        case.case_id, workspace, 1.2, 40
    )

    assert len(matrix.rows) == 1
    assert result.components[0].row_count == 1
    status = repository.get_status(case.case_id, workspace)
    route = next(item for item in status.facets if item.name == FacetName.ROUTE_MATRIX)
    assert route.state == FacetState.READY
    assert route.row_count == 1


def test_cost_matrix_requires_explicit_rule_when_quotes_are_missing(tmp_path: Path) -> None:
    repository = CaseRepository(tmp_path / "cases.sqlite3")
    workspace = tmp_path / "workspace"
    workspace.mkdir()
    case = _ready_case(repository, workspace)
    service = CaseMatrixService(repository)
    service.build_haversine(case.case_id, workspace, 1.2, 40)

    incomplete, result = service.build_costs(
        case.case_id, workspace, None, "existing_only"
    )
    complete, committed = service.build_costs(
        case.case_id,
        workspace,
        {"currency": "IDR", "fixed_price": 10_000, "price_per_km": 2_000},
        "existing_only",
    )

    assert result is None
    assert incomplete.missing_routes == [("warehouse-a", "city-a", "last_mile")]
    assert committed is not None
    assert complete.missing_routes == []
    stored, _ = repository.load_cost_matrix(case.case_id, workspace)
    assert stored.rows[0].source == "calculated"
