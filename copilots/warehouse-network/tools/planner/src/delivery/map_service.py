"""Deterministic warehouse-network GeoJSON and data bundles."""

from __future__ import annotations

from collections.abc import Mapping
from decimal import Decimal
from typing import Literal

from pydantic import Field
from supply_chain_planner.delivery.models import (
    DeliveryModel,
    validate_baseline_delivery_inputs,
    validate_delivery_inputs,
)
from supply_chain_planner.network.models import (
    DemandCityRecord,
    NormalizedInputBatch,
    WarehouseRecord,
)
from supply_chain_planner.network.optimization_models import (
    AssignmentComparison,
    AssignmentResult,
    AssignmentRow,
    BaselineResult,
    CoverageComparison,
    PMedianSolution,
)


class PointGeometry(DeliveryModel):
    type: Literal["Point"] = "Point"
    coordinates: tuple[float, float]


class LineStringGeometry(DeliveryModel):
    type: Literal["LineString"] = "LineString"
    coordinates: tuple[tuple[float, float], tuple[float, float]]


class WarehouseMapProperties(DeliveryModel):
    kind: Literal["warehouse"] = "warehouse"
    warehouse_id: str
    warehouse_name: str
    warehouse_type: Literal["center", "cross_docking"]
    city_id: str
    city_name: str
    province_id: str | None
    province_name: str | None
    is_existing: bool
    baseline_active: bool
    facility_active: bool
    opened_candidate: bool
    closed_existing: bool


class DemandMapProperties(DeliveryModel):
    kind: Literal["demand"] = "demand"
    city_id: str
    city_name: str
    province_id: str | None
    province_name: str | None
    # GeoJSON is a presentation boundary. Emit a JSON number here rather than
    # Pydantic's default Decimal string so Mapbox numeric expressions remain
    # valid while the planning domain continues to own Decimal arithmetic.
    demand_quantity: float = Field(ge=0)
    assigned_warehouse_id: str | None = None
    distance_km: float | None = Field(default=None, ge=0)
    duration_hours: float | None = Field(default=None, ge=0)
    unit_cost: float | None = Field(default=None, ge=0)
    baseline_warehouse_id: str | None = None
    baseline_distance_km: float | None = Field(default=None, ge=0)
    baseline_duration_hours: float | None = Field(default=None, ge=0)
    baseline_unit_cost: float | None = Field(default=None, ge=0)
    facility_warehouse_id: str | None = None
    facility_distance_km: float | None = Field(default=None, ge=0)
    facility_duration_hours: float | None = Field(default=None, ge=0)
    facility_unit_cost: float | None = Field(default=None, ge=0)


class AssignmentMapProperties(DeliveryModel):
    kind: Literal["last_mile_assignment"] = "last_mile_assignment"
    scenario: Literal["baseline", "scenario", "facility"]
    result_label: str
    warehouse_id: str
    demand_city_id: str
    demand_quantity: float = Field(ge=0)
    distance_km: float | None
    duration_hours: float | None
    unit_cost: float | None


class LinehaulMapProperties(DeliveryModel):
    kind: Literal["linehaul_connection"] = "linehaul_connection"
    scenario: Literal["baseline", "scenario", "facility"]
    upstream_center_id: str
    crossdock_warehouse_id: str
    assigned_demand: float = Field(ge=0)


MapProperties = (
    WarehouseMapProperties | DemandMapProperties | AssignmentMapProperties | LinehaulMapProperties
)


class NetworkMapFeature(DeliveryModel):
    type: Literal["Feature"] = "Feature"
    id: str
    geometry: PointGeometry | LineStringGeometry
    properties: MapProperties


class NetworkMapFeatureCollection(DeliveryModel):
    type: Literal["FeatureCollection"] = "FeatureCollection"
    features: list[NetworkMapFeature]


class NetworkDistributionGeoJson(DeliveryModel):
    """Point-only GeoJSON for interactive current-network map cards."""

    schema_version: Literal["network_distribution_geojson.v1"] = "network_distribution_geojson.v1"
    type: Literal["FeatureCollection"] = "FeatureCollection"
    features: list[NetworkMapFeature]


