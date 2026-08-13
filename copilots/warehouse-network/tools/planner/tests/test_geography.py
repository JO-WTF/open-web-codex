from __future__ import annotations

from decimal import Decimal

from supply_chain_planner.data.geography import (
    build_administrative_candidates,
    enrich_network_geography,
    resolve_place_names,
    validate_points_within_boundaries,
)
from supply_chain_planner.network.models import DemandCityRecord, WarehouseRecord


def _boundary_payload() -> dict:
    return {
        "code_field": "GID_1",
        "name_field": "NAME_1",
        "features": [
            {
                "type": "Feature",
                "properties": {"GID_1": "P-1", "NAME_1": "Province 1"},
                "geometry": {
                    "type": "Polygon",
                    "coordinates": [[[0, 0], [2, 0], [2, 2], [0, 2], [0, 0]]],
                },
            }
        ],
    }


def test_place_resolution_does_not_guess_ambiguous_names() -> None:
    catalog = {
        "rows": [
            {"city_id": "c-1", "city_name": "Springfield", "province_id": "p-1"},
            {"city_id": "c-2", "city_name": "Springfield", "province_id": "p-2"},
        ]
    }
    result = resolve_place_names([{"city_name": "Springfield"}], catalog)
    assert result["ready"] is False
    assert result["ambiguous"] == ["Springfield"]


def test_boundary_validation_requires_exactly_one_polygon() -> None:
    result = validate_points_within_boundaries(
        [
            {"id": "inside", "longitude": 1, "latitude": 1},
            {"id": "outside", "longitude": 3, "latitude": 3},
        ],
        _boundary_payload(),
    )
    assert result["valid"] is False
    assert result["checked_count"] == 2
    assert [item["id"] for item in result["invalid"]] == ["outside"]


def test_city_candidates_are_built_from_catalog_rows() -> None:
    result = build_administrative_candidates(
        {
            "rows": [
                {
                    "city_id": "c-1",
                    "city_name": "City 1",
                    "province_id": "p-1",
                    "province_name": "Province 1",
                    "longitude": 1,
                    "latitude": 1,
                }
            ]
        },
        "city",
    )
    assert result["candidates"][0]["warehouse_id"] == "candidate-c-1"


def _demand(city_id: str, city_name: str) -> DemandCityRecord:
    return DemandCityRecord(
        city_id=city_id,
        city_name=city_name,
        demand_quantity=Decimal("10"),
    )


def _warehouse(city_id: str, city_name: str) -> WarehouseRecord:
    return WarehouseRecord(
        warehouse_id="wh-1",
        warehouse_name="Warehouse One",
        warehouse_type="center",
        city_id=city_id,
        city_name=city_name,
        is_existing=True,
        is_fixed=True,
    )


def test_unknown_exact_override_never_falls_back_to_original_name() -> None:
    catalog = {
        "rows": [
            {
                "city_id": "real",
                "city_name": "Jakarta",
                "province_id": "p-1",
                "province_name": "Province One",
                "longitude": 106.8,
                "latitude": -6.2,
            }
        ]
    }
    demand = _demand("local", "Jakarta")

    demands, _, _, issues = enrich_network_geography(
        [demand],
        [],
        catalog,
        overrides={("demand", "local"): "not-real"},
    )

    assert [item.code for item in issues] == ["geography_override_city_unknown"]
    assert demands == [demand]
    assert demands[0].city_id == "local"


def test_country_neutral_catalog_enriches_records_and_candidates() -> None:
    catalog = {
        "country_code": "TH",
        "rows": [
            {
                "city_id": "TH-CITY-001",
                "city_name": "Bangkok",
                "province_id": "TH-PROV-10",
                "province_name": "Bangkok",
                "longitude": 100.5018,
                "latitude": 13.7563,
                "is_province_capital": True,
            }
        ],
    }

    demands, warehouses, candidates, issues = enrich_network_geography(
        [_demand("TH-CITY-001", "Bangkok")],
        [_warehouse("TH-CITY-001", "Bangkok")],
        catalog,
        candidate_level="province",
    )

    assert issues == []
    assert demands[0].province_id == "TH-PROV-10"
    assert warehouses[0].province_name == "Bangkok"
    assert warehouses[0].model_dump()["province_id"] == "TH-PROV-10"
    assert candidates[0]["city_id"] == "TH-CITY-001"


def test_unknown_record_city_id_does_not_fall_back_to_same_name() -> None:
    catalog = {
        "rows": [
            {"city_id": "c-1", "city_name": "Springfield", "province_id": "p-1"},
            {"city_id": "c-2", "city_name": "Springfield", "province_id": "p-2"},
        ]
    }

    demands, _, _, issues = enrich_network_geography(
        [_demand("local", "Springfield")],
        [],
        catalog,
    )

    assert [item.code for item in issues] == ["geography_city_id_unknown"]
    assert demands[0].city_id == "local"
