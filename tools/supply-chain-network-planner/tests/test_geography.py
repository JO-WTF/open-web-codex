from __future__ import annotations

from supply_chain_planner.geography import (
    build_administrative_candidates,
    resolve_place_names,
    validate_points_within_boundaries,
)


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