class NetworkComparisonGeoJson(DeliveryModel):
    """Comparison GeoJSON published specifically for an inline map card."""

    schema_version: Literal["network_comparison_geojson.v1"] = "network_comparison_geojson.v1"
    type: Literal["FeatureCollection"] = "FeatureCollection"
    features: list[NetworkMapFeature]


class NetworkCoverageGeoJson(DeliveryModel):
    """One assignment result and all of its straight-line coverage facts."""

    schema_version: Literal["network_coverage_geojson.v1"] = "network_coverage_geojson.v1"
    type: Literal["FeatureCollection"] = "FeatureCollection"
    features: list[NetworkMapFeature]


class NetworkMapSummary(DeliveryModel):
    country_code: str = Field(pattern=r"^[A-Z]{2}$")
    baseline_label: str
    facility_status: str
    feature_count: int = Field(ge=0)
    baseline_active_warehouse_ids: list[str]
    facility_active_warehouse_ids: list[str]
    opened_candidate_ids: list[str]
    closed_existing_ids: list[str]
    before_cost: float | None
    after_cost: float | None
    cost_delta: float | None
    coverage: list[CoverageComparison]


class NetworkComparisonMapBundle(DeliveryModel):
    schema_version: Literal["network_comparison_map_bundle.v1"] = "network_comparison_map_bundle.v1"
    kind: Literal["network_comparison_map"] = "network_comparison_map"
    summary: NetworkMapSummary
    geojson: NetworkMapFeatureCollection


def build_network_distribution_geojson(
    normalized: NormalizedInputBatch,
    *,
    include_candidates: bool,
    baseline: BaselineResult | None = None,
) -> NetworkDistributionGeoJson:
    """Build raw map features without prescribing presentation."""

    baseline_rows: Mapping[str, AssignmentRow] = {}
    if baseline is not None:
        baseline_rows = validate_baseline_delivery_inputs(
            normalized,
            baseline,
        ).baseline_rows_by_city
    features: list[NetworkMapFeature] = []
    for city in sorted(normalized.demand_cities, key=lambda item: item.city_id):
        row = baseline_rows.get(city.city_id)
        features.append(
            NetworkMapFeature(
                id=_feature_id("demand", city.city_id),
                geometry=PointGeometry(
                    coordinates=_required_coordinates(
                        "demand",
                        city.city_id,
                        city.longitude,
                        city.latitude,
                    )
                ),
                properties=DemandMapProperties(
                    city_id=city.city_id,
                    city_name=city.city_name,
                    province_id=city.province_id,
                    province_name=city.province_name,
                    demand_quantity=city.demand_quantity,
                    assigned_warehouse_id=row.warehouse_id if row else None,
                    distance_km=row.distance_km if row else None,
                    duration_hours=row.duration_hours if row else None,
                    unit_cost=row.cost if row else None,
                ),
            )
        )
    warehouses = [
        warehouse
        for warehouse in normalized.warehouses
        if warehouse.is_existing or include_candidates
    ]
    for warehouse in sorted(warehouses, key=lambda item: item.warehouse_id):
        features.append(
            NetworkMapFeature(
                id=_feature_id("warehouse", warehouse.warehouse_id),
                geometry=PointGeometry(
                    coordinates=_required_coordinates(
                        "warehouse",
                        warehouse.warehouse_id,
                        warehouse.longitude,
                        warehouse.latitude,
                    )
                ),
                properties=WarehouseMapProperties(
                    warehouse_id=warehouse.warehouse_id,
                    warehouse_name=warehouse.warehouse_name,
                    warehouse_type=warehouse.warehouse_type,
                    city_id=warehouse.city_id,
                    city_name=warehouse.city_name,
                    province_id=warehouse.province_id,
                    province_name=warehouse.province_name,
                    is_existing=warehouse.is_existing,
                    baseline_active=warehouse.is_existing,
                    facility_active=warehouse.is_existing,
                    opened_candidate=False,
                    closed_existing=False,
                ),
            )
        )
    return NetworkDistributionGeoJson(features=features)


