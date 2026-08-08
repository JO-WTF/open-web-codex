"""Administrative catalog and point-validation helpers."""

from __future__ import annotations

from typing import Any

from .geo import ProvinceBoundary


def load_administrative_catalog(
    country_code: str,
    admin_level: str,
    payload: dict[str, Any],
) -> dict[str, Any]:
    requested_country = {"IDN": "ID"}.get(country_code.upper(), country_code.upper())
    payload_country = str(payload.get("country_code", country_code)).upper()
    if requested_country != payload_country:
        raise ValueError("administrative_catalog_country_mismatch")
    if admin_level != payload.get("admin_level", admin_level):
        raise ValueError("administrative_catalog_level_mismatch")
    rows = payload.get("rows")
    if not isinstance(rows, list):
        raise ValueError("administrative_catalog_rows_missing")
    return {
        "schema": "administrative_catalog.v1",
        "country_code": country_code.upper(),
        "admin_level": admin_level,
        "rows": rows,
    }


def resolve_place_names(rows: list[dict[str, Any]], catalog: dict[str, Any]) -> dict[str, Any]:
    by_id = {str(row.get("city_id")): row for row in catalog.get("rows", [])}
    exact = []
    ambiguous = []
    missing = []
    resolved = []
    for row in rows:
        city_id = str(row.get("city_id", "")).strip()
        name = str(row.get("city_name", row.get("name", ""))).strip()
        match = by_id.get(city_id) if city_id else None
        if match is not None:
            exact.append(city_id)
            resolved.append({**row, **match, "match": "exact"})
            continue
        candidates = [
            item
            for item in catalog.get("rows", [])
            if str(item.get("city_name", "")).casefold() == name.casefold()
        ]
        if len(candidates) == 1:
            resolved.append({**row, **candidates[0], "match": "normalized"})
        elif len(candidates) > 1:
            ambiguous.append(name)
        else:
            missing.append(name or city_id)
    return {
        "schema": "place_resolution.v1",
        "resolved": resolved,
        "exact": exact,
        "ambiguous": ambiguous,
        "missing": missing,
        "ready": not ambiguous and not missing,
    }


def build_administrative_candidates(catalog: dict[str, Any], level: str) -> dict[str, Any]:
    if level not in {"province", "city"}:
        raise ValueError("candidate_level_must_be_province_or_city")
    return {
        "schema": "warehouse_candidates.v1",
        "level": level,
        "candidates": [
            {
                "warehouse_id": f"candidate-{row.get('city_id')}",
                "warehouse_name": row.get("city_name", row.get("name")),
                "city_id": row.get("city_id"),
                "city_name": row.get("city_name", row.get("name")),
                "province_id": row.get("province_id"),
                "province_name": row.get("province_name"),
                "longitude": row.get("longitude"),
                "latitude": row.get("latitude"),
                "source": "administrative_catalog",
            }
            for row in catalog.get("rows", [])
            if level == "city" or row.get("is_province_capital") is True
        ],
    }


def validate_points_within_boundaries(
    points: list[dict[str, Any]],
    boundary_payload: dict[str, Any],
) -> dict[str, Any]:
    features = boundary_payload.get("features") or []
    boundaries: list[ProvinceBoundary] = []
    for feature in features:
        properties = feature.get("properties") or {}
        code_field = str(boundary_payload.get("code_field", "GID_1"))
        name_field = str(boundary_payload.get("name_field", "NAME_1"))
        if code_field not in properties or name_field not in properties:
            continue
        boundaries.append(
            ProvinceBoundary.from_feature(feature, code_field=code_field, name_field=name_field)
        )
    invalid = []
    checked = []
    for point in points:
        coordinate = (float(point["longitude"]), float(point["latitude"]))
        matches = [boundary.code for boundary in boundaries if boundary.contains(coordinate)]
        checked.append({"id": point.get("id", point.get("city_id")), "province_codes": matches})
        if len(matches) != 1:
            invalid.append(checked[-1])
    return {
        "schema": "boundary_validation.v1",
        "valid": not invalid,
        "checked_count": len(checked),
        "invalid": invalid,
    }
