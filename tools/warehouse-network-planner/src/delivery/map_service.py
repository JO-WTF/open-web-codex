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
    ComparableNetworkView,
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
    before_active: bool
    after_active: bool
    added_facility: bool
    removed_facility: bool


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
    before_warehouse_id: str | None = None
    before_distance_km: float | None = Field(default=None, ge=0)
    before_duration_hours: float | None = Field(default=None, ge=0)
    before_unit_cost: float | None = Field(default=None, ge=0)
    after_warehouse_id: str | None = None
    after_distance_km: float | None = Field(default=None, ge=0)
    after_duration_hours: float | None = Field(default=None, ge=0)
    after_unit_cost: float | None = Field(default=None, ge=0)


class AssignmentMapProperties(DeliveryModel):
    kind: Literal["last_mile_assignment"] = "last_mile_assignment"
    scenario: Literal["before", "after", "scenario"]
    result_label: str
    warehouse_id: str
    demand_city_id: str
    demand_quantity: float = Field(ge=0)
    distance_km: float | None
    duration_hours: float | None
    unit_cost: float | None


class ComparisonDemandMapProperties(DemandMapProperties):
    """Demand facts for one comparison map at its selected SLA target."""

    after_service_status: Literal["attained", "missed", "unassigned"]


class ComparisonAssignmentMapProperties(AssignmentMapProperties):
    """One before/after assignment line at the selected SLA target."""

    service_status: Literal["attained", "missed", "unassigned"]


class LinehaulMapProperties(DeliveryModel):
    kind: Literal["linehaul_connection"] = "linehaul_connection"
    scenario: Literal["before", "after", "scenario"]
    upstream_center_id: str
    crossdock_warehouse_id: str
    assigned_demand: float = Field(ge=0)


MapProperties = (
    WarehouseMapProperties | DemandMapProperties | AssignmentMapProperties | LinehaulMapProperties
)


ComparisonMapProperties = (
    WarehouseMapProperties
    | ComparisonDemandMapProperties
    | ComparisonAssignmentMapProperties
    | LinehaulMapProperties
)


class CoverageDemandMapProperties(DemandMapProperties):
    service_status: Literal["attained", "missed", "unassigned"]


class CoverageAssignmentMapProperties(AssignmentMapProperties):
    service_status: Literal["attained", "missed", "unassigned"]


CoverageMapProperties = (
    WarehouseMapProperties
    | CoverageDemandMapProperties
    | CoverageAssignmentMapProperties
    | LinehaulMapProperties
)


class NetworkMapFeature(DeliveryModel):
    type: Literal["Feature"] = "Feature"
    id: str
    geometry: PointGeometry | LineStringGeometry
    properties: MapProperties


class ComparisonNetworkMapFeature(DeliveryModel):
    type: Literal["Feature"] = "Feature"
    id: str
    geometry: PointGeometry | LineStringGeometry
    properties: ComparisonMapProperties


class CoverageNetworkMapFeature(DeliveryModel):
    type: Literal["Feature"] = "Feature"
    id: str
    geometry: PointGeometry | LineStringGeometry
    properties: CoverageMapProperties


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

    schema_version: Literal["network_comparison_geojson.v3"] = "network_comparison_geojson.v3"
    service_target_hours: float = Field(gt=0)
    type: Literal["FeatureCollection"] = "FeatureCollection"
    features: list[ComparisonNetworkMapFeature]


class NetworkCoverageGeoJson(DeliveryModel):
    """One assignment result and all of its straight-line coverage facts."""

    schema_version: Literal["network_coverage_geojson.v2"] = "network_coverage_geojson.v2"
    service_target_hours: float = Field(gt=0)
    type: Literal["FeatureCollection"] = "FeatureCollection"
    features: list[CoverageNetworkMapFeature]


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
                    before_active=warehouse.is_existing,
                    after_active=warehouse.is_existing,
                    added_facility=False,
                    removed_facility=False,
                ),
            )
        )
    return NetworkDistributionGeoJson(features=features)


