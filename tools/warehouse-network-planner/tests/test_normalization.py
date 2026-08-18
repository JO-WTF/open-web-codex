from __future__ import annotations

from supply_chain_planner.data.mapping import SourceRole, TransformKind, TransformSpec
from supply_chain_planner.data.normalization import (
    ConfirmedFieldMapping,
    ConfirmedSourceRows,
    normalize_confirmed_rows,
)


def _mapping(target: str, kind: TransformKind) -> ConfirmedFieldMapping:
    return ConfirmedFieldMapping(
        target_field=target,
        source_field=target,
        transform=TransformSpec(kind=kind),
    )


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


def test_route_quote_preserves_complete_provided_route_fact() -> None:
    row = {
        "origin_id": "WH-1",
        "destination_id": "CITY-1",
        "destination_name": "City One",
        "layer": "last_mile",
        "distance_km": "12.5",
        "duration_hours": "0.5",
        "price_per_vehicle": "100",
        "currency": "IDR",
        "vehicle_capacity": "1",
        "method": "haversine",
    }
    decimal_fields = {
        "distance_km",
        "duration_hours",
        "price_per_vehicle",
        "vehicle_capacity",
    }
    id_fields = {"origin_id", "destination_id"}
    state, batch = normalize_confirmed_rows(
        [
            ConfirmedSourceRows(
                role=SourceRole.ROUTE_QUOTE,
                rows=[row],
                mappings=[
                    _mapping(
                        field,
                        TransformKind.PARSE_DECIMAL
                        if field in decimal_fields
                        else TransformKind.NORMALIZE_IDENTIFIER
                        if field in id_fields
                        else TransformKind.TRIM,
                    )
                    for field in row
                ],
            )
        ]
    )

    assert state == "needs_input"  # demand and warehouse inputs are intentionally absent
    assert len(batch.route_quotes) == 1
    assert len(batch.provided_route_facts) == 1
    fact = batch.provided_route_facts[0]
    assert fact.distance_km == 12.5
    assert fact.duration_hours == 0.5
    assert fact.source_method == "haversine"


def test_incomplete_provided_route_fact_rejects_the_entire_route_row() -> None:
    state, batch = normalize_confirmed_rows(
        [
            ConfirmedSourceRows(
                role=SourceRole.ROUTE_QUOTE,
                rows=[
                    {
                        "origin_id": "WH-1",
                        "destination_id": "CITY-1",
                        "layer": "last_mile",
                        "distance_km": "12.5",
                        "price_per_vehicle": "100",
                        "currency": "IDR",
                        "vehicle_capacity": "1",
                    }
                ],
                mappings=[
                    _mapping(
                        field,
                        TransformKind.PARSE_DECIMAL
                        if field in {"distance_km", "price_per_vehicle", "vehicle_capacity"}
                        else TransformKind.NORMALIZE_IDENTIFIER
                        if field in {"origin_id", "destination_id"}
                        else TransformKind.TRIM,
                    )
                    for field in (
                        "origin_id",
                        "destination_id",
                        "layer",
                        "distance_km",
                        "price_per_vehicle",
                        "currency",
                        "vehicle_capacity",
                    )
                ],
            )
        ]
    )

    assert state == "needs_input"
    assert batch.route_quotes == []
    assert batch.provided_route_facts == []
    assert "provided_route_fact_incomplete" in {issue.code for issue in batch.issues}
