"""Deterministic network comparison map bundles and legacy publication."""

from __future__ import annotations

from collections.abc import Mapping
from decimal import Decimal
from typing import Literal

from pydantic import Field
from supply_chain_planner.delivery.models import DeliveryModel, validate_delivery_inputs
from supply_chain_planner.network.models import (
    DemandCityRecord,
    NormalizedInputBatch,
    WarehouseRecord,
)
from supply_chain_planner.network.optimization_models import (
    AssignmentComparison,
    AssignmentRow,
    BaselineResult,
    CoverageComparison,
    PMedianSolution,
)
from supply_chain_planner.resources.contracts import MapResourceRef


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
    demand_quantity: Decimal


class AssignmentMapProperties(DeliveryModel):
    kind: Literal["last_mile_assignment"] = "last_mile_assignment"
    scenario: Literal["baseline", "facility"]
    result_label: str
    warehouse_id: str
    demand_city_id: str
    demand_quantity: Decimal
    distance_km: float | None
    duration_hours: float | None
    unit_cost: float | None


class LinehaulMapProperties(DeliveryModel):
    kind: Literal["linehaul_connection"] = "linehaul_connection"
    scenario: Literal["baseline", "facility"]
    upstream_center_id: str
    crossdock_warehouse_id: str
    assigned_demand: Decimal


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


class MapCardToolTarget(DeliveryModel):
    server: Literal["map_utils"] = "map_utils"
    name: Literal["create_map_card"] = "create_map_card"


class MapCardGeoJsonSource(DeliveryModel):
    type: Literal["geojson"] = "geojson"
    data_ref: MapResourceRef


class NetworkDistributionMapCardArguments(DeliveryModel):
    title: str
    intent: Literal["visualization"] = "visualization"
    fallback_text: str
    summary: str
    sources: dict[str, MapCardGeoJsonSource]
    layers: list[dict[str, object]]
    extensions: dict[str, object]


class NetworkDistributionMapCardHandoff(DeliveryModel):
    """Exact cross-Tool handoff for the generic map-card provider."""

    schema_version: Literal["network_distribution_map_card_handoff.v1"] = Field(
        default="network_distribution_map_card_handoff.v1",
        alias="schemaVersion",
    )
    tool: MapCardToolTarget = Field(default_factory=MapCardToolTarget)
    arguments: NetworkDistributionMapCardArguments


class NetworkComparisonMapCardHandoff(DeliveryModel):
    """Exact comparison-map handoff for the generic map-card provider."""

    schema_version: Literal["network_comparison_map_card_handoff.v1"] = Field(
        default="network_comparison_map_card_handoff.v1",
        alias="schemaVersion",
    )
    tool: MapCardToolTarget = Field(default_factory=MapCardToolTarget)
    arguments: NetworkDistributionMapCardArguments


class NetworkMapLayer(DeliveryModel):
    layer_id: str
    feature_kind: str
    geometry_type: Literal["Point", "LineString"]
    scenario: Literal["baseline", "facility"] | None
    label: str


class NetworkMapLegendItem(DeliveryModel):
    code: str
    label: str
    color: str = Field(pattern=r"^#[0-9A-F]{6}$")


class NetworkMapExtensions(DeliveryModel):
    legend: list[NetworkMapLegendItem]
    hover_fields: dict[str, list[str]]


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
    title: str = "Warehouse network: baseline vs selected facilities"
    summary: NetworkMapSummary
    geojson: NetworkMapFeatureCollection
    layers: list[NetworkMapLayer]
    extensions: NetworkMapExtensions


def build_network_distribution_geojson(
    normalized: NormalizedInputBatch,
    *,
    include_candidates: bool,
) -> NetworkDistributionGeoJson:
    """Build demand and warehouse points without routing or optimization."""

    features: list[NetworkMapFeature] = []
    for city in sorted(normalized.demand_cities, key=lambda item: item.city_id):
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