def build_network_comparison_map_bundle(
    normalized: NormalizedInputBatch,
    baseline: BaselineResult,
    facility: PMedianSolution,
    comparison: AssignmentComparison,
    *,
    country_code: str,
) -> NetworkComparisonMapBundle:
    """Build a self-contained comparison map from exact prepared results."""

    validated = validate_delivery_inputs(normalized, baseline, facility, comparison)
    opened_ids = set(facility.opened_candidate_ids)
    closed_ids = set(facility.closed_existing_ids)
    features: list[NetworkMapFeature] = []
    for warehouse_id in sorted(validated.warehouse_by_id):
        warehouse = validated.warehouse_by_id[warehouse_id]
        coordinates = _required_coordinates(
            "warehouse",
            warehouse_id,
            warehouse.longitude,
            warehouse.latitude,
        )
        features.append(
            NetworkMapFeature(
                id=_feature_id("warehouse", warehouse_id),
                geometry=PointGeometry(coordinates=coordinates),
                properties=WarehouseMapProperties(
                    warehouse_id=warehouse_id,
                    warehouse_name=warehouse.warehouse_name,
                    warehouse_type=warehouse.warehouse_type,
                    city_id=warehouse.city_id,
                    city_name=warehouse.city_name,
                    province_id=warehouse.province_id,
                    province_name=warehouse.province_name,
                    is_existing=warehouse.is_existing,
                    baseline_active=warehouse_id in validated.baseline_active_ids,
                    facility_active=warehouse_id in validated.facility_active_ids,
                    opened_candidate=warehouse_id in opened_ids,
                    closed_existing=warehouse_id in closed_ids,
                ),
            )
        )
    for city_id in sorted(validated.demand_by_id):
        city = validated.demand_by_id[city_id]
        baseline_row = validated.baseline_rows_by_city[city_id]
        facility_row = validated.facility_rows_by_city[city_id]
        coordinates = _required_coordinates(
            "demand",
            city_id,
            city.longitude,
            city.latitude,
        )
        features.append(
            NetworkMapFeature(
                id=_feature_id("demand", city_id),
                geometry=PointGeometry(coordinates=coordinates),
                properties=DemandMapProperties(
                    city_id=city_id,
                    city_name=city.city_name,
                    province_id=city.province_id,
                    province_name=city.province_name,
                    demand_quantity=city.demand_quantity,
                    baseline_warehouse_id=baseline_row.warehouse_id,
                    baseline_distance_km=baseline_row.distance_km,
                    baseline_duration_hours=baseline_row.duration_hours,
                    baseline_unit_cost=baseline_row.cost,
                    facility_warehouse_id=facility_row.warehouse_id,
                    facility_distance_km=facility_row.distance_km,
                    facility_duration_hours=facility_row.duration_hours,
                    facility_unit_cost=facility_row.cost,
                ),
            )
        )
    features.extend(
        _assignment_features(
            "baseline",
            baseline.label,
            validated.baseline_rows_by_city,
            validated.warehouse_by_id,
            validated.demand_by_id,
        )
    )
    features.extend(
        _assignment_features(
            "facility",
            facility.status,
            validated.facility_rows_by_city,
            validated.warehouse_by_id,
            validated.demand_by_id,
        )
    )
    features.extend(
        _linehaul_features(
            "baseline",
            validated.baseline_active_ids,
            validated.baseline_rows_by_city,
            validated.warehouse_by_id,
        )
    )
    features.extend(
        _linehaul_features(
            "facility",
            validated.facility_active_ids,
            validated.facility_rows_by_city,
            validated.warehouse_by_id,
        )
    )
    return NetworkComparisonMapBundle(
        summary=NetworkMapSummary(
            country_code=country_code,
            baseline_label=baseline.label,
            facility_status=facility.status,
            feature_count=len(features),
            baseline_active_warehouse_ids=sorted(validated.baseline_active_ids),
            facility_active_warehouse_ids=sorted(validated.facility_active_ids),
            opened_candidate_ids=sorted(opened_ids),
            closed_existing_ids=sorted(closed_ids),
            before_cost=comparison.before_cost,
            after_cost=comparison.after_cost,
            cost_delta=comparison.cost_delta,
            coverage=sorted(comparison.coverage, key=lambda item: item.target_hours),
        ),
        geojson=NetworkMapFeatureCollection(features=features),
    )


