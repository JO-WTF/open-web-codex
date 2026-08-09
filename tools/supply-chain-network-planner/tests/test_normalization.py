from __future__ import annotations

from pathlib import Path

from supply_chain_planner.case_repository import CaseRepository
from supply_chain_planner.case_types import FacetName, FacetState
from supply_chain_planner.mapping import SourceRole, TransformKind, TransformSpec
from supply_chain_planner.mapping_service import CaseMappingService
from supply_chain_planner.network_data import SourceInventoryService
from supply_chain_planner.normalization import (
    ConfirmedFieldMapping,
    ConfirmedSourceRows,
    NormalizationService,
    normalize_confirmed_rows,
)


def _mapping(target: str, kind: TransformKind) -> ConfirmedFieldMapping:
    return ConfirmedFieldMapping(
        target_field=target,
        source_field=target,
        transform=TransformSpec(kind=kind),
    )


def test_quote_rows_never_create_current_assignments(tmp_path: Path) -> None:
    workspace = tmp_path / "workspace"
    workspace.mkdir()
    (workspace / "demand.csv").write_text(
        "city_id,city_name,demand_quantity,longitude,latitude\n"
        "ID-1,Jakarta,10,106.8,-6.2\n",
        encoding="utf-8",
    )
    (workspace / "warehouses.csv").write_text(
        "warehouse_id,warehouse_name,warehouse_type,city_id,city_name,is_existing,"
        "is_fixed,longitude,latitude\n"
        "WH-1,Center,center,ID-1,Jakarta,true,true,106.8,-6.2\n",
        encoding="utf-8",
    )
    (workspace / "quotes.csv").write_text(
        "origin_id,destination_id,layer,price_per_vehicle,currency,vehicle_capacity\n"
        "WH-1,ID-1,last_mile,10000,IDR,1\n",
        encoding="utf-8",
    )
    repository = CaseRepository(tmp_path / "cases.sqlite3")
    case = repository.create_case(workspace, "ID", "Analyze")
    SourceInventoryService(repository).refresh(case.case_id, workspace)
    proposal, _ = CaseMappingService(repository).propose(case.case_id, workspace)
    selected = [
        candidate.candidate_id
        for role in proposal.proposals
        if role.role
        in {SourceRole.DEMAND, SourceRole.EXISTING_WAREHOUSE, SourceRole.ROUTE_QUOTE}
        for candidate in role.field_candidates
    ]
    CaseMappingService(repository).apply(case.case_id, workspace, selected)

    batch, result = NormalizationService(repository).normalize(case.case_id, workspace)

    assert result is not None
    assert len(batch.route_quotes) == 1
    assert batch.current_assignments == []
    status = repository.get_status(case.case_id, workspace)
    assignment = next(item for item in status.facets if item.name == FacetName.CURRENT_ASSIGNMENT)
    assert assignment.state == FacetState.MISSING


def test_pure_normalization_accepts_verified_rows_without_case_or_io() -> None:
    demand = ConfirmedSourceRows(
        role=SourceRole.DEMAND,
        rows=[
            {
                "city_id": "CITY-1",
                "city_name": "City One",
                "demand_quantity": "10",
                "longitude": "100.1",
                "latitude": "1.2",
            }
        ],
        mappings=[
            _mapping("city_id", TransformKind.NORMALIZE_IDENTIFIER),
            _mapping("city_name", TransformKind.TRIM),
            _mapping("demand_quantity", TransformKind.PARSE_INTEGER),
            _mapping("longitude", TransformKind.PARSE_DECIMAL),
            _mapping("latitude", TransformKind.PARSE_DECIMAL),
        ],
    )
    warehouse = ConfirmedSourceRows(
        role=SourceRole.EXISTING_WAREHOUSE,
        rows=[
            {
                "warehouse_id": "WH-1",
                "warehouse_name": "Warehouse One",
                "warehouse_type": "center",
                "city_id": "CITY-1",
                "city_name": "City One",
                "longitude": "100.1",
                "latitude": "1.2",
            }
        ],
        mappings=[
            _mapping("warehouse_id", TransformKind.NORMALIZE_IDENTIFIER),
            _mapping("warehouse_name", TransformKind.TRIM),
            _mapping("warehouse_type", TransformKind.NORMALIZE_WAREHOUSE_TYPE),
            _mapping("city_id", TransformKind.NORMALIZE_IDENTIFIER),
            _mapping("city_name", TransformKind.TRIM),
            _mapping("longitude", TransformKind.PARSE_DECIMAL),
            _mapping("latitude", TransformKind.PARSE_DECIMAL),
        ],
    )

    state, batch = normalize_confirmed_rows([warehouse, demand])

    assert state == "ready"
    assert [item.city_id for item in batch.demand_cities] == ["CITY-1"]
    assert [item.warehouse_id for item in batch.warehouses] == ["WH-1"]
    assert batch.warehouses[0].is_existing is True
    assert batch.warehouses[0].is_fixed is None
    assert set(ConfirmedSourceRows.__dataclass_fields__) == {"role", "rows", "mappings"}

    without_coordinates = [
        ConfirmedSourceRows(
            role=source.role,
            rows=[
                {
                    key: value
                    for key, value in source.rows[0].items()
                    if key not in {"longitude", "latitude"}
                }
            ],
            mappings=[
                mapping
                for mapping in source.mappings
                if mapping.target_field not in {"longitude", "latitude"}
            ],
        )
        for source in (warehouse, demand)
    ]
    geography_state, _ = normalize_confirmed_rows(without_coordinates)
    assert geography_state == "needs_geography"


def test_pure_normalization_returns_typed_missing_and_unknown_role_issues() -> None:
    state, batch = normalize_confirmed_rows(
        [
            ConfirmedSourceRows(role="unknown", rows=[{}], mappings=[]),
            ConfirmedSourceRows(
                role=SourceRole.ADMINISTRATIVE_CATALOG,
                rows=[{"city_id": "CITY-1"}],
                mappings=[],
            ),
        ]
    )

    assert state == "needs_input"
    assert {item.code for item in batch.issues} >= {
        "normalization_role_unknown",
        "normalization_role_unsupported",
        "demand_missing",
        "warehouse_missing",
    }


def test_route_quote_missing_business_fields_is_typed_issue() -> None:
    base_row = {
        "origin_id": "WH-1",
        "destination_id": "CITY-1",
        "price_per_vehicle": "100",
        "layer": "last_mile",
        "currency": "USD",
        "vehicle_capacity": "20",
    }
    kinds = {
        "origin_id": TransformKind.NORMALIZE_IDENTIFIER,
        "destination_id": TransformKind.NORMALIZE_IDENTIFIER,
        "price_per_vehicle": TransformKind.PARSE_DECIMAL,
        "layer": TransformKind.TRIM,
        "currency": TransformKind.TRIM,
        "vehicle_capacity": TransformKind.PARSE_DECIMAL,
    }
    for missing in ("layer", "currency", "vehicle_capacity"):
        row = {key: value for key, value in base_row.items() if key != missing}
        state, batch = normalize_confirmed_rows(
            [
                ConfirmedSourceRows(
                    role=SourceRole.ROUTE_QUOTE,
                    rows=[row],
                    mappings=[_mapping(field, kind) for field, kind in kinds.items()],
                )
            ]
        )

        assert state == "needs_input"
        assert f"route_quote_{missing}_missing" in {item.code for item in batch.issues}
