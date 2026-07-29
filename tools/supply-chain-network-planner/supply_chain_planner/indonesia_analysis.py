"""Streaming, deterministic analysis for the Indonesia tutorial release."""

from __future__ import annotations

import csv
import gzip
import hashlib
import json
import math
from array import array
from collections import defaultdict
from collections.abc import Iterable
from dataclasses import dataclass
from itertools import zip_longest
from typing import Any

from pydantic import ValidationError

from .geo import ProvinceBoundary, haversine_km
from .indonesia_models import (
    CandidateEvaluation,
    CandidateSummary,
    CostMetrics,
    CoverageMetrics,
    DatasetFileInspection,
    DatasetReleaseBinding,
    IndonesiaCandidateScenario,
    IndonesiaCurrentNetworkAnalysis,
    IndonesiaDatasetInspection,
    IndonesiaGeoJsonRef,
    IndonesiaLocationOptimization,
    IndonesiaNetworkMap,
    IndonesiaServiceBaseline,
    IndonesiaValidationResult,
    NetworkLink,
    ProvinceRankingPolicy,
    ProvinceServiceComparison,
    ProvinceServiceMetric,
    WarehouseMetric,
)
from .workspace_dataset import WorkspaceDatasetRelease

EXPECTED_FILES = {
    "dataset-manifest.json": ("dataset_manifest", "application/json"),
    "province-boundaries.geojson": (
        "province_boundaries",
        "application/geo+json",
    ),
    "customers.csv.gz": ("customers", "application/gzip"),
    "customer-assignments.csv.gz": (
        "customer_assignments",
        "application/gzip",
    ),
    "warehouses.csv": ("warehouses", "text/csv"),
    "warehouse-links.csv": ("warehouse_links", "text/csv"),
    "candidate-locations.csv": ("candidate_locations", "text/csv"),
    "transport-quotes.csv": ("transport_quotes", "text/csv"),
    "planning-policy.json": ("planning_policy", "application/json"),
    "validation-report.json": ("validation_report", "application/json"),
}

CUSTOMER_FIELDS = [
    "customer_id",
    "province_code",
    "province_name",
    "latitude",
    "longitude",
    "annual_demand_units",
    "customer_tier",
]
ASSIGNMENT_FIELDS = [
    "customer_id",
    "current_forward_id",
    "assignment_reason",
    "estimated_distance_km",
    "service_days",
    "nearest_forward_id",
    "nearest_distance_km",
]
WAREHOUSE_FIELDS = [
    "warehouse_id",
    "warehouse_name",
    "warehouse_type",
    "province_code",
    "province_name",
    "latitude",
    "longitude",
    "current_parent_center_id",
    "annual_capacity_units",
    "current_annual_demand_units",
    "utilization_ratio",
]
LINK_FIELDS = [
    "forward_warehouse_id",
    "current_center_warehouse_id",
    "linehaul_quote_id",
    "estimated_distance_km",
    "replenishment_days",
    "nearest_center_warehouse_id",
]
CANDIDATE_FIELDS = [
    "candidate_id",
    "candidate_name",
    "province_code",
    "province_name",
    "latitude",
    "longitude",
    "annual_capacity_units",
    "opening_cost_idr",
    "annual_fixed_cost_idr",
    "proposed_parent_center_id",
]
QUOTE_FIELDS = [
    "quote_id",
    "leg_type",
    "origin_warehouse_id",
    "destination_type",
    "destination_id",
    "destination_name",
    "haversine_km",
    "road_factor",
    "estimated_distance_km",
    "base_idr_per_demand_unit",
    "distance_rate_idr_per_km_per_demand_unit",
    "lane_factor",
    "noise_factor",
    "quoted_idr_per_demand_unit",
    "is_current_lane",
]


@dataclass(frozen=True)
class Warehouse:
    warehouse_id: str
    name: str
    warehouse_type: str
    province_code: str
    province_name: str
    latitude: float
    longitude: float
    parent_center_id: str | None
    capacity_units: int


@dataclass(frozen=True)
class Candidate:
    candidate_id: str
    name: str
    province_code: str
    province_name: str
    latitude: float
    longitude: float
    capacity_units: int
    opening_cost_idr: int
    annual_fixed_cost_idr: int
    parent_center_id: str


@dataclass(frozen=True)
class CustomerTable:
    province_indexes: array
    latitudes: array
    longitudes: array
    demand_units: array
    forward_indexes: array
    current_distances_km: array
    current_service_days: array
    province_customer_counts: tuple[int, ...]
    province_demand_units: tuple[int, ...]
    province_weighted_longitudes: tuple[float, ...]
    province_weighted_latitudes: tuple[float, ...]

    def __len__(self) -> int:
        return len(self.demand_units)


@dataclass(frozen=True)
class NetworkData:
    release: WorkspaceDatasetRelease
    internal_manifest: dict[str, Any]
    policy: dict[str, Any]
    validation_report: dict[str, Any]
    boundaries: dict[str, ProvinceBoundary]
    province_codes: tuple[str, ...]
    province_names: tuple[str, ...]
    province_index_by_code: dict[str, int]
    warehouses: dict[str, Warehouse]
    centers: tuple[str, ...]
    forwards: tuple[str, ...]
    forward_index_by_id: dict[str, int]
    links: dict[str, str]
    candidates: dict[str, Candidate]
    linehaul_quotes: dict[tuple[str, str], int]
    last_mile_quotes: dict[tuple[str, str], int]
    quote_row_count: int
    customers: CustomerTable
    forward_loads: dict[str, int]
    forward_province_loads: dict[tuple[str, str], int]
    center_loads: dict[str, int]


@dataclass(frozen=True)
class PreparedIndonesiaNetworkMap:
    release: DatasetReleaseBinding
    baseline_resource_name: str
    candidate_resource_name: str
    title: str
    summary: str
    geojson: dict[str, Any]
    layers: list[dict[str, Any]]
    extensions: dict[str, Any]

    def to_resource(
        self,
        *,
        geojson_resource_name: str,
        geojson_ref: IndonesiaGeoJsonRef,
    ) -> IndonesiaNetworkMap:
        features = self.geojson.get("features")
        if self.geojson.get("type") != "FeatureCollection" or not isinstance(features, list):
            raise ValueError("Prepared map is not a GeoJSON FeatureCollection")
        return IndonesiaNetworkMap(
            release=self.release,
            baseline_resource_name=self.baseline_resource_name,
            candidate_resource_name=self.candidate_resource_name,
            title=self.title,
            summary=self.summary,
            geojson_resource_name=geojson_resource_name,
            geojson_ref=geojson_ref,
            feature_count=len(features),
            layers=self.layers,
            extensions=self.extensions,
        )


def inspect_dataset_release(
    release: WorkspaceDatasetRelease,
) -> IndonesiaDatasetInspection:
    data = load_network_data(release, validate_customer_geometry=True)
    manifest_entries = {item["path"]: item for item in data.internal_manifest["files"]}
    files = [
        DatasetFileInspection(
            logical_name=name,
            role=file.role,
            media_type=file.media_type,
            byte_size=file.byte_size,
            content_sha256=file.content_sha256,
            row_count=(
                1 if name == "dataset-manifest.json" else int(manifest_entries[name]["row_count"])
            ),
        )
        for name, file in sorted(release.files.items())
    ]
    report = data.validation_report
    return IndonesiaDatasetInspection(
        release=release.binding,
        internal_content_sha256=data.internal_manifest["content_sha256"],
        data_classification=data.internal_manifest["data_classification"],
        customer_count=len(data.customers),
        annual_demand_units=sum(data.customers.demand_units),
        province_count=len(data.province_codes),
        central_warehouse_count=len(data.centers),
        forward_warehouse_count=len(data.forwards),
        candidate_location_count=len(data.candidates),
        quote_row_count=data.quote_row_count,
        files=files,
        policy=data.policy,
        source_attribution=data.internal_manifest["attribution"],
        checks=[
            "Platform Release identity, manifest, file sizes, and SHA-256 values match.",
            "Internal tutorial manifest and every declared domain file match.",
            "All customer and assignment rows are synchronized and aggregate correctly.",
            "Every customer coordinate is inside its declared current province boundary.",
            "Current assignment distance and service-day formulas recompute from policy.",
            "Three central warehouses, eight forward warehouses, twenty candidates, "
            "and complete current/candidate quote matrices are present.",
            f"Generator quality report has {len(report['checks'])} passing checks.",
        ],
    )