def build_network_distribution_map_card_handoff(
    data_ref: MapResourceRef,
    *,
    include_candidates: bool,
) -> NetworkDistributionMapCardHandoff:
    """Build the exact generic map-card call without exposing GeoJSON to the model."""

    source_id = "network-distribution"
    layers: list[dict[str, object]] = [
        {
            "id": "demand-cities",
            "type": "circle",
            "source": source_id,
            "filter": ["==", ["get", "kind"], "demand"],
            "paint": {
                "circle-color": "#F59E0B",
                "circle-opacity": 0.78,
                "circle-radius": [
                    "interpolate",
                    ["linear"],
                    ["to-number", ["get", "demand_quantity"]],
                    0,
                    4,
                    10000,
                    14,
                ],
                "circle-stroke-color": "#7C2D12",
                "circle-stroke-width": 1,
            },
        },
        {
            "id": "center-warehouses",
            "type": "circle",
            "source": source_id,
            "filter": [
                "all",
                ["==", ["get", "kind"], "warehouse"],
                ["==", ["get", "is_existing"], True],
                ["==", ["get", "warehouse_type"], "center"],
            ],
            "paint": {
                "circle-color": "#DC2626",
                "circle-radius": 9,
                "circle-stroke-color": "#FFFFFF",
                "circle-stroke-width": 2,
            },
        },
        {
            "id": "cross-docking-warehouses",
            "type": "circle",
            "source": source_id,
            "filter": [
                "all",
                ["==", ["get", "kind"], "warehouse"],
                ["==", ["get", "is_existing"], True],
                ["==", ["get", "warehouse_type"], "cross_docking"],
            ],
            "paint": {
                "circle-color": "#2563EB",
                "circle-radius": 7,
                "circle-stroke-color": "#FFFFFF",
                "circle-stroke-width": 2,
            },
        },
        {
            "id": "warehouse-labels",
            "type": "symbol",
            "source": source_id,
            "filter": [
                "all",
                ["==", ["get", "kind"], "warehouse"],
                ["==", ["get", "is_existing"], True],
            ],
            "layout": {
                "text-field": ["get", "warehouse_name"],
                "text-size": 11,
                "text-offset": [0, 1.3],
                "text-anchor": "top",
            },
            "paint": {
                "text-color": "#111827",
                "text-halo-color": "#FFFFFF",
                "text-halo-width": 1,
            },
        },
    ]
    legend_items: list[dict[str, object]] = [
        {"label": "Demand city", "color": "#F59E0B", "type": "circle"},
        {"label": "Center warehouse", "color": "#DC2626", "type": "circle"},
        {
            "label": "Cross-docking warehouse",
            "color": "#2563EB",
            "type": "circle",
        },
    ]
    if include_candidates:
        layers.insert(
            3,
            {
                "id": "candidate-warehouses",
                "type": "circle",
                "source": source_id,
                "filter": [
                    "all",
                    ["==", ["get", "kind"], "warehouse"],
                    ["==", ["get", "is_existing"], False],
                ],
                "paint": {
                    "circle-color": "#16A34A",
                    "circle-radius": 6,
                    "circle-stroke-color": "#FFFFFF",
                    "circle-stroke-width": 2,
                },
            },
        )
        legend_items.append({"label": "Candidate warehouse", "color": "#16A34A", "type": "circle"})

    summary = "Demand cities and existing warehouses" + (
        " with candidate warehouses." if include_candidates else "."
    )
    return NetworkDistributionMapCardHandoff(
        arguments=NetworkDistributionMapCardArguments(
            title="Warehouse network distribution",
            fallback_text=summary,
            summary=summary,
            sources={
                source_id: MapCardGeoJsonSource(data_ref=data_ref),
            },
            layers=layers,
            extensions={
                "hover": {
                    "layers": [
                        {
                            "layer": "demand-cities",
                            "title_property": "city_name",
                            "fields": ["province_name", "demand_quantity"],
                        },
                        {
                            "layer": "center-warehouses",
                            "title_property": "warehouse_name",
                            "fields": ["warehouse_type", "city_name"],
                        },
                        {
                            "layer": "cross-docking-warehouses",
                            "title_property": "warehouse_name",
                            "fields": ["warehouse_type", "city_name"],
                        },
                    ]
                    + (
                        [
                            {
                                "layer": "candidate-warehouses",
                                "title_property": "warehouse_name",
                                "fields": ["warehouse_type", "city_name"],
                            }
                        ]
                        if include_candidates
                        else []
                    ),
                },
                "legend": {
                    "title": "Network features",
                    "items": legend_items,
                },
            },
        )
    )