def build_network_coverage_geojson(
    normalized: NormalizedInputBatch,
    assignment: AssignmentResult,
    active_warehouse_ids: list[str],
    *,
    result_label: str,
    scenario: Literal["baseline", "scenario", "facility"],
) -> NetworkCoverageGeoJson:
    """Build presentation-neutral after-result points and coverage lines.

    The function deliberately receives an already solved assignment.  It does
    not select facilities, calculate routes, or infer a business scenario.
    """

    if any(issue.severity == "error" for issue in normalized.issues):
        raise ValueError("delivery_normalized_input_has_errors")
    warehouses = {item.warehouse_id: item for item in normalized.warehouses}
    cities = {item.city_id: item for item in normalized.demand_cities}
    if len(warehouses) != len(normalized.warehouses) or len(cities) != len(normalized.demand_cities):
        raise ValueError("delivery_duplicate_map_entity")
    active = set(active_warehouse_ids)
    if len(active) != len(active_warehouse_ids) or not active <= set(warehouses):
        raise ValueError("delivery_active_warehouse_invalid")
    rows = {row.demand_city_id: row for row in assignment.rows}
    if len(rows) != len(assignment.rows) or set(rows) != set(cities):
        raise ValueError("delivery_assignment_city_set_mismatch")
    for city_id, row in rows.items():
        if row.demand_quantity != cities[city_id].demand_quantity:
            raise ValueError(f"delivery_assignment_demand_mismatch:{city_id}")
        if row.warehouse_id is not None and row.warehouse_id not in active:
            raise ValueError(f"delivery_assignment_warehouse_inactive:{row.warehouse_id}")
    features: list[NetworkMapFeature] = []
    for warehouse_id in sorted(warehouses):
        warehouse = warehouses[warehouse_id]
        features.append(
            NetworkMapFeature(
                id=_feature_id("warehouse", warehouse_id),
                geometry=PointGeometry(
                    coordinates=_required_coordinates(
                        "warehouse", warehouse_id, warehouse.longitude, warehouse.latitude
                    )
                ),
                properties=WarehouseMapProperties(
                    warehouse_id=warehouse_id,
                    warehouse_name=warehouse.warehouse_name,
                    warehouse_type=warehouse.warehouse_type,
                    city_id=warehouse.city_id,
                    city_name=warehouse.city_name,
                    province_id=warehouse.province_id,
                    province_name=warehouse.province_name,
                    is_existing=warehouse.is_existing,
                    baseline_active=warehouse.is_existing,
                    facility_active=warehouse_id in active,
                    opened_candidate=not warehouse.is_existing and warehouse_id in active,
                    closed_existing=warehouse.is_existing and warehouse_id not in active,
                ),
            )
        )
    for city_id in sorted(cities):
        city = cities[city_id]
        row = rows[city_id]
        features.append(
            NetworkMapFeature(
                id=_feature_id("demand", city_id),
                geometry=PointGeometry(
                    coordinates=_required_coordinates("demand", city_id, city.longitude, city.latitude)
                ),
                properties=DemandMapProperties(
                    city_id=city_id,
                    city_name=city.city_name,
                    province_id=city.province_id,
                    province_name=city.province_name,
                    demand_quantity=city.demand_quantity,
                    assigned_warehouse_id=row.warehouse_id,
                    distance_km=row.distance_km,
                    duration_hours=row.duration_hours,
                    unit_cost=row.cost,
                ),
            )
        )
    features.extend(_assignment_features(scenario, result_label, rows, warehouses, cities))
    features.extend(_linehaul_features(scenario, frozenset(active), rows, warehouses))
    return NetworkCoverageGeoJson(features=features)