def load_network_data(
    release: WorkspaceDatasetRelease,
    *,
    validate_customer_geometry: bool,
) -> NetworkData:
    _validate_platform_files(release)
    internal_manifest = _load_json(release, "dataset-manifest.json")
    policy = _load_json(release, "planning-policy.json")
    validation_report = _load_json(release, "validation-report.json")
    _validate_internal_manifest(release, internal_manifest)
    _validate_policy_and_report(release, policy, validation_report)

    boundary_payload = _load_json(release, "province-boundaries.geojson")
    boundaries = _load_boundaries(boundary_payload)
    province_codes = tuple(sorted(boundaries))
    province_names = tuple(boundaries[code].name for code in province_codes)
    province_index_by_code = {code: index for index, code in enumerate(province_codes)}

    warehouse_rows = _read_csv(release, "warehouses.csv", WAREHOUSE_FIELDS)
    warehouses = _load_warehouses(warehouse_rows, boundaries)
    centers = tuple(
        sorted(
            warehouse_id
            for warehouse_id, warehouse in warehouses.items()
            if warehouse.warehouse_type == "central"
        )
    )
    forwards = tuple(
        sorted(
            warehouse_id
            for warehouse_id, warehouse in warehouses.items()
            if warehouse.warehouse_type == "forward"
        )
    )
    if len(centers) != 3 or len(forwards) != 8 or "CEN-BEKASI" not in centers:
        raise ValueError(
            "Tutorial network must contain three centers including Bekasi and eight forwards"
        )
    forward_index_by_id = {forward_id: index for index, forward_id in enumerate(forwards)}

    link_rows = _read_csv(release, "warehouse-links.csv", LINK_FIELDS)
    links = _load_links(link_rows, warehouses, centers, forwards)
    candidate_rows = _read_csv(
        release,
        "candidate-locations.csv",
        CANDIDATE_FIELDS,
    )
    candidates = _load_candidates(candidate_rows, boundaries, centers)
    if len(candidates) != 20:
        raise ValueError("Tutorial release must contain twenty candidate locations")

    quote_rows = _read_csv(release, "transport-quotes.csv", QUOTE_FIELDS)
    linehaul_quotes, last_mile_quotes = _load_quotes(
        quote_rows,
        centers=centers,
        forwards=forwards,
        candidates=tuple(sorted(candidates)),
        province_codes=province_codes,
    )
    road_factor = float(policy["distance"]["road_factor"])
    speed_kph = float(policy["time"]["average_speed_kph"])
    driver_hours = float(policy["time"]["driver_hours_per_day"])
    customers = _load_customers(
        release,
        boundaries=boundaries,
        province_codes=province_codes,
        province_names=province_names,
        province_index_by_code=province_index_by_code,
        warehouses=warehouses,
        forwards=forwards,
        forward_index_by_id=forward_index_by_id,
        road_factor=road_factor,
        speed_kph=speed_kph,
        driver_hours=driver_hours,
        validate_geometry=validate_customer_geometry,
    )
    forward_loads: dict[str, int] = {forward_id: 0 for forward_id in forwards}
    forward_province_loads: dict[tuple[str, str], int] = defaultdict(int)
    for index, demand in enumerate(customers.demand_units):
        forward_id = forwards[customers.forward_indexes[index]]
        province_code = province_codes[customers.province_indexes[index]]
        forward_loads[forward_id] += demand
        forward_province_loads[(forward_id, province_code)] += demand
    center_loads = {center_id: 0 for center_id in centers}
    for forward_id, demand in forward_loads.items():
        center_loads[links[forward_id]] += demand

    data = NetworkData(
        release=release,
        internal_manifest=internal_manifest,
        policy=policy,
        validation_report=validation_report,
        boundaries=boundaries,
        province_codes=province_codes,
        province_names=province_names,
        province_index_by_code=province_index_by_code,
        warehouses=warehouses,
        centers=centers,
        forwards=forwards,
        forward_index_by_id=forward_index_by_id,
        links=links,
        candidates=candidates,
        linehaul_quotes=linehaul_quotes,
        last_mile_quotes=last_mile_quotes,
        quote_row_count=len(quote_rows),
        customers=customers,
        forward_loads=forward_loads,
        forward_province_loads=dict(forward_province_loads),
        center_loads=center_loads,
    )
    _reconcile_generator_report(data)
    return data


def evaluate_service_baseline(data: NetworkData) -> IndonesiaServiceBaseline:
    coverage, provinces = _service_metrics(data, data.customers.current_service_days)
    priority, best = _rank_service_provinces(provinces)
    return IndonesiaServiceBaseline(
        release=data.release.binding,
        coverage=coverage,
        provinces=provinces,
        province_ranking_policy=ProvinceRankingPolicy(),
        priority_province_codes=[item.province_code for item in priority],
        best_province_codes=[item.province_code for item in best],
        assumptions=_analysis_assumptions(data),
        checks=[
            "All 240,000 customers were streamed once; no raw customer rows are returned.",
            "Coverage reconciles by customer count and annual demand units.",
            "Service uses each customer's current forward-warehouse assignment.",
            "This baseline excludes center-to-forward linehaul, capacity and all cost fields.",
        ],
    )


def evaluate_current_network(data: NetworkData) -> IndonesiaCurrentNetworkAnalysis:
    service = evaluate_service_baseline(data)
    coverage = service.coverage
    provinces = service.provinces
    costs = _cost_metrics(
        data,
        forward_loads=data.forward_loads,
        forward_province_loads=data.forward_province_loads,
    )
    warehouses = _warehouse_metrics(
        data,
        forward_loads=data.forward_loads,
        center_loads=data.center_loads,
    )
    links = [
        NetworkLink(
            origin_id=data.links[forward_id],
            destination_id=forward_id,
            leg_type="linehaul",
            demand_units=data.forward_loads[forward_id],
        )
        for forward_id in data.forwards
    ]
    return IndonesiaCurrentNetworkAnalysis(
        release=data.release.binding,
        coverage=coverage,
        costs=costs,
        provinces=provinces,
        warehouses=warehouses,
        links=links,
        province_ranking_policy=service.province_ranking_policy,
        priority_province_codes=service.priority_province_codes,
        best_province_codes=service.best_province_codes,
        assumptions=_analysis_assumptions(data),
        checks=[
            "All 240,000 customers were streamed once; no raw customer rows are returned.",
            "Coverage reconciles by customer count and annual demand units.",
            "Transport cost includes current center-to-forward and forward-to-customer demand.",
            "Province metrics reconcile to the same actual current assignments.",
        ],
    )


def _rank_service_provinces(
    provinces: list[ProvinceServiceMetric],
) -> tuple[list[ProvinceServiceMetric], list[ProvinceServiceMetric]]:
    active_provinces = [province for province in provinces if province.demand_units > 0]
    priority = sorted(
        active_provinces,
        key=lambda item: (
            item.demand_coverage["2_day"],
            -(item.demand_weighted_average_service_days or 0),
            item.province_code,
        ),
    )[:6]
    best = sorted(
        active_provinces,
        key=lambda item: (
            -(item.demand_coverage["2_day"]),
            item.demand_weighted_average_service_days or math.inf,
            item.province_code,
        ),
    )[:6]
    return priority, best