def build_network_comparison_map_card_handoff(
    data_ref: MapResourceRef,
) -> NetworkComparisonMapCardHandoff:
    """Build a bounded baseline-versus-plan map-card call from typed GeoJSON."""

    source_id = "network-comparison"
    layers: list[dict[str, object]] = [
        {
            "id": "baseline-assignments",
            "type": "line",
            "source": source_id,
            "filter": [
                "all",
                ["==", ["get", "kind"], "last_mile_assignment"],
                ["==", ["get", "scenario"], "baseline"],
            ],
            "paint": {
                "line-color": "#64748B",
                "line-opacity": 0.24,
                "line-width": 1,
            },
        },
        {
            "id": "planned-assignments",
            "type": "line",
            "source": source_id,
            "filter": [
                "all",
                ["==", ["get", "kind"], "last_mile_assignment"],
                ["==", ["get", "scenario"], "facility"],
            ],
            "paint": {
                "line-color": "#16A34A",
                "line-opacity": 0.58,
                "line-width": 2,
            },
        },
        {
            "id": "demand-cities",
            "type": "circle",
            "source": source_id,
            "filter": ["==", ["get", "kind"], "demand"],
            "paint": {
                "circle-color": "#F59E0B",
                "circle-opacity": 0.78,
                "circle-radius": 5,
                "circle-stroke-color": "#7C2D12",
                "circle-stroke-width": 1,
            },
        },
        {
            "id": "active-existing-warehouses",
            "type": "circle",
            "source": source_id,
            "filter": [
                "all",
                ["==", ["get", "kind"], "warehouse"],
                ["==", ["get", "is_existing"], True],
                ["==", ["get", "facility_active"], True],
            ],
            "paint": {
                "circle-color": "#2563EB",
                "circle-radius": 8,
                "circle-stroke-color": "#FFFFFF",
                "circle-stroke-width": 2,
            },
        },
        {
            "id": "opened-candidates",
            "type": "circle",
            "source": source_id,
            "filter": [
                "all",
                ["==", ["get", "kind"], "warehouse"],
                ["==", ["get", "opened_candidate"], True],
            ],
            "paint": {
                "circle-color": "#16A34A",
                "circle-radius": 9,
                "circle-stroke-color": "#FFFFFF",
                "circle-stroke-width": 2,
            },
        },
        {
            "id": "closed-existing-warehouses",
            "type": "circle",
            "source": source_id,
            "filter": [
                "all",
                ["==", ["get", "kind"], "warehouse"],
                ["==", ["get", "closed_existing"], True],
            ],
            "paint": {
                "circle-color": "#DC2626",
                "circle-radius": 9,
                "circle-stroke-color": "#7F1D1D",
                "circle-stroke-width": 2,
            },
        },
        {
            "id": "warehouse-labels",
            "type": "symbol",
            "source": source_id,
            "filter": [
                "all",
                ["==", ["get", "kind"], "warehouse"],
                ["==", ["get", "facility_active"], True],
            ],
            "layout": {
                "text-field": ["get", "warehouse_name"],
                "text-size": 11,
                "text-offset": [0, 1.3],
                "text-anchor": "top",
            },
            "paint": {
                "text-color": "#111827",
                "text-halo-color": "#FFFFFF",
                "text-halo-width": 1,
            },
        },
    ]
    summary = (
        "Baseline and planned warehouse-to-demand assignments, including "
        "opened and closed facilities."
    )
    return NetworkComparisonMapCardHandoff(
        arguments=NetworkDistributionMapCardArguments(
            title="Warehouse network comparison",
            fallback_text=summary,
            summary=summary,
            sources={source_id: MapCardGeoJsonSource(data_ref=data_ref)},
            layers=layers,
            extensions={
                "hover": {
                    "layers": [
                        {
                            "layer": "demand-cities",
                            "title_property": "city_name",
                            "fields": ["province_name", "demand_quantity"],
                        },
                        {
                            "layer": "active-existing-warehouses",
                            "title_property": "warehouse_name",
                            "fields": ["warehouse_type", "city_name"],
                        },
                        {
                            "layer": "opened-candidates",
                            "title_property": "warehouse_name",
                            "fields": ["warehouse_type", "city_name"],
                        },
                        {
                            "layer": "closed-existing-warehouses",
                            "title_property": "warehouse_name",
                            "fields": ["warehouse_type", "city_name"],
                        },
                    ]
                },
                "legend": {
                    "title": "Network comparison",
                    "items": [
                        {"label": "Baseline assignment", "color": "#64748B", "type": "line"},
                        {"label": "Planned assignment", "color": "#16A34A", "type": "line"},
                        {"label": "Demand city", "color": "#F59E0B", "type": "circle"},
                        {
                            "label": "Active existing warehouse",
                            "color": "#2563EB",
                            "type": "circle",
                        },
                        {"label": "Opened candidate", "color": "#16A34A", "type": "circle"},
                        {
                            "label": "Closed existing warehouse",
                            "color": "#DC2626",
                            "type": "circle",
                        },
                    ],
                },
            },
        )
    )


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
        layers=_map_layers(),
        extensions=_map_extensions(),
    )


