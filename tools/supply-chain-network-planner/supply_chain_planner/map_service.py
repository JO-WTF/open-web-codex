"""Deterministic network comparison map bundles and legacy publication."""

from __future__ import annotations

import hashlib
from collections.abc import Mapping
from decimal import Decimal
from pathlib import Path
from typing import TYPE_CHECKING, Literal
from uuid import UUID

from pydantic import Field

from .delivery_models import DeliveryModel, validate_delivery_inputs
from .mcp_contracts import MapResourceRef
from .network_models import DemandCityRecord, NormalizedInputBatch, WarehouseRecord
from .optimization_models import (
    AssignmentComparison,
    AssignmentRow,
    BaselineResult,
    PMedianSolution,
    ServiceComparison,
)

if TYPE_CHECKING:
    from .case_repository import CaseRepository
    from .case_types import CaseOperationResult
    from .resource_store import PublishedResource, ResourceStore

CandidateSource = Literal["scenario", "facility_location"]


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
    WarehouseMapProperties
    | DemandMapProperties
    | AssignmentMapProperties
    | LinehaulMapProperties
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

    schema_version: Literal["network_distribution_geojson.v1"] = (
        "network_distribution_geojson.v1"
    )
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
    service: list[ServiceComparison]


class NetworkComparisonMapBundle(DeliveryModel):
    schema_version: Literal["network_comparison_map_bundle.v1"] = (
        "network_comparison_map_bundle.v1"
    )
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
        legend_items.append(
            {"label": "Candidate warehouse", "color": "#16A34A", "type": "circle"}
        )

    summary = (
        "Demand cities and existing warehouses"
        + (" with candidate warehouses." if include_candidates else ".")
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
            service=sorted(comparison.service, key=lambda item: item.target_hours),
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


class NetworkMapService:
    def __init__(self, repository: CaseRepository, resource_store: ResourceStore):
        self.repository = repository
        self.resource_store = resource_store

    def publish_comparison(
        self,
        case_id: UUID,
        workspace_root: Path,
        candidate_source: CandidateSource,
    ) -> tuple[PublishedResource, dict[str, object], CaseOperationResult]:
        from .case_types import FacetName

        normalized, normalized_id = self.repository.load_normalized_input(
            case_id, workspace_root
        )
        baseline, baseline_id = self.repository.load_baseline(case_id, workspace_root)
        if candidate_source == "scenario":
            candidate, candidate_id = self.repository.load_scenario(
                case_id, workspace_root
            )
            candidate_assignment = candidate.assignment
            candidate_label = "scenario"
            candidate_service = candidate.service
        else:
            candidate, candidate_id = self.repository.load_facility_solution(
                case_id, workspace_root
            )
            if candidate.assignment is None:
                raise ValueError("facility_solution_has_no_assignment")
            candidate_assignment = candidate.assignment
            candidate_label = candidate.status
            candidate_service = candidate.service

        warehouses = {item.warehouse_id: item for item in normalized.warehouses}
        demand = {item.city_id: item for item in normalized.demand_cities}
        baseline_active = {
            row.warehouse_id
            for row in baseline.assignment.rows
            if row.warehouse_id is not None
        }
        candidate_active = {
            row.warehouse_id
            for row in candidate_assignment.rows
            if row.warehouse_id is not None
        }
        features: list[dict[str, object]] = []
        for warehouse in sorted(warehouses.values(), key=lambda item: item.warehouse_id):
            if warehouse.longitude is None or warehouse.latitude is None:
                continue
            features.append(
                {
                    "type": "Feature",
                    "geometry": {
                        "type": "Point",
                        "coordinates": [warehouse.longitude, warehouse.latitude],
                    },
                    "properties": {
                        "kind": "warehouse",
                        "warehouse_id": warehouse.warehouse_id,
                        "warehouse_name": warehouse.warehouse_name,
                        "warehouse_type": warehouse.warehouse_type,
                        "is_existing": warehouse.is_existing,
                        "baseline_active": warehouse.warehouse_id in baseline_active,
                        "candidate_active": warehouse.warehouse_id in candidate_active,
                    },
                }
            )
        for city in sorted(demand.values(), key=lambda item: item.city_id):
            if city.longitude is None or city.latitude is None:
                continue
            features.append(
                {
                    "type": "Feature",
                    "geometry": {
                        "type": "Point",
                        "coordinates": [city.longitude, city.latitude],
                    },
                    "properties": {
                        "kind": "demand",
                        "city_id": city.city_id,
                        "city_name": city.city_name,
                        "demand_quantity": str(city.demand_quantity),
                    },
                }
            )
        for scenario, result_label, assignment in (
            ("baseline", baseline.label, baseline.assignment),
            ("candidate", candidate_label, candidate_assignment),
        ):
            for row in assignment.rows:
                warehouse = warehouses.get(row.warehouse_id or "")
                city = demand.get(row.demand_city_id)
                if (
                    warehouse is None
                    or city is None
                    or warehouse.longitude is None
                    or warehouse.latitude is None
                    or city.longitude is None
                    or city.latitude is None
                ):
                    continue
                features.append(
                    {
                        "type": "Feature",
                        "geometry": {
                            "type": "LineString",
                            "coordinates": [
                                [warehouse.longitude, warehouse.latitude],
                                [city.longitude, city.latitude],
                            ],
                        },
                        "properties": {
                            "kind": "assignment",
                            "scenario": scenario,
                            "result_label": result_label,
                            "warehouse_id": warehouse.warehouse_id,
                            "demand_city_id": city.city_id,
                        },
                    }
                )
        metadata = {
            "case_id": str(case_id),
            "baseline_label": baseline.label,
            "candidate_source": candidate_source,
            "candidate_label": candidate_label,
            "baseline_service": [
                item.model_dump(mode="json") for item in baseline.service
            ],
            "candidate_service": [
                item.model_dump(mode="json") for item in candidate_service
            ],
        }
        geojson = {
            "type": "FeatureCollection",
            "schema_version": "network_comparison_map.v1",
            "metadata": metadata,
            "features": features,
        }
        published = self.resource_store.publish("network_comparison_map.v1", geojson)
        raw = self.resource_store.read(published.resource_id).encode("utf-8")
        content_sha256 = hashlib.sha256(raw).hexdigest()
        lease = self.repository.begin_operation(
            case_id,
            workspace_root,
            "publish_network_comparison_map",
            {
                "normalized_component_id": str(normalized_id),
                "baseline_component_id": str(baseline_id),
                "candidate_component_id": str(candidate_id),
                "candidate_source": candidate_source,
                "content_sha256": content_sha256,
            },
        )
        summary = {
            "candidate_source": candidate_source,
            "feature_count": len(features),
            "baseline_active_warehouse_count": len(baseline_active),
            "candidate_active_warehouse_count": len(candidate_active),
        }
        result = self.repository.commit_deliverable(
            lease,
            workspace_root,
            component_kind="network_comparison_map.v1",
            facet=FacetName.MAP,
            artifact_schema="network_comparison_map.v1",
            media_type="application/geo+json",
            byte_size=len(raw),
            content_sha256=content_sha256,
            storage_name=published.resource_id,
            summary=summary,
            depends_on=[normalized_id, baseline_id, candidate_id],
        )
        return published, summary, result