def build_network_comparison_geojson(
    normalized: NormalizedInputBatch,
    before: ComparableNetworkView,
    after: ComparableNetworkView,
    comparison: AssignmentComparison,
    *,
    service_target_hours: float,
) -> NetworkComparisonGeoJson:
    """Build comparison GeoJSON for any exact before/after result pair."""

    if service_target_hours not in comparison.requested_service_targets:
        raise ValueError("comparison_map_service_target_unavailable")
    validated = validate_delivery_inputs(normalized, before, after, comparison)
    added_ids = set(after.active_warehouse_ids) - set(before.active_warehouse_ids)
    removed_ids = set(before.active_warehouse_ids) - set(after.active_warehouse_ids)
    features: list[ComparisonNetworkMapFeature] = []
    for warehouse_id in sorted(validated.warehouse_by_id):
        warehouse = validated.warehouse_by_id[warehouse_id]
        coordinates = _required_coordinates(
            "warehouse",
            warehouse_id,
            warehouse.longitude,
            warehouse.latitude,
        )
        features.append(
            ComparisonNetworkMapFeature(
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
                    before_active=warehouse_id in validated.before_active_ids,
                    after_active=warehouse_id in validated.after_active_ids,
                    added_facility=warehouse_id in added_ids,
                    removed_facility=warehouse_id in removed_ids,
                ),
            )
        )
    for city_id in sorted(validated.demand_by_id):
        city = validated.demand_by_id[city_id]
        before_row = validated.before_rows_by_city[city_id]
        after_row = validated.after_rows_by_city[city_id]
        coordinates = _required_coordinates(
            "demand",
            city_id,
            city.longitude,
            city.latitude,
        )
        features.append(
            ComparisonNetworkMapFeature(
                id=_feature_id("demand", city_id),
                geometry=PointGeometry(coordinates=coordinates),
                properties=ComparisonDemandMapProperties(
                    city_id=city_id,
                    city_name=city.city_name,
                    province_id=city.province_id,
                    province_name=city.province_name,
                    demand_quantity=city.demand_quantity,
                    before_warehouse_id=before_row.warehouse_id,
                    before_distance_km=before_row.distance_km,
                    before_duration_hours=before_row.duration_hours,
                    before_unit_cost=before_row.cost,
                    after_warehouse_id=after_row.warehouse_id,
                    after_distance_km=after_row.distance_km,
                    after_duration_hours=after_row.duration_hours,
                    after_unit_cost=after_row.cost,
                    after_service_status=_service_status(after_row, service_target_hours),
                ),
            )
        )
    features.extend(
        _comparison_assignment_features(
            "before",
            before.label,
            validated.before_rows_by_city,
            validated.warehouse_by_id,
            validated.demand_by_id,
            service_target_hours=service_target_hours,
        )
    )
    features.extend(
        _comparison_assignment_features(
            "after",
            after.label,
            validated.after_rows_by_city,
            validated.warehouse_by_id,
            validated.demand_by_id,
            service_target_hours=service_target_hours,
        )
    )
    features.extend(
        _linehaul_features(
            "before",
            validated.before_active_ids,
            validated.before_rows_by_city,
            validated.warehouse_by_id,
            comparison=True,
        )
    )
    features.extend(
        _linehaul_features(
            "after",
            validated.after_active_ids,
            validated.after_rows_by_city,
            validated.warehouse_by_id,
            comparison=True,
        )
    )
    return NetworkComparisonGeoJson(
        service_target_hours=service_target_hours,
        features=features,
    )


def build_network_coverage_geojson(
    normalized: NormalizedInputBatch,
    assignment: AssignmentResult,
    active_warehouse_ids: list[str],
    *,
    result_label: str,
    scenario: Literal["before", "after", "scenario"],
    service_target_hours: float,
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
    features: list[CoverageNetworkMapFeature] = []
    for warehouse_id in sorted(warehouses):
        warehouse = warehouses[warehouse_id]
        features.append(
            CoverageNetworkMapFeature(
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
                    before_active=warehouse.is_existing,
                    after_active=warehouse_id in active,
                    added_facility=not warehouse.is_existing and warehouse_id in active,
                    removed_facility=warehouse.is_existing and warehouse_id not in active,
                ),
            )
        )
    for city_id in sorted(cities):
        city = cities[city_id]
        row = rows[city_id]
        features.append(
            CoverageNetworkMapFeature(
                id=_feature_id("demand", city_id),
                geometry=PointGeometry(
                    coordinates=_required_coordinates("demand", city_id, city.longitude, city.latitude)
                ),
                properties=CoverageDemandMapProperties(
                    city_id=city_id,
                    city_name=city.city_name,
                    province_id=city.province_id,
                    province_name=city.province_name,
                    demand_quantity=city.demand_quantity,
                    assigned_warehouse_id=row.warehouse_id,
                    distance_km=row.distance_km,
                    duration_hours=row.duration_hours,
                    unit_cost=row.cost,
                    service_status=_service_status(row, service_target_hours),
                ),
            )
        )
    features.extend(
        _assignment_features(
            scenario,
            result_label,
            rows,
            warehouses,
            cities,
            service_target_hours=service_target_hours,
        )
    )
    features.extend(
        _linehaul_features(
            scenario,
            frozenset(active),
            rows,
            warehouses,
            coverage=True,
        )
    )
    return NetworkCoverageGeoJson(
        service_target_hours=service_target_hours,
        features=features,
    )