def evaluate_candidate_scenario(
    data: NetworkData,
    candidate_id: str,
    opening_amortization_years: int,
) -> IndonesiaCandidateScenario:
    try:
        candidate = data.candidates[candidate_id]
    except KeyError as error:
        raise ValueError(f"Unknown candidate location: {candidate_id}") from error

    candidate_days = array("B")
    candidate_distances = array("f")
    prospects: list[tuple[int, int, int, int]] = []
    road_factor = float(data.policy["distance"]["road_factor"])
    speed_kph = float(data.policy["time"]["average_speed_kph"])
    driver_hours = float(data.policy["time"]["driver_hours_per_day"])
    candidate_linehaul = data.linehaul_quotes[(candidate.parent_center_id, candidate.candidate_id)]
    for index, demand in enumerate(data.customers.demand_units):
        distance = (
            haversine_km(
                (candidate.longitude, candidate.latitude),
                (
                    data.customers.longitudes[index],
                    data.customers.latitudes[index],
                ),
            )
            * road_factor
        )
        days = _service_days(distance, speed_kph, driver_hours)
        candidate_distances.append(distance)
        candidate_days.append(days)
        current_days = data.customers.current_service_days[index]
        day_gain = current_days - days
        province_code = data.province_codes[data.customers.province_indexes[index]]
        current_forward = data.forwards[data.customers.forward_indexes[index]]
        current_unit_cost = (
            data.linehaul_quotes[(data.links[current_forward], current_forward)]
            + data.last_mile_quotes[(current_forward, province_code)]
        )
        candidate_unit_cost = (
            candidate_linehaul + data.last_mile_quotes[(candidate.candidate_id, province_code)]
        )
        unit_savings = current_unit_cost - candidate_unit_cost
        if day_gain > 0 or (day_gain == 0 and unit_savings > 0):
            distance_gain_meters = round(
                (data.customers.current_distances_km[index] - candidate_distances[index]) * 1000
            )
            prospects.append((-day_gain, -unit_savings, -distance_gain_meters, index))
    prospects.sort()

    selected = bytearray(len(data.customers))
    selected_demand = 0
    selected_customers = 0
    candidate_parent_load = data.center_loads[candidate.parent_center_id]
    parent_capacity = data.warehouses[candidate.parent_center_id].capacity_units
    for _, _, _, index in prospects:
        demand = data.customers.demand_units[index]
        if selected_demand + demand > candidate.capacity_units:
            continue
        current_forward = data.forwards[data.customers.forward_indexes[index]]
        current_parent = data.links[current_forward]
        if (
            current_parent != candidate.parent_center_id
            and candidate_parent_load + demand > parent_capacity
        ):
            continue
        selected[index] = 1
        selected_demand += demand
        selected_customers += 1
        if current_parent != candidate.parent_center_id:
            candidate_parent_load += demand

    scenario_days = array("B", data.customers.current_service_days)
    forward_loads = dict(data.forward_loads)
    forward_province_loads = defaultdict(int, data.forward_province_loads)
    candidate_province_loads: dict[str, int] = defaultdict(int)
    for index, is_selected in enumerate(selected):
        if not is_selected:
            continue
        demand = data.customers.demand_units[index]
        current_forward = data.forwards[data.customers.forward_indexes[index]]
        province_code = data.province_codes[data.customers.province_indexes[index]]
        scenario_days[index] = candidate_days[index]
        forward_loads[current_forward] -= demand
        forward_province_loads[(current_forward, province_code)] -= demand
        candidate_province_loads[province_code] += demand

    center_loads = {center_id: 0 for center_id in data.centers}
    for forward_id, demand in forward_loads.items():
        center_loads[data.links[forward_id]] += demand
    center_loads[candidate.parent_center_id] += selected_demand
    baseline_coverage, baseline_provinces = _service_metrics(
        data,
        data.customers.current_service_days,
    )
    candidate_coverage, candidate_provinces = _service_metrics(
        data,
        scenario_days,
    )
    baseline_costs = _cost_metrics(
        data,
        forward_loads=data.forward_loads,
        forward_province_loads=data.forward_province_loads,
    )
    candidate_costs = _cost_metrics(
        data,
        forward_loads=forward_loads,
        forward_province_loads=dict(forward_province_loads),
        candidate=candidate,
        candidate_demand=selected_demand,
        candidate_province_loads=dict(candidate_province_loads),
        opening_amortization_years=opening_amortization_years,
    )
    province_comparisons = [
        ProvinceServiceComparison(
            province_code=baseline.province_code,
            province_name=baseline.province_name,
            demand_units=baseline.demand_units,
            baseline_average_service_days=baseline.demand_weighted_average_service_days,
            candidate_average_service_days=scenario.demand_weighted_average_service_days,
            average_service_days_delta=_optional_delta(
                scenario.demand_weighted_average_service_days,
                baseline.demand_weighted_average_service_days,
            ),
            baseline_demand_coverage=baseline.demand_coverage,
            candidate_demand_coverage=scenario.demand_coverage,
            demand_centroid=baseline.demand_centroid,
        )
        for baseline, scenario in zip(
            baseline_provinces,
            candidate_provinces,
            strict=True,
        )
    ]
    warehouse_metrics = _warehouse_metrics(
        data,
        forward_loads=forward_loads,
        center_loads=center_loads,
        candidate=candidate,
        candidate_demand=selected_demand,
    )
    network_links = [
        NetworkLink(
            origin_id=data.links[forward_id],
            destination_id=forward_id,
            leg_type="linehaul",
            demand_units=forward_loads[forward_id],
        )
        for forward_id in data.forwards
    ]
    network_links.append(
        NetworkLink(
            origin_id=candidate.parent_center_id,
            destination_id=candidate.candidate_id,
            leg_type="candidate_linehaul",
            demand_units=selected_demand,
        )
    )
    if selected_demand > candidate.capacity_units:
        raise AssertionError("candidate assignment exceeded candidate capacity")
    if any(
        center_loads[center_id] > data.warehouses[center_id].capacity_units
        for center_id in data.centers
    ):
        raise AssertionError("candidate assignment exceeded central capacity")
    return IndonesiaCandidateScenario(
        release=data.release.binding,
        candidate=CandidateSummary(
            candidate_id=candidate.candidate_id,
            candidate_name=candidate.name,
            province_name=candidate.province_name,
            longitude=candidate.longitude,
            latitude=candidate.latitude,
            parent_center_id=candidate.parent_center_id,
            annual_capacity_units=candidate.capacity_units,
            selected_customer_count=selected_customers,
            selected_demand_units=selected_demand,
        ),
        opening_amortization_years=opening_amortization_years,
        baseline_coverage=baseline_coverage,
        candidate_coverage=candidate_coverage,
        baseline_costs=baseline_costs,
        candidate_costs=candidate_costs,
        demand_coverage_delta={
            key: candidate_coverage.demand_coverage[key] - baseline_coverage.demand_coverage[key]
            for key in ("1_day", "2_day", "3_day")
        },
        transport_cost_delta_idr=(
            candidate_costs.transport_total_idr - baseline_costs.transport_total_idr
        ),
        annual_decision_cost_delta_idr=(
            candidate_costs.annual_decision_cost_idr - baseline_costs.annual_decision_cost_idr
        ),
        provinces=province_comparisons,
        warehouses=warehouse_metrics,
        links=network_links,
        assignment_policy=(
            "Keep every actual current assignment unless the candidate reduces service "
            "days, or keeps the same service day with lower quoted transport cost. "
            "Accept whole customers in descending service-day gain, then unit-cost "
            "saving, then distance gain, subject to candidate and parent-center capacity."
        ),
        constraints=[
            f"Candidate annual capacity is {candidate.capacity_units} demand units.",
            (
                f"Candidate parent center is {candidate.parent_center_id}; "
                "its capacity remains binding."
            ),
            "Existing forward assignments not selected by the deterministic rule are unchanged.",
            (
                "The customer promise begins at a stocked forward warehouse; "
                "linehaul time is replenishment."
            ),
        ],
        checks=[
            "Actual baseline assignments were not re-optimized or relabeled.",
            "Selected customer demand reconciles to candidate and source warehouse loads.",
            "Candidate and all central warehouse capacities are respected.",
            "Scenario service and cost totals reconcile to province and warehouse projections.",
        ],
    )