def _assignment_features(
    scenario: Literal["baseline", "facility"],
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
    scenario: Literal["baseline", "facility"],
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
        upstream = warehouse_by_id[warehouse.upstream_center_id]
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


def _map_layers() -> list[NetworkMapLayer]:
    return [
        NetworkMapLayer(
            layer_id="warehouses",
            feature_kind="warehouse",
            geometry_type="Point",
            scenario=None,
            label="Warehouses",
        ),
        NetworkMapLayer(
            layer_id="demand",
            feature_kind="demand",
            geometry_type="Point",
            scenario=None,
            label="Demand cities",
        ),
        NetworkMapLayer(
            layer_id="baseline-last-mile",
            feature_kind="last_mile_assignment",
            geometry_type="LineString",
            scenario="baseline",
            label="Baseline last-mile assignments",
        ),
        NetworkMapLayer(
            layer_id="facility-last-mile",
            feature_kind="last_mile_assignment",
            geometry_type="LineString",
            scenario="facility",
            label="Selected-facility last-mile assignments",
        ),
        NetworkMapLayer(
            layer_id="baseline-linehaul",
            feature_kind="linehaul_connection",
            geometry_type="LineString",
            scenario="baseline",
            label="Baseline linehaul connections",
        ),
        NetworkMapLayer(
            layer_id="facility-linehaul",
            feature_kind="linehaul_connection",
            geometry_type="LineString",
            scenario="facility",
            label="Selected-facility linehaul connections",
        ),
    ]


def _map_extensions() -> NetworkMapExtensions:
    return NetworkMapExtensions(
        legend=[
            NetworkMapLegendItem(
                code="existing",
                label="Existing warehouse",
                color="#2563EB",
            ),
            NetworkMapLegendItem(
                code="opened_candidate",
                label="Opened candidate",
                color="#16A34A",
            ),
            NetworkMapLegendItem(
                code="demand",
                label="Demand city",
                color="#DC2626",
            ),
            NetworkMapLegendItem(
                code="baseline",
                label="Baseline connection",
                color="#64748B",
            ),
            NetworkMapLegendItem(
                code="facility",
                label="Selected-facility connection",
                color="#F59E0B",
            ),
        ],
        hover_fields={
            "warehouse": [
                "warehouse_name",
                "warehouse_type",
                "baseline_active",
                "facility_active",
            ],
            "demand": ["city_name", "province_name", "demand_quantity"],
            "last_mile_assignment": [
                "scenario",
                "warehouse_id",
                "demand_city_id",
                "duration_hours",
                "unit_cost",
            ],
            "linehaul_connection": [
                "scenario",
                "upstream_center_id",
                "crossdock_warehouse_id",
                "assigned_demand",
            ],
        },
    )