def _assignment_features(
    scenario: Literal["before", "after", "scenario"],
    result_label: str,
    rows_by_city: Mapping[str, AssignmentRow],
    warehouse_by_id: Mapping[str, WarehouseRecord],
    demand_by_id: Mapping[str, DemandCityRecord],
    *,
    service_target_hours: float | None = None,
) -> list[NetworkMapFeature | CoverageNetworkMapFeature]:
    features: list[NetworkMapFeature | CoverageNetworkMapFeature] = []
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
        feature_type = CoverageNetworkMapFeature if service_target_hours is not None else NetworkMapFeature
        properties_type = (
            CoverageAssignmentMapProperties
            if service_target_hours is not None
            else AssignmentMapProperties
        )
        features.append(
            feature_type(
                id=_feature_id(
                    scenario,
                    "last_mile",
                    warehouse.warehouse_id,
                    city_id,
                ),
                geometry=LineStringGeometry(
                    coordinates=(origin, destination),
                ),
                properties=properties_type(
                    scenario=scenario,
                    result_label=result_label,
                    warehouse_id=warehouse.warehouse_id,
                    demand_city_id=city_id,
                    demand_quantity=row.demand_quantity,
                    distance_km=row.distance_km,
                    duration_hours=row.duration_hours,
                    unit_cost=row.cost,
                    **(
                        {"service_status": _service_status(row, service_target_hours)}
                        if service_target_hours is not None
                        else {}
                    ),
                ),
            )
        )
    return features


def _comparison_assignment_features(
    scenario: Literal["before", "after", "scenario"],
    result_label: str,
    rows_by_city: Mapping[str, AssignmentRow],
    warehouse_by_id: Mapping[str, WarehouseRecord],
    demand_by_id: Mapping[str, DemandCityRecord],
    *,
    service_target_hours: float,
) -> list[ComparisonNetworkMapFeature]:
    """Build the selected-target SLA facts for before and after assignment lines."""

    features: list[ComparisonNetworkMapFeature] = []
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
            ComparisonNetworkMapFeature(
                id=_feature_id(
                    scenario,
                    "last_mile",
                    warehouse.warehouse_id,
                    city_id,
                ),
                geometry=LineStringGeometry(coordinates=(origin, destination)),
                properties=ComparisonAssignmentMapProperties(
                    scenario=scenario,
                    result_label=result_label,
                    warehouse_id=warehouse.warehouse_id,
                    demand_city_id=city_id,
                    demand_quantity=row.demand_quantity,
                    distance_km=row.distance_km,
                    duration_hours=row.duration_hours,
                    unit_cost=row.cost,
                    service_status=_service_status(row, service_target_hours),
                ),
            )
        )
    return features


def _service_status(
    row: AssignmentRow,
    service_target_hours: float,
) -> Literal["attained", "missed", "unassigned"]:
    if row.warehouse_id is None or row.duration_hours is None:
        return "unassigned"
    return "attained" if row.duration_hours <= service_target_hours else "missed"


def _linehaul_features(
    scenario: Literal["before", "after", "scenario"],
    active_ids: frozenset[str],
    rows_by_city: Mapping[str, AssignmentRow],
    warehouse_by_id: Mapping[str, WarehouseRecord],
    *,
    coverage: bool = False,
    comparison: bool = False,
) -> list[NetworkMapFeature | ComparisonNetworkMapFeature | CoverageNetworkMapFeature]:
    if coverage and comparison:
        raise ValueError("delivery_linehaul_map_variant_invalid")
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
    features: list[NetworkMapFeature | ComparisonNetworkMapFeature | CoverageNetworkMapFeature] = []
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
        feature_type = (
            CoverageNetworkMapFeature
            if coverage
            else ComparisonNetworkMapFeature
            if comparison
            else NetworkMapFeature
        )
        features.append(
            feature_type(
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