def build_location_optimization(
    data: NetworkData,
    *,
    target_service_days: int,
    target_demand_coverage: float,
    opening_amortization_years: int,
) -> tuple[
    IndonesiaLocationOptimization,
    IndonesiaCandidateScenario | None,
    list[CandidateEvaluation],
]:
    baseline = evaluate_current_network(data)
    baseline_target = _coverage_at_days(
        data.customers.current_service_days,
        data.customers.demand_units,
        target_service_days,
    )
    if baseline_target + 1e-12 >= target_demand_coverage:
        optimization = IndonesiaLocationOptimization(
            release=data.release.binding,
            target_service_days=target_service_days,
            target_demand_coverage=target_demand_coverage,
            status="target_already_met",
            selected_candidate_id=None,
            evaluated_candidate_count=0,
            target_met_candidate_count=0,
            evaluations=[],
            selected_scenario_resource_name=None,
            selected_scenario_ref=None,
            assumptions=[
                f"The actual current network already covers {baseline_target:.6f} of demand "
                f"within {target_service_days} day(s), so opening a new warehouse is not "
                "required solely to satisfy this target.",
                *baseline.assumptions,
            ],
        )
        return optimization, None, []

    evaluated: list[tuple[CandidateEvaluation, IndonesiaCandidateScenario]] = []
    for candidate_id in sorted(data.candidates):
        scenario = evaluate_candidate_scenario(
            data,
            candidate_id,
            opening_amortization_years,
        )
        coverage = _scenario_coverage_for_days(
            scenario,
            target_service_days,
            data,
        )
        evaluation = CandidateEvaluation(
            candidate_id=candidate_id,
            candidate_name=scenario.candidate.candidate_name,
            selected_demand_units=scenario.candidate.selected_demand_units,
            target_demand_coverage=coverage,
            target_met=coverage + 1e-12 >= target_demand_coverage,
            transport_total_idr=scenario.candidate_costs.transport_total_idr,
            annual_decision_cost_idr=(scenario.candidate_costs.annual_decision_cost_idr),
            demand_coverage_delta=coverage - baseline_target,
        )
        evaluated.append((evaluation, scenario))

    feasible = [item for item in evaluated if item[0].target_met]
    if feasible:
        selected_evaluation, selected_scenario = min(
            feasible,
            key=lambda item: (
                item[0].annual_decision_cost_idr,
                item[0].transport_total_idr,
                item[0].candidate_id,
            ),
        )
        status = "target_met"
    else:
        selected_evaluation, selected_scenario = min(
            evaluated,
            key=lambda item: (
                -item[0].target_demand_coverage,
                item[0].annual_decision_cost_idr,
                item[0].candidate_id,
            ),
        )
        status = "best_available"
    optimization = IndonesiaLocationOptimization(
        release=data.release.binding,
        target_service_days=target_service_days,
        target_demand_coverage=target_demand_coverage,
        status=status,
        selected_candidate_id=selected_evaluation.candidate_id,
        evaluated_candidate_count=len(evaluated),
        target_met_candidate_count=len(feasible),
        evaluations=[item[0] for item in evaluated],
        selected_scenario_resource_name=None,
        selected_scenario_ref=None,
        assumptions=[
            "Every one of the twenty reviewed candidate locations is evaluated under the "
            "same deterministic reallocation, capacity, distance, time, and quote policy.",
            "The selected site is exact only over this finite candidate set; it is not a "
            "continuous geographic optimum.",
            "Among candidates meeting the requested service target, annual transport plus "
            "fixed cost plus amortized opening cost is minimized.",
            "If no candidate meets the target, the highest achievable demand coverage is "
            "selected first and annual decision cost breaks ties.",
        ],
    )
    return optimization, selected_scenario, [item[0] for item in evaluated]


def prepare_network_map(
    baseline: IndonesiaCurrentNetworkAnalysis,
    scenario: IndonesiaCandidateScenario,
    *,
    baseline_resource_name: str,
    candidate_resource_name: str,
) -> PreparedIndonesiaNetworkMap:
    if baseline.release != scenario.release:
        raise ValueError("Map inputs must use the same Dataset Release")
    features: list[dict[str, Any]] = []
    locations = {
        warehouse.warehouse_id: (warehouse.longitude, warehouse.latitude)
        for warehouse in scenario.warehouses
    }
    for warehouse in scenario.warehouses:
        features.append(
            {
                "type": "Feature",
                "properties": {
                    "feature_type": "warehouse",
                    "warehouse_id": warehouse.warehouse_id,
                    "name": warehouse.warehouse_name,
                    "warehouse_type": warehouse.warehouse_type,
                    "province": warehouse.province_name,
                    "assigned_demand_units": warehouse.assigned_demand_units,
                    "utilization_ratio": round(warehouse.utilization_ratio, 6),
                },
                "geometry": {
                    "type": "Point",
                    "coordinates": [warehouse.longitude, warehouse.latitude],
                },
            }
        )
    for link in scenario.links:
        features.append(
            {
                "type": "Feature",
                "properties": {
                    "feature_type": "network_link",
                    "leg_type": link.leg_type,
                    "origin_id": link.origin_id,
                    "destination_id": link.destination_id,
                    "demand_units": link.demand_units,
                },
                "geometry": {
                    "type": "LineString",
                    "coordinates": [
                        list(locations[link.origin_id]),
                        list(locations[link.destination_id]),
                    ],
                },
            }
        )
    for province in scenario.provinces:
        if province.demand_centroid is None:
            continue
        longitude, latitude = province.demand_centroid
        features.append(
            {
                "type": "Feature",
                "properties": {
                    "feature_type": "province_service",
                    "province_code": province.province_code,
                    "province_name": province.province_name,
                    "demand_units": province.demand_units,
                    "baseline_average_days": province.baseline_average_service_days,
                    "candidate_average_days": province.candidate_average_service_days,
                    "average_days_delta": province.average_service_days_delta,
                    "baseline_2_day_coverage": province.baseline_demand_coverage["2_day"],
                    "candidate_2_day_coverage": province.candidate_demand_coverage["2_day"],
                },
                "geometry": {
                    "type": "Point",
                    "coordinates": [longitude, latitude],
                },
            }
        )
    layers: list[dict[str, Any]] = [
        {
            "id": "network-links",
            "type": "line",
            "source": "network",
            "filter": ["==", ["get", "feature_type"], "network_link"],
            "paint": {
                "line-color": [
                    "match",
                    ["get", "leg_type"],
                    "candidate_linehaul",
                    "#e76f51",
                    "#8a817c",
                ],
                "line-width": [
                    "match",
                    ["get", "leg_type"],
                    "candidate_linehaul",
                    3,
                    1.5,
                ],
                "line-opacity": 0.72,
            },
        },
        {
            "id": "province-service",
            "type": "circle",
            "source": "network",
            "filter": ["==", ["get", "feature_type"], "province_service"],
            "paint": {
                "circle-radius": [
                    "interpolate",
                    ["linear"],
                    ["sqrt", ["get", "demand_units"]],
                    0,
                    3,
                    1100,
                    13,
                ],
                "circle-color": [
                    "step",
                    ["coalesce", ["get", "average_days_delta"], 0],
                    "#2a9d8f",
                    -0.5,
                    "#007f5f",
                    -0.01,
                    "#74c69d",
                    0,
                    "#adb5bd",
                ],
                "circle-opacity": 0.7,
                "circle-stroke-color": "#ffffff",
                "circle-stroke-width": 1,
            },
        },
        {
            "id": "existing-warehouses",
            "type": "circle",
            "source": "network",
            "filter": [
                "all",
                ["==", ["get", "feature_type"], "warehouse"],
                ["!=", ["get", "warehouse_type"], "candidate"],
            ],
            "paint": {
                "circle-radius": [
                    "match",
                    ["get", "warehouse_type"],
                    "central",
                    8,
                    5,
                ],
                "circle-color": [
                    "match",
                    ["get", "warehouse_type"],
                    "central",
                    "#264653",
                    "#f4a261",
                ],
                "circle-stroke-color": "#ffffff",
                "circle-stroke-width": 1.5,
            },
        },
        {
            "id": "candidate-warehouse",
            "type": "circle",
            "source": "network",
            "filter": [
                "all",
                ["==", ["get", "feature_type"], "warehouse"],
                ["==", ["get", "warehouse_type"], "candidate"],
            ],
            "paint": {
                "circle-radius": 9,
                "circle-color": "#e63946",
                "circle-stroke-color": "#ffffff",
                "circle-stroke-width": 2,
            },
        },
    ]
    extensions = {
        "hover": {
            "layers": [
                {
                    "layer": "province-service",
                    "title_property": "province_name",
                    "fields": [
                        "demand_units",
                        "baseline_average_days",
                        "candidate_average_days",
                        "average_days_delta",
                        "baseline_2_day_coverage",
                        "candidate_2_day_coverage",
                    ],
                },
                {
                    "layer": "existing-warehouses",
                    "title_property": "name",
                    "fields": [
                        "warehouse_type",
                        "assigned_demand_units",
                        "utilization_ratio",
                    ],
                },
                {
                    "layer": "candidate-warehouse",
                    "title_property": "name",
                    "fields": [
                        "assigned_demand_units",
                        "utilization_ratio",
                    ],
                },
            ]
        },
        "legend": {
            "items": [
                {"label": "Central warehouse", "color": "#264653", "type": "circle"},
                {"label": "Forward warehouse", "color": "#f4a261", "type": "circle"},
                {"label": "Candidate warehouse", "color": "#e63946", "type": "circle"},
                {"label": "Improved province", "color": "#2a9d8f", "type": "circle"},
                {"label": "Current linehaul", "color": "#8a817c", "type": "line"},
                {"label": "Candidate linehaul", "color": "#e76f51", "type": "line"},
            ]
        },
    }
    if not features or len(features) > 200:
        raise ValueError("Prepared map must contain between 1 and 200 features")
    return PreparedIndonesiaNetworkMap(
        release=baseline.release,
        baseline_resource_name=baseline_resource_name,
        candidate_resource_name=candidate_resource_name,
        title=(f"Indonesia network: current vs {scenario.candidate.candidate_name}"),
        summary=(
            f"The candidate takes {scenario.candidate.selected_demand_units:,} annual "
            f"demand units. Two-day demand coverage changes by "
            f"{scenario.demand_coverage_delta['2_day']:.2%}; annual transport cost "
            f"changes by IDR {scenario.transport_cost_delta_idr:,}."
        ),
        geojson={"type": "FeatureCollection", "features": features},
        layers=layers,
        extensions=extensions,
    )


