"""Administrative catalog and point-validation helpers."""

from __future__ import annotations

from collections.abc import Mapping, Sequence
from typing import Any, Literal

from supply_chain_planner.network.geo import ProvinceBoundary
from supply_chain_planner.network.models import DataQualityIssue, DemandCityRecord, WarehouseRecord


def load_administrative_catalog(
    country_code: str,
    admin_level: str,
    payload: dict[str, Any],
) -> dict[str, Any]:
    requested_country = country_code.upper()
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


def enrich_network_geography(
    demand_cities: Sequence[DemandCityRecord],
    warehouses: Sequence[WarehouseRecord],
    catalog: dict[str, Any],
    *,
    overrides: Mapping[tuple[Literal["demand", "warehouse"], str], str] | None = None,
    candidate_level: Literal["province", "city"] | None = None,
) -> tuple[
    list[DemandCityRecord],
    list[WarehouseRecord],
    list[dict[str, Any]],
    list[DataQualityIssue],
]:
    """Enrich typed records from one already-validated country catalog.

    The adapter owns catalog availability and country selection.  Overrides
    name an exact catalog city ID; an unknown override never falls back to the
    record's original city name.
    """

    catalog_rows = catalog.get("rows")
    if not isinstance(catalog_rows, list):
        raise ValueError("administrative_catalog_rows_missing")
    selected_overrides = overrides or {}
    issue_counts: dict[tuple[str, str], tuple[int, str, str]] = {}
    enriched_demands = [
        _enrich_demand_city(
            record,
            catalog,
            selected_overrides.get(("demand", record.city_id)),
            issue_counts,
        )
        for record in demand_cities
    ]
    enriched_warehouses = [
        _enrich_warehouse(
            record,
            catalog,
            selected_overrides.get(("warehouse", record.warehouse_id)),
            issue_counts,
        )
        for record in warehouses
    ]
    candidates = (
        build_administrative_candidates(catalog, candidate_level)["candidates"]
        if candidate_level is not None
        else []
    )
    return enriched_demands, enriched_warehouses, candidates, _materialize_issues(issue_counts)


def _enrich_demand_city(
    record: DemandCityRecord,
    catalog: dict[str, Any],
    override_city_id: str | None,
    issue_counts: dict[tuple[str, str], tuple[int, str, str]],
) -> DemandCityRecord:
    match = _resolve_record_place(
        record.city_id,
        record.city_name,
        override_city_id,
        catalog,
        "demand",
        record.city_id,
        issue_counts,
    )
    if match is None:
        return record
    return DemandCityRecord.model_validate(
        {
            **record.model_dump(),
            "city_id": match.get("city_id"),
            "city_name": match.get("city_name", match.get("name")),
            "province_id": match.get("province_id"),
            "province_name": match.get("province_name"),
            "longitude": match.get("longitude"),
            "latitude": match.get("latitude"),
        }
    )


def _enrich_warehouse(
    record: WarehouseRecord,
    catalog: dict[str, Any],
    override_city_id: str | None,
    issue_counts: dict[tuple[str, str], tuple[int, str, str]],
) -> WarehouseRecord:
    match = _resolve_record_place(
        record.city_id,
        record.city_name,
        override_city_id,
        catalog,
        "existing_warehouse" if record.is_existing else "candidate_warehouse",
        record.warehouse_id,
        issue_counts,
    )
    if match is None:
        return record
    return WarehouseRecord.model_validate(
        {
            **record.model_dump(),
            "city_id": match.get("city_id"),
            "city_name": match.get("city_name", match.get("name")),
            "province_id": match.get("province_id"),
            "province_name": match.get("province_name"),
            "longitude": match.get("longitude"),
            "latitude": match.get("latitude"),
        }
    )


def _resolve_record_place(
    city_id: str,
    city_name: str,
    override_city_id: str | None,
    catalog: dict[str, Any],
    entity: Literal["demand", "existing_warehouse", "candidate_warehouse"],
    entity_id: str,
    issue_counts: dict[tuple[str, str], tuple[int, str, str]],
) -> dict[str, Any] | None:
    if override_city_id is not None:
        exact = next(
            (
                row
                for row in catalog.get("rows", [])
                if str(row.get("city_id", "")).strip() == override_city_id
            ),
            None,
        )
        if exact is None:
            _record_issue(
                issue_counts,
                "geography_override_city_unknown",
                "行政区目录中的显式 override 城市不存在。",
                "administrative_catalog:override",
            )
        return exact
    if city_id:
        exact = next(
            (
                row
                for row in catalog.get("rows", [])
                if str(row.get("city_id", "")).strip() == city_id
            ),
            None,
        )
        if exact is None:
            _record_issue(
                issue_counts,
                "geography_city_id_unknown",
                "记录中的城市标识不在当前行政区目录中。",
                f"{entity}:{entity_id}",
            )
        return exact
    resolution = resolve_place_names(
        [{"city_id": city_id, "city_name": city_name}],
        catalog,
    )
    if resolution["ready"] and len(resolution["resolved"]) == 1:
        return resolution["resolved"][0]
    code = "geography_city_ambiguous" if resolution["ambiguous"] else "geography_city_missing"
    _record_issue(
        issue_counts,
        code,
        "城市无法唯一匹配当前行政区目录。",
        f"{entity}:{entity_id}",
    )
    return None


def _record_issue(
    issue_counts: dict[tuple[str, str], tuple[int, str, str]],
    code: str,
    message: str,
    field_name: str,
) -> None:
    entity_role = field_name.split(":", 1)[0]
    key = (code, entity_role)
    prior = issue_counts.get(key)
    if prior is None:
        issue_counts[key] = (1, message, field_name)
    else:
        issue_counts[key] = (prior[0] + 1, prior[1], prior[2])


def _materialize_issues(
    issue_counts: dict[tuple[str, str], tuple[int, str, str]]
) -> list[DataQualityIssue]:
    issues: list[DataQualityIssue] = []
    for (code, entity_role), (count, message, first_field) in issue_counts.items():
        field_name = first_field
        if count > 1 and entity_role in {"demand", "existing_warehouse", "candidate_warehouse"}:
            field_name = f"{entity_role}:multiple"
        issues.append(
            DataQualityIssue(
                code=code,
                severity="error",
                business_message=f"{message} 问题记录数：{count}。" if count > 1 else message,
                field_name=field_name,
            )
        )
    return issues


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