def _assignment_features(
    scenario: Literal["baseline", "scenario", "facility"],
    result_label: str,
    rows_by_city: Mapping[str, AssignmentRow],
    warehouse_by_id: Mapping[str, WarehouseRecord],
    demand_by_id: Mapping[str, DemandCityRecord],
) -> list[NetworkMapFeature]:
    features: list[NetworkMapFeature] = []
    for city_id in sorted(rows_by_city):
        row = rows_by_city[city_id]
        if row.warehouse_id is None:
            continue
        warehouse = warehouse_by_id[row.warehouse_id]
        city = demand_by_id[city_id]
        origin = _required_coordinates(
            "warehouse",
            warehouse.warehouse_id,
            warehouse.longitude,
            warehouse.latitude,
        )
        destination = _required_coordinates(
            "demand",
            city_id,
            city.longitude,
            city.latitude,
        )
        features.append(
            NetworkMapFeature(
                id=_feature_id(
                    scenario,
                    "last_mile",
                    warehouse.warehouse_id,
                    city_id,
                ),
                geometry=LineStringGeometry(
                    coordinates=(origin, destination),
                ),
                properties=AssignmentMapProperties(
                    scenario=scenario,
                    result_label=result_label,
                    warehouse_id=warehouse.warehouse_id,
                    demand_city_id=city_id,
                    demand_quantity=row.demand_quantity,
                    distance_km=row.distance_km,
                    duration_hours=row.duration_hours,
                    unit_cost=row.cost,
                ),
            )
        )
    return features


def _linehaul_features(
    scenario: Literal["baseline", "scenario", "facility"],
    active_ids: frozenset[str],
    rows_by_city: Mapping[str, AssignmentRow],
    warehouse_by_id: Mapping[str, WarehouseRecord],
) -> list[NetworkMapFeature]:
    assigned_demand = {
        warehouse_id: sum(
            (
                row.demand_quantity
                for row in rows_by_city.values()
                if row.warehouse_id == warehouse_id
            ),
            start=Decimal(0),
        )
        for warehouse_id in active_ids
    }
    features: list[NetworkMapFeature] = []
    for warehouse_id in sorted(active_ids):
        warehouse = warehouse_by_id[warehouse_id]
        if warehouse.warehouse_type != "cross_docking":
            continue
        if warehouse.upstream_center_id is None:
            raise ValueError(f"delivery_linehaul_upstream_missing:{warehouse_id}")
        upstream = warehouse_by_id.get(warehouse.upstream_center_id)
        if upstream is None:
            raise ValueError(f"delivery_linehaul_upstream_missing:{warehouse_id}")
        origin = _required_coordinates(
            "warehouse",
            upstream.warehouse_id,
            upstream.longitude,
            upstream.latitude,
        )
        destination = _required_coordinates(
            "warehouse",
            warehouse_id,
            warehouse.longitude,
            warehouse.latitude,
        )
        features.append(
            NetworkMapFeature(
                id=_feature_id(
                    scenario,
                    "linehaul",
                    upstream.warehouse_id,
                    warehouse_id,
                ),
                geometry=LineStringGeometry(
                    coordinates=(origin, destination),
                ),
                properties=LinehaulMapProperties(
                    scenario=scenario,
                    upstream_center_id=upstream.warehouse_id,
                    crossdock_warehouse_id=warehouse_id,
                    assigned_demand=assigned_demand[warehouse_id],
                ),
            )
        )
    return features


def _required_coordinates(
    entity: str,
    entity_id: str,
    longitude: float | None,
    latitude: float | None,
) -> tuple[float, float]:
    if longitude is None or latitude is None:
        raise ValueError(f"delivery_map_coordinates_required:{entity}:{entity_id}")
    return longitude, latitude


def _feature_id(*parts: str) -> str:
    return "|".join(f"{len(part)}:{part}" for part in parts)