def validate_indonesia_resource(payload: dict[str, Any]) -> IndonesiaValidationResult:
    schema = payload.get("schema_version")
    model_by_schema = {
        "indonesia_dataset_inspection.v1": IndonesiaDatasetInspection,
        "indonesia_service_baseline.v1": IndonesiaServiceBaseline,
        "indonesia_current_network_analysis.v1": IndonesiaCurrentNetworkAnalysis,
        "indonesia_candidate_scenario.v1": IndonesiaCandidateScenario,
        "indonesia_location_optimization.v1": IndonesiaLocationOptimization,
        "indonesia_network_map.v1": IndonesiaNetworkMap,
    }
    model_type = model_by_schema.get(schema)
    if model_type is None:
        return IndonesiaValidationResult(
            resource_schema=str(schema or "unknown"),
            valid=False,
            errors=["Unsupported Indonesia tutorial Resource schema."],
        )
    try:
        value = model_type.model_validate(payload)
    except ValidationError as error:
        return IndonesiaValidationResult(
            resource_schema=str(schema),
            valid=False,
            errors=[
                f"{'.'.join(str(part) for part in item['loc'])}: {item['msg']}"
                for item in error.errors()
            ],
        )

    checks = [f"Resource conforms to {schema}."]
    errors: list[str] = []
    if isinstance(value, IndonesiaServiceBaseline):
        if sum(item.demand_units for item in value.provinces) != (
            value.coverage.total_demand_units
        ):
            errors.append("Service-baseline province demand does not reconcile.")
        errors.extend(_service_ranking_errors(value))
        checks.append("Service-baseline province totals and rankings reconcile.")
    elif isinstance(value, IndonesiaCurrentNetworkAnalysis):
        if value.costs.linehaul_idr + value.costs.last_mile_idr != value.costs.transport_total_idr:
            errors.append("Current transport cost components do not reconcile.")
        if sum(item.demand_units for item in value.provinces) != (
            value.coverage.total_demand_units
        ):
            errors.append("Current province demand does not reconcile.")
        errors.extend(_service_ranking_errors(value))
        checks.append("Current cost, province totals and rankings reconcile.")
    elif isinstance(value, IndonesiaCandidateScenario):
        expected_delta = (
            value.candidate_costs.transport_total_idr - value.baseline_costs.transport_total_idr
        )
        if expected_delta != value.transport_cost_delta_idr:
            errors.append("Candidate transport cost delta is inconsistent.")
        if value.candidate.selected_demand_units > value.candidate.annual_capacity_units:
            errors.append("Candidate demand exceeds capacity.")
        checks.append("Candidate deltas and capacity reconcile.")
    elif isinstance(value, IndonesiaLocationOptimization):
        if value.evaluated_candidate_count != len(value.evaluations):
            errors.append("Optimization evaluated count is inconsistent.")
        actual_target_met_count = sum(
            evaluation.target_met for evaluation in value.evaluations
        )
        if value.target_met_candidate_count != actual_target_met_count:
            errors.append("Optimization target-met count is inconsistent.")
        if value.status == "target_already_met" and (
            value.selected_candidate_id is not None
            or value.selected_scenario_ref is not None
            or value.evaluated_candidate_count != 0
            or value.target_met_candidate_count != 0
        ):
            errors.append("No candidate may be selected when the target already holds.")
        if value.status == "target_met" and value.target_met_candidate_count == 0:
            errors.append("Target-met status requires at least one qualifying candidate.")
        if value.status == "best_available" and value.target_met_candidate_count != 0:
            errors.append("Best-available status cannot contain a qualifying candidate.")
        selected = next(
            (
                evaluation
                for evaluation in value.evaluations
                if evaluation.candidate_id == value.selected_candidate_id
            ),
            None,
        )
        if value.selected_candidate_id is not None and selected is None:
            errors.append("Selected candidate is absent from finite evaluations.")
        if value.status == "target_met" and selected is not None and not selected.target_met:
            errors.append("Selected target-met candidate does not meet the target.")
        checks.append(
            "Optimization status, finite evaluation count, qualifying count and selection reconcile."
        )
    elif isinstance(value, IndonesiaNetworkMap):
        if value.feature_count > 200:
            errors.append("Map GeoJSON exceeds the bounded feature count.")
        checks.append("Map manifest references a bounded GeoJSON Resource.")
    elif isinstance(value, IndonesiaDatasetInspection):
        if value.customer_count != 240_000 or value.province_count != 38:
            errors.append("Inspection does not match the tutorial release scale.")
        checks.append("Inspection reports the expected tutorial scale.")
    return IndonesiaValidationResult(
        resource_schema=str(schema),
        valid=not errors,
        checks=checks,
        errors=errors,
    )


def _service_ranking_errors(
    value: IndonesiaServiceBaseline | IndonesiaCurrentNetworkAnalysis,
) -> list[str]:
    priority, best = _rank_service_provinces(value.provinces)
    errors = []
    if value.priority_province_codes != [item.province_code for item in priority]:
        errors.append("Priority province ranking is inconsistent with its declared policy.")
    if value.best_province_codes != [item.province_code for item in best]:
        errors.append("Best province ranking is inconsistent with its declared policy.")
    return errors


def require_valid_indonesia_resource(payload: dict[str, Any]) -> IndonesiaValidationResult:
    validation = validate_indonesia_resource(payload)
    if not validation.valid:
        details = "; ".join(validation.errors) or "unknown validation failure"
        raise ValueError(
            f"{validation.resource_schema} failed deterministic validation: {details}"
        )
    return validation


def _validate_platform_files(release: WorkspaceDatasetRelease) -> None:
    if set(release.files) != set(EXPECTED_FILES):
        missing = sorted(set(EXPECTED_FILES) - set(release.files))
        extra = sorted(set(release.files) - set(EXPECTED_FILES))
        raise ValueError(f"Dataset Release file contract differs; missing={missing}, extra={extra}")
    for name, (role, media_type) in EXPECTED_FILES.items():
        file = release.files[name]
        if file.role != role or file.media_type != media_type:
            raise ValueError(
                f"Dataset file {name} must use role {role} and media type {media_type}"
            )


def _validate_internal_manifest(
    release: WorkspaceDatasetRelease,
    manifest: dict[str, Any],
) -> None:
    if (
        manifest.get("schema_version") != "workspace_dataset_release.v1"
        or manifest.get("dataset_id") != release.binding.dataset_id
        or manifest.get("version") != release.binding.version
        or manifest.get("data_classification") != "synthetic_tutorial_data"
        or manifest.get("customer_count") != 240_000
    ):
        raise ValueError("Internal tutorial manifest identity is invalid")
    entries = manifest.get("files")
    if not isinstance(entries, list) or len(entries) != len(EXPECTED_FILES) - 1:
        raise ValueError("Internal tutorial file declaration is invalid")
    seen: set[str] = set()
    for entry in entries:
        if not isinstance(entry, dict):
            raise ValueError("Internal tutorial file declaration is invalid")
        name = entry.get("path")
        if (
            not isinstance(name, str)
            or name == "dataset-manifest.json"
            or name in seen
            or name not in release.files
            or entry.get("role") != release.files[name].role
            or entry.get("bytes") != release.files[name].byte_size
            or entry.get("sha256") != release.files[name].content_sha256
            or not isinstance(entry.get("row_count"), int)
            or entry["row_count"] <= 0
        ):
            raise ValueError("Internal tutorial file declaration does not match")
        seen.add(name)
    expected_identity = hashlib.sha256(
        json.dumps(entries, sort_keys=True, separators=(",", ":")).encode("utf-8")
    ).hexdigest()
    if manifest.get("content_sha256") != expected_identity:
        raise ValueError("Internal tutorial content SHA-256 is invalid")
    attribution = manifest.get("attribution")
    if not isinstance(attribution, list) or not all(
        isinstance(item, str) and item for item in attribution
    ):
        raise ValueError("Tutorial source attribution is missing")


def _validate_policy_and_report(
    release: WorkspaceDatasetRelease,
    policy: dict[str, Any],
    report: dict[str, Any],
) -> None:
    if (
        policy.get("schema_version") != "indonesia_network_planning_policy.v1"
        or policy.get("dataset_id") != release.binding.dataset_id
        or policy.get("release_version") != release.binding.version
        or policy.get("currency") != "IDR"
        or policy.get("distance", {}).get("method") != "haversine_factor"
        or policy.get("distance", {}).get("navigation_api_used") is not False
        or float(policy.get("distance", {}).get("road_factor", 0)) <= 0
        or float(policy.get("time", {}).get("average_speed_kph", 0)) <= 0
        or float(policy.get("time", {}).get("driver_hours_per_day", 0)) <= 0
    ):
        raise ValueError("Indonesia planning policy is invalid")
    checks = report.get("checks")
    if (
        report.get("schema_version") != "indonesia_tutorial_validation_report.v1"
        or report.get("dataset_id") != release.binding.dataset_id
        or report.get("release_version") != release.binding.version
        or report.get("all_passed") is not True
        or not isinstance(checks, dict)
        or not checks
        or not all(value is True for value in checks.values())
    ):
        raise ValueError("Tutorial generator validation report is not fully passing")


def _load_boundaries(payload: dict[str, Any]) -> dict[str, ProvinceBoundary]:
    features = payload.get("features")
    if payload.get("type") != "FeatureCollection" or not isinstance(features, list):
        raise ValueError("Province boundary file is not a GeoJSON FeatureCollection")
    boundaries: dict[str, ProvinceBoundary] = {}
    for feature in features:
        boundary = ProvinceBoundary.from_feature(
            feature,
            code_field="province_code",
            name_field="province_name",
        )
        if boundary.code in boundaries:
            raise ValueError(f"Duplicate province boundary: {boundary.code}")
        boundaries[boundary.code] = boundary
    if len(boundaries) != 38:
        raise ValueError("Province boundary file must contain 38 current provinces")
    return boundaries


def _load_warehouses(
    rows: list[dict[str, str]],
    boundaries: dict[str, ProvinceBoundary],
) -> dict[str, Warehouse]:
    warehouses: dict[str, Warehouse] = {}
    for row in rows:
        warehouse_id = row["warehouse_id"]
        warehouse_type = row["warehouse_type"]
        if warehouse_id in warehouses or warehouse_type not in {"central", "forward"}:
            raise ValueError("Warehouse identifiers or types are invalid")
        province_code = row["province_code"]
        latitude = float(row["latitude"])
        longitude = float(row["longitude"])
        boundary = boundaries.get(province_code)
        if (
            boundary is None
            or boundary.name != row["province_name"]
            or not boundary.contains((longitude, latitude))
        ):
            raise ValueError(f"Warehouse {warehouse_id} is outside its province")
        parent = row["current_parent_center_id"] or None
        warehouses[warehouse_id] = Warehouse(
            warehouse_id=warehouse_id,
            name=row["warehouse_name"],
            warehouse_type=warehouse_type,
            province_code=province_code,
            province_name=row["province_name"],
            latitude=latitude,
            longitude=longitude,
            parent_center_id=parent,
            capacity_units=int(row["annual_capacity_units"]),
        )
    return warehouses


def _load_links(
    rows: list[dict[str, str]],
    warehouses: dict[str, Warehouse],
    centers: tuple[str, ...],
    forwards: tuple[str, ...],
) -> dict[str, str]:
    links: dict[str, str] = {}
    for row in rows:
        forward_id = row["forward_warehouse_id"]
        center_id = row["current_center_warehouse_id"]
        if (
            forward_id in links
            or forward_id not in forwards
            or center_id not in centers
            or warehouses[forward_id].parent_center_id != center_id
        ):
            raise ValueError("Warehouse link contract is invalid")
        links[forward_id] = center_id
    if set(links) != set(forwards):
        raise ValueError("Every forward warehouse needs one current center link")
    return links


def _load_candidates(
    rows: list[dict[str, str]],
    boundaries: dict[str, ProvinceBoundary],
    centers: tuple[str, ...],
) -> dict[str, Candidate]:
    candidates: dict[str, Candidate] = {}
    for row in rows:
        candidate_id = row["candidate_id"]
        province_code = row["province_code"]
        latitude = float(row["latitude"])
        longitude = float(row["longitude"])
        parent_center = row["proposed_parent_center_id"]
        boundary = boundaries.get(province_code)
        if (
            candidate_id in candidates
            or boundary is None
            or boundary.name != row["province_name"]
            or not boundary.contains((longitude, latitude))
            or parent_center not in centers
        ):
            raise ValueError("Candidate location contract is invalid")
        candidates[candidate_id] = Candidate(
            candidate_id=candidate_id,
            name=row["candidate_name"],
            province_code=province_code,
            province_name=row["province_name"],
            latitude=latitude,
            longitude=longitude,
            capacity_units=int(row["annual_capacity_units"]),
            opening_cost_idr=int(row["opening_cost_idr"]),
            annual_fixed_cost_idr=int(row["annual_fixed_cost_idr"]),
            parent_center_id=parent_center,
        )
    return candidates


def _load_quotes(
    rows: list[dict[str, str]],
    *,
    centers: tuple[str, ...],
    forwards: tuple[str, ...],
    candidates: tuple[str, ...],
    province_codes: tuple[str, ...],
) -> tuple[dict[tuple[str, str], int], dict[tuple[str, str], int]]:
    linehaul: dict[tuple[str, str], int] = {}
    last_mile: dict[tuple[str, str], int] = {}
    quote_ids: set[str] = set()
    for row in rows:
        quote_id = row["quote_id"]
        if quote_id in quote_ids:
            raise ValueError(f"Duplicate transport quote: {quote_id}")
        quote_ids.add(quote_id)
        value = int(row["quoted_idr_per_demand_unit"])
        if value <= 0:
            raise ValueError("Transport quote values must be positive")
        key = (row["origin_warehouse_id"], row["destination_id"])
        if row["leg_type"] == "linehaul":
            if key in linehaul:
                raise ValueError("Duplicate linehaul quote selector")
            linehaul[key] = value
        elif row["leg_type"] == "last_mile":
            if key in last_mile:
                raise ValueError("Duplicate last-mile quote selector")
            last_mile[key] = value
        else:
            raise ValueError("Transport quote leg type is invalid")
    expected_linehaul = {
        (center_id, destination_id)
        for center_id in centers
        for destination_id in (*forwards, *candidates)
    }
    expected_last_mile = {
        (origin_id, province_code)
        for origin_id in (*forwards, *candidates)
        for province_code in province_codes
    }
    if set(linehaul) != expected_linehaul or set(last_mile) != expected_last_mile:
        raise ValueError("Current and candidate transport quote matrices are incomplete")
    return linehaul, last_mile


def _load_customers(
    release: WorkspaceDatasetRelease,
    *,
    boundaries: dict[str, ProvinceBoundary],
    province_codes: tuple[str, ...],
    province_names: tuple[str, ...],
    province_index_by_code: dict[str, int],
    warehouses: dict[str, Warehouse],
    forwards: tuple[str, ...],
    forward_index_by_id: dict[str, int],
    road_factor: float,
    speed_kph: float,
    driver_hours: float,
    validate_geometry: bool,
) -> CustomerTable:
    province_indexes = array("B")
    latitudes = array("d")
    longitudes = array("d")
    demand_units = array("I")
    forward_indexes = array("B")
    current_distances = array("f")
    current_days = array("B")
    province_customer_counts = [0] * len(province_codes)
    province_demand_units = [0] * len(province_codes)
    weighted_longitudes = [0.0] * len(province_codes)
    weighted_latitudes = [0.0] * len(province_codes)

    customer_file = release.require_file("customers.csv.gz")
    assignment_file = release.require_file("customer-assignments.csv.gz")
    with (
        gzip.open(
            customer_file.path,
            "rt",
            encoding="utf-8",
            newline="",
        ) as customer_source,
        gzip.open(
            assignment_file.path,
            "rt",
            encoding="utf-8",
            newline="",
        ) as assignment_source,
    ):
        customer_reader = csv.DictReader(customer_source)
        assignment_reader = csv.DictReader(assignment_source)
        if customer_reader.fieldnames != CUSTOMER_FIELDS:
            raise ValueError("Customer CSV header does not match the tutorial contract")
        if assignment_reader.fieldnames != ASSIGNMENT_FIELDS:
            raise ValueError("Assignment CSV header does not match the tutorial contract")
        for row_number, (customer, assignment) in enumerate(
            zip_longest(customer_reader, assignment_reader),
            start=1,
        ):
            if customer is None or assignment is None:
                raise ValueError("Customer and assignment row counts differ")
            customer_id = customer["customer_id"]
            if customer_id != assignment["customer_id"]:
                raise ValueError(f"Customer assignment mismatch at row {row_number}")
            province_code = customer["province_code"]
            province_index = province_index_by_code.get(province_code)
            if (
                province_index is None
                or customer["province_name"] != province_names[province_index]
            ):
                raise ValueError(f"Customer province is invalid at row {row_number}")
            latitude = float(customer["latitude"])
            longitude = float(customer["longitude"])
            if validate_geometry and not boundaries[province_code].contains((longitude, latitude)):
                raise ValueError(f"Customer is outside its province at row {row_number}")
            demand = int(customer["annual_demand_units"])
            if demand <= 0:
                raise ValueError(f"Customer demand is invalid at row {row_number}")
            forward_id = assignment["current_forward_id"]
            forward_index = forward_index_by_id.get(forward_id)
            if forward_index is None:
                raise ValueError(f"Current forward is invalid at row {row_number}")
            distance = float(assignment["estimated_distance_km"])
            days = int(assignment["service_days"])
            if days != _service_days(distance, speed_kph, driver_hours):
                raise ValueError(f"Service-day formula differs at row {row_number}")
            if validate_geometry:
                warehouse = warehouses[forward_id]
                expected_distance = (
                    haversine_km(
                        (warehouse.longitude, warehouse.latitude),
                        (longitude, latitude),
                    )
                    * road_factor
                )
                if abs(expected_distance - distance) > 0.01:
                    raise ValueError(f"Assignment distance differs at row {row_number}")
            province_indexes.append(province_index)
            latitudes.append(latitude)
            longitudes.append(longitude)
            demand_units.append(demand)
            forward_indexes.append(forward_index)
            current_distances.append(distance)
            current_days.append(days)
            province_customer_counts[province_index] += 1
            province_demand_units[province_index] += demand
            weighted_longitudes[province_index] += longitude * demand
            weighted_latitudes[province_index] += latitude * demand
    if len(demand_units) != 240_000:
        raise ValueError("Tutorial release must contain exactly 240,000 customers")
    return CustomerTable(
        province_indexes=province_indexes,
        latitudes=latitudes,
        longitudes=longitudes,
        demand_units=demand_units,
        forward_indexes=forward_indexes,
        current_distances_km=current_distances,
        current_service_days=current_days,
        province_customer_counts=tuple(province_customer_counts),
        province_demand_units=tuple(province_demand_units),
        province_weighted_longitudes=tuple(weighted_longitudes),
        province_weighted_latitudes=tuple(weighted_latitudes),
    )


def _reconcile_generator_report(data: NetworkData) -> None:
    report = data.validation_report
    total_demand = sum(data.customers.demand_units)
    if report["demand"]["customer_count"] != len(data.customers):
        raise ValueError("Generator customer count does not reconcile")
    if report["demand"]["annual_demand_units"] != total_demand:
        raise ValueError("Generator demand total does not reconcile")
    if report["quotes"]["row_count"] != data.quote_row_count:
        raise ValueError("Generator quote count does not reconcile")
    current = evaluate_current_network(data)
    expected_coverage = report["network"]["coverage_by_demand_units"]
    for key in ("1_day", "2_day", "3_day"):
        if abs(current.coverage.demand_coverage[key] - expected_coverage[key]) > 1e-12:
            raise ValueError("Generator coverage metrics do not reconcile")
    expected_cost = report["network"]["current_cost_idr"]
    if (
        current.costs.linehaul_idr != expected_cost["linehaul"]
        or current.costs.last_mile_idr != expected_cost["last_mile"]
        or current.costs.transport_total_idr != expected_cost["total"]
    ):
        raise ValueError("Generator current cost metrics do not reconcile")


def _service_metrics(
    data: NetworkData,
    service_days: array,
) -> tuple[CoverageMetrics, list[ProvinceServiceMetric]]:
    if len(service_days) != len(data.customers):
        raise ValueError("Scenario service-day vector has the wrong length")
    customer_coverage = {day: 0 for day in (1, 2, 3)}
    demand_coverage = {day: 0 for day in (1, 2, 3)}
    province_weighted_days = [0] * len(data.province_codes)
    province_max_days = [0] * len(data.province_codes)
    province_coverage = [{day: 0 for day in (1, 2, 3)} for _ in data.province_codes]
    for index, days in enumerate(service_days):
        demand = data.customers.demand_units[index]
        province_index = data.customers.province_indexes[index]
        province_weighted_days[province_index] += days * demand
        province_max_days[province_index] = max(
            province_max_days[province_index],
            days,
        )
        for threshold in (1, 2, 3):
            if days <= threshold:
                customer_coverage[threshold] += 1
                demand_coverage[threshold] += demand
                province_coverage[province_index][threshold] += demand
    total_customers = len(data.customers)
    total_demand = sum(data.customers.demand_units)
    coverage = CoverageMetrics(
        total_customers=total_customers,
        total_demand_units=total_demand,
        customer_coverage={
            f"{day}_day": customer_coverage[day] / total_customers for day in (1, 2, 3)
        },
        demand_coverage={f"{day}_day": demand_coverage[day] / total_demand for day in (1, 2, 3)},
    )
    provinces: list[ProvinceServiceMetric] = []
    for province_index, province_code in enumerate(data.province_codes):
        demand = data.customers.province_demand_units[province_index]
        centroid = (
            (
                data.customers.province_weighted_longitudes[province_index] / demand,
                data.customers.province_weighted_latitudes[province_index] / demand,
            )
            if demand
            else None
        )
        provinces.append(
            ProvinceServiceMetric(
                province_code=province_code,
                province_name=data.province_names[province_index],
                customer_count=(data.customers.province_customer_counts[province_index]),
                demand_units=demand,
                demand_weighted_average_service_days=(
                    province_weighted_days[province_index] / demand if demand else None
                ),
                maximum_service_days=(province_max_days[province_index] if demand else None),
                demand_coverage={
                    f"{day}_day": (
                        province_coverage[province_index][day] / demand if demand else 0.0
                    )
                    for day in (1, 2, 3)
                },
                demand_centroid=centroid,
            )
        )
    return coverage, provinces


def _cost_metrics(
    data: NetworkData,
    *,
    forward_loads: dict[str, int],
    forward_province_loads: dict[tuple[str, str], int],
    candidate: Candidate | None = None,
    candidate_demand: int = 0,
    candidate_province_loads: dict[str, int] | None = None,
    opening_amortization_years: int = 1,
) -> CostMetrics:
    linehaul = sum(
        demand * data.linehaul_quotes[(data.links[forward_id], forward_id)]
        for forward_id, demand in forward_loads.items()
    )
    last_mile = sum(
        demand * data.last_mile_quotes[(forward_id, province_code)]
        for (forward_id, province_code), demand in forward_province_loads.items()
        if demand
    )
    opening = 0
    fixed = 0
    annualized_opening = 0
    if candidate is not None:
        linehaul += (
            candidate_demand
            * data.linehaul_quotes[(candidate.parent_center_id, candidate.candidate_id)]
        )
        last_mile += sum(
            demand * data.last_mile_quotes[(candidate.candidate_id, province_code)]
            for province_code, demand in (candidate_province_loads or {}).items()
            if demand
        )
        opening = candidate.opening_cost_idr
        fixed = candidate.annual_fixed_cost_idr
        annualized_opening = round(opening / opening_amortization_years)
    transport = linehaul + last_mile
    return CostMetrics(
        linehaul_idr=linehaul,
        last_mile_idr=last_mile,
        transport_total_idr=transport,
        opening_cost_idr=opening,
        annual_fixed_cost_idr=fixed,
        annualized_opening_cost_idr=annualized_opening,
        annual_decision_cost_idr=transport + fixed + annualized_opening,
    )


def _warehouse_metrics(
    data: NetworkData,
    *,
    forward_loads: dict[str, int],
    center_loads: dict[str, int],
    candidate: Candidate | None = None,
    candidate_demand: int = 0,
) -> list[WarehouseMetric]:
    metrics: list[WarehouseMetric] = []
    for warehouse_id in (*data.centers, *data.forwards):
        warehouse = data.warehouses[warehouse_id]
        demand = (
            center_loads[warehouse_id]
            if warehouse.warehouse_type == "central"
            else forward_loads[warehouse_id]
        )
        metrics.append(
            WarehouseMetric(
                warehouse_id=warehouse.warehouse_id,
                warehouse_name=warehouse.name,
                warehouse_type=warehouse.warehouse_type,
                province_name=warehouse.province_name,
                longitude=warehouse.longitude,
                latitude=warehouse.latitude,
                annual_capacity_units=warehouse.capacity_units,
                assigned_demand_units=demand,
                utilization_ratio=(
                    demand / warehouse.capacity_units if warehouse.capacity_units else 0
                ),
                parent_center_id=warehouse.parent_center_id,
            )
        )
    if candidate is not None:
        metrics.append(
            WarehouseMetric(
                warehouse_id=candidate.candidate_id,
                warehouse_name=candidate.name,
                warehouse_type="candidate",
                province_name=candidate.province_name,
                longitude=candidate.longitude,
                latitude=candidate.latitude,
                annual_capacity_units=candidate.capacity_units,
                assigned_demand_units=candidate_demand,
                utilization_ratio=candidate_demand / candidate.capacity_units,
                parent_center_id=candidate.parent_center_id,
            )
        )
    return metrics


def _analysis_assumptions(data: NetworkData) -> list[str]:
    distance = data.policy["distance"]
    time = data.policy["time"]
    return [
        f"Distance uses {distance['formula']} with road factor {distance['road_factor']}; "
        "no navigation API is called.",
        f"Driving time uses {time['average_speed_kph']} km/h and "
        f"{time['driver_hours_per_day']} driving hours per driver-day.",
        "The customer promise starts from stocked forward inventory; linehaul time is "
        "reported for replenishment and is not added to customer delivery days.",
        "All customers, assignments, quotes, and warehouse economics are deterministic "
        "synthetic tutorial data, not observed Indonesian business records.",
    ]


def _scenario_coverage_for_days(
    scenario: IndonesiaCandidateScenario,
    target_service_days: int,
    data: NetworkData,
) -> float:
    key = f"{target_service_days}_day"
    if key in scenario.candidate_coverage.demand_coverage:
        return scenario.candidate_coverage.demand_coverage[key]
    numerator = 0.0
    for province in scenario.provinces:
        if province.demand_units == 0:
            continue
        province_key = f"{target_service_days}_day"
        if province_key not in province.candidate_demand_coverage:
            raise ValueError("Targets above three days require direct customer-vector evaluation")
        numerator += province.demand_units * province.candidate_demand_coverage[province_key]
    return numerator / sum(data.customers.demand_units)


def _coverage_at_days(
    service_days: array,
    demand_units: array,
    target_days: int,
) -> float:
    total = sum(demand_units)
    return (
        sum(
            demand
            for days, demand in zip(service_days, demand_units, strict=True)
            if days <= target_days
        )
        / total
    )


def _optional_delta(candidate: float | None, baseline: float | None) -> float | None:
    if candidate is None or baseline is None:
        return None
    return candidate - baseline


def _service_days(
    distance_km: float,
    speed_kph: float,
    driver_hours: float,
) -> int:
    return max(1, math.ceil(distance_km / speed_kph / driver_hours))


def _load_json(
    release: WorkspaceDatasetRelease,
    logical_name: str,
) -> dict[str, Any]:
    payload = json.loads(release.require_file(logical_name).path.read_text(encoding="utf-8"))
    if not isinstance(payload, dict):
        raise ValueError(f"Dataset file {logical_name} must contain a JSON object")
    return payload


def _read_csv(
    release: WorkspaceDatasetRelease,
    logical_name: str,
    expected_fields: list[str],
) -> list[dict[str, str]]:
    with release.require_file(logical_name).path.open(
        "r",
        encoding="utf-8",
        newline="",
    ) as source:
        reader = csv.DictReader(source)
        if reader.fieldnames != expected_fields:
            raise ValueError(f"Dataset file {logical_name} has an invalid CSV header")
        return list(reader)


def resource_name_from_uri(uri: str) -> str:
    return uri.rsplit("/", maxsplit=1)[-1]


def sorted_resource_schemas() -> Iterable[str]:
    return (
        "indonesia_dataset_inspection.v1",
        "indonesia_current_network_analysis.v1",
        "indonesia_candidate_scenario.v1",
        "indonesia_location_optimization.v1",
        "indonesia_network_map.v1",
    )
