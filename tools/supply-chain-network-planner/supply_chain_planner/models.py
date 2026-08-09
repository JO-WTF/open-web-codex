"""Typed public contracts for supply-chain network planning."""

from __future__ import annotations

from datetime import UTC, date, datetime
from decimal import Decimal
from typing import Annotated, Any, Literal

from pydantic import BaseModel, ConfigDict, Field, model_validator

from .mapping import SourceRole, TransformKind
from .network_models import (
    CurrentAssignmentRecord,
    DataQualityIssue,
    DemandCityRecord,
    RouteQuoteRecord,
    WarehouseRecord,
)

SCHEMA_VERSION = "1.0"
MCP_SERVER_NAME = "supply_chain"

NonNegativeMoney = Annotated[Decimal, Field(ge=0, decimal_places=6)]


def utc_now() -> datetime:
    return datetime.now(UTC)


class StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid", populate_by_name=True)


class ResourceRef(StrictModel):
    """An exact provider-owned MCP Resource reference.

    MIME type remains on the official ResourceLink/Resource contents contract;
    a resource reference carries only the identity needed to read it again.
    """

    type: Literal["mcp_resource"] = "mcp_resource"
    server: Literal["supply_chain"] = MCP_SERVER_NAME
    uri: str = Field(pattern=r"^supply-chain://resources/[a-z0-9_.-]{1,160}$")
    resource_schema: str = Field(
        min_length=1,
        max_length=128,
        pattern=r"^[a-z][a-z0-9_.-]*$",
    )


class PreparedNetworkResource(StrictModel):
    """Typed Data-to-Network Resource payload owned by supply_chain."""

    schema_version: Literal["normalized_network_input.v1"] = Field(
        default="normalized_network_input.v1",
        alias="schemaVersion",
    )
    country_code: str = Field(pattern=r"^[A-Z]{2,3}$")
    state: Literal["ready", "needs_input", "needs_geography"]
    demand_cities: list[DemandCityRecord]
    warehouses: list[WarehouseRecord]
    current_assignments: list[CurrentAssignmentRecord]
    route_quotes: list[RouteQuoteRecord]
    issues: list[DataQualityIssue] = Field(default_factory=list)


class ConfirmedFieldDecision(StrictModel):
    source_field: str = Field(min_length=1, max_length=256)
    target_field: str = Field(min_length=1, max_length=128)
    transform: TransformKind
    factor: Decimal | None = None


class ConfirmedSourceDecision(StrictModel):
    relative_path: str = Field(min_length=1, max_length=1024)
    role: SourceRole
    mappings: list[ConfirmedFieldDecision] = Field(min_length=1, max_length=64)

    @model_validator(mode="after")
    def reject_catalog_role(self) -> ConfirmedSourceDecision:
        if self.role == SourceRole.ADMINISTRATIVE_CATALOG:
            raise ValueError("administrative catalog is prepared by the geography tool")
        return self


class GeographyOverride(StrictModel):
    entity: Literal["demand", "warehouse"]
    entity_id: str = Field(min_length=1, max_length=128)
    catalog_city_id: str = Field(min_length=1, max_length=128)


EvidenceRef = ResourceRef


class Point(StrictModel):
    latitude: float = Field(ge=-90, le=90)
    longitude: float = Field(ge=-180, le=180)


class ServicePolicy(StrictModel):
    policy_id: str = Field(min_length=1, max_length=128)
    max_delivery_seconds: int = Field(gt=0)
    order_cutoff_wait_seconds: int = Field(default=0, ge=0)
    last_mile_buffer_seconds: int = Field(default=0, ge=0)
    label: str | None = Field(default=None, max_length=256)


class City(StrictModel):
    city_id: str = Field(min_length=1, max_length=128)
    name: str = Field(min_length=1, max_length=256)
    region: str = Field(min_length=1, max_length=128)
    location: Point


class DemandPoint(StrictModel):
    demand_id: str = Field(min_length=1, max_length=128)
    city_id: str = Field(min_length=1, max_length=128)
    location: Point
    demand_units: int = Field(ge=0)
    region: str = Field(min_length=1, max_length=128)
    current_facility_id: str | None = Field(default=None, max_length=128)


class Facility(StrictModel):
    facility_id: str = Field(min_length=1, max_length=128)
    city_id: str = Field(min_length=1, max_length=128)
    location: Point
    capacity_units: int = Field(ge=0)
    is_existing: bool
    handling_seconds: int = Field(default=0, ge=0)
    fixed_cost: NonNegativeMoney = Decimal("0")
    opening_cost: NonNegativeMoney = Decimal("0")
    handling_cost_per_unit: NonNegativeMoney = Decimal("0")
    label: str | None = Field(default=None, max_length=256)


class TransportRate(StrictModel):
    rate_id: str = Field(min_length=1, max_length=128)
    origin_city_id: str = Field(min_length=1, max_length=128)
    destination_city_id: str = Field(min_length=1, max_length=128)
    base_cost_per_unit: NonNegativeMoney = Decimal("0")
    distance_cost_per_km_per_unit: NonNegativeMoney = Decimal("0")


class NetworkInput(StrictModel):
    schema_version: Literal["network_input.v1"] = "network_input.v1"
    planning_period: str = Field(min_length=1, max_length=128)
    currency: str = Field(pattern=r"^[A-Z]{3}$")
    service_policy: ServicePolicy
    cities: list[City] = Field(min_length=1)
    demand_points: list[DemandPoint] = Field(min_length=1)
    facilities: list[Facility] = Field(min_length=1)
    transport_rates: list[TransportRate] = Field(default_factory=list)

    @model_validator(mode="after")
    def validate_references(self) -> NetworkInput:
        demand_ids = [item.demand_id for item in self.demand_points]
        city_ids = [item.city_id for item in self.cities]
        facility_ids = [item.facility_id for item in self.facilities]
        rate_ids = [item.rate_id for item in self.transport_rates]
        for label, identifiers in (
            ("demand", demand_ids),
            ("city", city_ids),
            ("facility", facility_ids),
            ("transport rate", rate_ids),
        ):
            if len(identifiers) != len(set(identifiers)):
                raise ValueError(f"duplicate {label} identifier")

        city_by_id = {item.city_id: item for item in self.cities}
        facility_id_set = set(facility_ids)
        for demand in self.demand_points:
            city = city_by_id.get(demand.city_id)
            if city is None:
                raise ValueError(
                    f"demand {demand.demand_id!r} references unknown city {demand.city_id!r}"
                )
            if demand.location != city.location or demand.region != city.region:
                raise ValueError(
                    f"demand {demand.demand_id!r} city projection does not match {demand.city_id!r}"
                )
            if (
                demand.current_facility_id is not None
                and demand.current_facility_id not in facility_id_set
            ):
                raise ValueError(
                    f"demand {demand.demand_id!r} references unknown current facility "
                    f"{demand.current_facility_id!r}"
                )
        for facility in self.facilities:
            if facility.city_id not in city_by_id:
                raise ValueError(
                    f"facility {facility.facility_id!r} references unknown city "
                    f"{facility.city_id!r}"
                )
        city_ids_set = set(city_ids)
        selectors: set[tuple[str, str]] = set()
        for rate in self.transport_rates:
            if rate.origin_city_id not in city_ids_set:
                raise ValueError(
                    f"rate {rate.rate_id!r} references unknown origin city {rate.origin_city_id!r}"
                )
            if rate.destination_city_id not in city_by_id:
                raise ValueError(
                    f"rate {rate.rate_id!r} references unknown destination city "
                    f"{rate.destination_city_id!r}"
                )
            selector = (rate.origin_city_id, rate.destination_city_id)
            if selector in selectors:
                raise ValueError(
                    f"ambiguous transport rates: more than one rate has selector {selector!r}"
                )
            selectors.add(selector)
        required_selectors = {
            (facility.city_id, demand.city_id)
            for facility in self.facilities
            for demand in self.demand_points
        }
        missing_selectors = required_selectors - selectors
        if missing_selectors:
            raise ValueError(
                "missing transport rates for "
                f"{len(missing_selectors)} origin-region to destination-city lanes"
            )
        return self


class CityDemand(StrictModel):
    city_id: str = Field(min_length=1, max_length=128)
    demand_date: date
    demand_units: int = Field(gt=0)


class WarehouseCityCoverage(StrictModel):
    facility_id: str = Field(min_length=1, max_length=128)
    city_id: str = Field(min_length=1, max_length=128)
    is_current: bool = True


class CityLane(StrictModel):
    origin_city_id: str = Field(min_length=1, max_length=128)
    destination_city_id: str = Field(min_length=1, max_length=128)
    distance_km: Decimal = Field(ge=0)
    travel_time_hours: Decimal = Field(ge=0)
    base_cost_per_unit: NonNegativeMoney = Decimal("0")
    distance_cost_per_km_per_unit: NonNegativeMoney = Decimal("0")
    currency: str = Field(pattern=r"^[A-Z]{3}$")


class RouteFact(StrictModel):
    origin_city_id: str
    destination_city_id: str
    distance_meters: int | None = Field(default=None, ge=0)
    travel_seconds: int | None = Field(default=None, ge=0)
    status: Literal["ready", "unreachable", "error"] = "ready"
    error_code: str | None = Field(default=None, max_length=128)

    @model_validator(mode="after")
    def ready_route_has_metrics(self) -> RouteFact:
        if self.status == "ready" and (self.distance_meters is None or self.travel_seconds is None):
            raise ValueError("ready route entries require distance_meters and travel_seconds")
        return self


class PlanningSource(StrictModel):
    schema_version: Literal["planning_source.v2"] = "planning_source.v2"
    source_id: str = Field(pattern=r"^[a-z0-9][a-z0-9_-]{0,127}$")
    market: str = Field(pattern=r"^[A-Z]{2}$")
    label: str = Field(min_length=1, max_length=256)
    source_updated_at: datetime
    planning_period: str = Field(min_length=1, max_length=128)
    currency: str = Field(pattern=r"^[A-Z]{3}$")
    service_policy: ServicePolicy
    cities: list[City] = Field(min_length=1)
    city_demands: list[CityDemand] = Field(min_length=1)
    facilities: list[Facility] = Field(min_length=1)
    warehouse_city_coverage: list[WarehouseCityCoverage] = Field(min_length=1)
    lanes: list[CityLane] = Field(min_length=1)
    route_provider: str = Field(default="workspace", min_length=1, max_length=128)
    route_method: Literal["navigation", "quoted", "haversine_estimate"]
    data_classification: Literal["workspace_data", "synthetic_demo"] = Field(
        default="workspace_data", alias="dataClassification"
    )
    demo_template: dict[str, Any] | None = Field(default=None, alias="demoTemplate")

    @model_validator(mode="after")
    def validate_source_references(self) -> PlanningSource:
        city_by_id = {item.city_id: item for item in self.cities}
        if len(city_by_id) != len(self.cities):
            raise ValueError("duplicate city identifier")
        existing_facility_ids = {item.facility_id for item in self.facilities if item.is_existing}
        if not existing_facility_ids:
            raise ValueError("planning source requires at least one existing facility")
        demand_city_ids = [item.city_id for item in self.city_demands]
        if len(demand_city_ids) != len(set(demand_city_ids)):
            raise ValueError("duplicate city demand identifier")
        if any(city_id not in city_by_id for city_id in demand_city_ids):
            raise ValueError("city demand references unknown city")
        facility_ids = {facility.facility_id for facility in self.facilities}
        for coverage in self.warehouse_city_coverage:
            if coverage.facility_id not in facility_ids or coverage.city_id not in city_by_id:
                raise ValueError("warehouse city coverage references unknown entity")
            if coverage.is_current and coverage.facility_id not in existing_facility_ids:
                raise ValueError("current coverage must reference an existing facility")
        lane_pairs = [(lane.origin_city_id, lane.destination_city_id) for lane in self.lanes]
        if len(lane_pairs) != len(set(lane_pairs)):
            raise ValueError("duplicate city lane")
        if any(
            lane.origin_city_id not in city_by_id
            or lane.destination_city_id not in city_by_id
            or lane.currency != self.currency.upper()
            for lane in self.lanes
        ):
            raise ValueError("city lane references unknown city or has a currency mismatch")
        expected_pairs = {
            (facility.city_id, city_id)
            for facility in self.facilities
            for city_id in demand_city_ids
        }
        if set(lane_pairs) != expected_pairs:
            raise ValueError("planning source city lanes are incomplete")
        current_coverage = {
            coverage.city_id: coverage.facility_id
            for coverage in self.warehouse_city_coverage
            if coverage.is_current
        }
        if set(current_coverage) != set(demand_city_ids):
            raise ValueError("current warehouse city coverage is incomplete")
        if any(
            facility_id not in existing_facility_ids for facility_id in current_coverage.values()
        ):
            raise ValueError("current coverage references a non-existing facility")
        projected_demands = [
            DemandPoint(
                demand_id=f"city-demand-{item.city_id}",
                city_id=item.city_id,
                location=city_by_id[item.city_id].location,
                demand_units=item.demand_units,
                region=city_by_id[item.city_id].region,
                current_facility_id=current_coverage.get(item.city_id),
            )
            for item in self.city_demands
        ]
        NetworkInput(
            planning_period=self.planning_period,
            currency=self.currency,
            service_policy=self.service_policy,
            cities=self.cities,
            demand_points=projected_demands,
            facilities=self.facilities,
            transport_rates=[
                TransportRate(
                    rate_id=f"rate-{lane.origin_city_id}-{lane.destination_city_id}",
                    origin_city_id=lane.origin_city_id,
                    destination_city_id=lane.destination_city_id,
                    base_cost_per_unit=lane.base_cost_per_unit,
                    distance_cost_per_km_per_unit=lane.distance_cost_per_km_per_unit,
                )
                for lane in self.lanes
            ],
        )
        return self


class PlanningSourceSummary(StrictModel):
    source_id: str
    market: str = Field(pattern=r"^[A-Z]{2}$")
    label: str
    source_updated_at: datetime
    city_demand_row_count: int = Field(ge=0)
    demand_node_count: int = Field(ge=0)
    facility_count: int = Field(ge=0)
    existing_facility_count: int = Field(ge=0)
    candidate_facility_count: int = Field(ge=0)
    lane_count: int = Field(ge=0)
    date_from: date
    date_to: date
    demand_units: int = Field(ge=0)
    truncated: Literal[False] = False


class DemandDistributionRow(StrictModel):
    demand_id: str
    region: str | None
    demand_units: int = Field(ge=0)
    city_demand_row_count: int = Field(ge=0)
    promotion_units: int = Field(ge=0)
    promotion_share: float = Field(ge=0, le=1)


class DeliveryBaseline(StrictModel):
    observed_demand_units: int = Field(ge=0)
    on_time_demand_units: int = Field(ge=0)
    unobserved_demand_units: int = Field(ge=0)
    on_time_ratio: float | None = Field(default=None, ge=0, le=1)
    threshold_seconds: int = Field(gt=0)


class PlanningDataQuality(StrictModel):
    valid: bool
    errors: list[str] = Field(default_factory=list)
    warnings: list[str] = Field(default_factory=list)


class PlanningDataset(StrictModel):
    schema_version: Literal["planning-dataset.v2"] = "planning-dataset.v2"
    # Bounded cross-agent provenance envelope.  The canonical planning schema
    # remains in the fields below; these fields only make ownership and
    # confirmation hashes explicit at the MCP boundary.
    schema_version_envelope: str | None = Field(default=None, alias="schemaVersion")
    contract: dict[str, Any] | None = None
    task_evidence: dict[str, Any] | None = Field(default=None, alias="taskEvidence")
    dataset_id: str
    created_at: datetime = Field(default_factory=utc_now)
    source_digest: str = Field(pattern=r"^[a-f0-9]{64}$")
    source_summary: PlanningSourceSummary
    network_input: NetworkInput
    route_provider: str
    route_method: Literal["navigation", "quoted", "haversine_estimate"]
    route_entries: list[RouteFact]
    demand_distribution: list[DemandDistributionRow]
    delivery_baseline: DeliveryBaseline
    data_quality: PlanningDataQuality
    assumptions: list[str] = Field(default_factory=list)
    # The normalized rows remain in the MCP Resource.  This bounded envelope is
    # intentionally opaque to the planner and carries only lineage needed for
    # cross-agent handoff and replay.
    normalization: dict[str, Any] = Field(default_factory=dict)
    normalization_status: Literal["ready"] = "ready"
    data_classification: Literal["workspace_data", "synthetic_demo"] = Field(
        default="workspace_data", alias="dataClassification"
    )
    demo_template: dict[str, Any] | None = Field(default=None, alias="demoTemplate")


class PlanningSourceInspection(StrictModel):
    schema_version: Literal["planning_source_inspection.v2"] = "planning_source_inspection.v2"
    source_summary: PlanningSourceSummary
    data_quality: PlanningDataQuality


class PlanningSourceCatalogEntry(StrictModel):
    source_id: str = Field(pattern=r"^[a-z0-9][a-z0-9_-]{0,127}$")
    market: str = Field(pattern=r"^[A-Z]{2}$")
    label: str = Field(min_length=1, max_length=256)
    source_updated_at: datetime
    planning_period: str = Field(min_length=1, max_length=128)
    currency: str = Field(pattern=r"^[A-Z]{3}$")
    service_policy_id: str = Field(min_length=1, max_length=128)
    date_from: date
    date_to: date
    city_demand_row_count: int = Field(ge=0)
    demand_units: int = Field(ge=0)
    demand_node_count: int = Field(ge=0)
    facility_count: int = Field(ge=0)
    existing_facility_count: int = Field(ge=0)
    candidate_facility_count: int = Field(ge=0)
    lane_count: int = Field(ge=0)
    regions: list[str]


class PlanningSourceCatalog(StrictModel):
    schema_version: Literal["planning_source_catalog.v2"] = "planning_source_catalog.v2"
    sources: list[PlanningSourceCatalogEntry]
    truncated: bool = False


class DataAgentResourceToolResult(StrictModel):
    summary: str
    resource_name: str
    resource_ref: ResourceRef


class NetworkSnapshot(NetworkInput):
    schema_version: Literal["network_snapshot.v1"] = "network_snapshot.v1"
    snapshot_id: str
    created_at: datetime = Field(default_factory=utc_now)
    source_name: str | None = None
    source_digest: str | None = Field(default=None, pattern=r"^[a-f0-9]{64}$")
    data_classification: Literal["workspace_data", "synthetic_demo"] = Field(
        default="workspace_data", alias="dataClassification"
    )
    demo_template: dict[str, Any] | None = Field(default=None, alias="demoTemplate")


class RouteEntry(RouteFact):
    pass


class RouteMatrix(StrictModel):
    schema_version: Literal["route_matrix.v1"] = "route_matrix.v1"
    route_matrix_id: str
    snapshot_id: str
    provider: str = Field(min_length=1, max_length=128)
    method: Literal["navigation", "quoted", "haversine_estimate"]
    calculated_at: datetime = Field(default_factory=utc_now)
    entries: list[RouteEntry]


class Allocation(StrictModel):
    demand_id: str
    facility_id: str | None
    units: int = Field(ge=0)
    covered: bool
    end_to_end_seconds: int | None = Field(default=None, ge=0)
    distance_meters: int | None = Field(default=None, ge=0)
    variable_cost: NonNegativeMoney = Decimal("0")
    rate_id: str | None = None
    reason: str | None = None


class NetworkMetrics(StrictModel):
    total_demand_units: int = Field(ge=0)
    covered_demand_units: int = Field(ge=0)
    uncovered_demand_units: int = Field(ge=0)
    coverage_ratio: float = Field(ge=0, le=1)
    fixed_cost: NonNegativeMoney
    variable_cost: NonNegativeMoney
    total_cost: NonNegativeMoney
    active_facility_count: int = Field(ge=0)


class NetworkScenarioResult(StrictModel):
    schema_version: Literal["network_scenario_result.v1"] = "network_scenario_result.v1"
    result_id: str
    snapshot_id: str
    route_matrix_id: str
    service_policy_id: str
    scenario_id: str
    mode: Literal["current_assignment", "optimized"]
    active_facility_ids: list[str]
    metrics: NetworkMetrics
    allocations: list[Allocation]
    issues: list[str] = Field(default_factory=list)
    calculated_at: datetime = Field(default_factory=utc_now)


class CurrentCoverageResult(StrictModel):
    schema_version: Literal["current_coverage_result.v1"] = "current_coverage_result.v1"
    snapshot_id: str
    route_matrix_id: str
    actual_result_resource_name: str
    actual_result_ref: ResourceRef
    optimized_result_resource_name: str
    optimized_result_ref: ResourceRef
    actual_metrics: NetworkMetrics
    optimized_metrics: NetworkMetrics
    interpretation: str


class ScenarioComparison(StrictModel):
    schema_version: Literal["scenario_comparison.v1"] = "scenario_comparison.v1"
    baseline_result_id: str
    candidate_result_id: str
    coverage_ratio_delta: float
    covered_demand_units_delta: int
    total_cost_delta: Decimal
    fixed_cost_delta: Decimal
    variable_cost_delta: Decimal
    comparable: bool = True


class FacilityLocationSolution(StrictModel):
    schema_version: Literal["facility_location_solution.v1"] = "facility_location_solution.v1"
    solution_id: str
    snapshot_id: str
    route_matrix_id: str
    target_coverage_ratio: float = Field(gt=0, le=1)
    status: Literal["optimal", "infeasible"]
    selected_candidate_facility_ids: list[str]
    active_facility_ids: list[str]
    evaluated_subset_count: int = Field(ge=0)
    result_resource_name: str
    result_ref: ResourceRef
    metrics: NetworkMetrics
    method: Literal["exact_subset_enumeration_with_min_cost_flow"] = (
        "exact_subset_enumeration_with_min_cost_flow"
    )
    assumptions: list[str]


class FinancialEvaluation(StrictModel):
    schema_version: Literal["financial_evaluation.v1"] = "financial_evaluation.v1"
    evaluation_id: str
    snapshot_id: str
    baseline_result_id: str
    candidate_result_id: str
    currency: str = Field(pattern=r"^[A-Z]{3}$")
    planning_period: str
    horizon_years: int = Field(ge=1, le=20)
    discount_rate: float = Field(ge=0, le=1)
    annual_growth_rate: float = Field(ge=-0.5, le=1)
    opening_investment: NonNegativeMoney
    annual_operating_savings: Decimal
    net_present_value: Decimal
    payback_years: float | None = Field(default=None, ge=0)
    financially_viable: bool
    input_refs: list[ResourceRef] = Field(min_length=3)
    assumptions: list[str] = Field(min_length=1)


class RiskItem(StrictModel):
    risk_id: str = Field(min_length=1, max_length=128)
    category: Literal[
        "demand",
        "service",
        "financial",
        "operational",
        "regulatory",
        "data",
    ]
    statement: str = Field(min_length=1, max_length=1024)
    likelihood: int = Field(ge=1, le=5)
    impact: int = Field(ge=1, le=5)
    mitigation: str = Field(min_length=1, max_length=1024)
    trigger: str = Field(min_length=1, max_length=1024)
    evidence_refs: list[EvidenceRef] = Field(min_length=1)


class RiskRegister(StrictModel):
    schema_version: Literal["risk_register.v1"] = "risk_register.v1"
    register_id: str
    decision_scope: str = Field(min_length=1, max_length=1024)
    risks: list[RiskItem] = Field(min_length=1)
    unresolved_risk_count: int = Field(ge=0)
    published_at: datetime = Field(default_factory=utc_now)


class ValidationResult(StrictModel):
    schema_version: Literal["network_validation.v1"] = "network_validation.v1"
    valid: bool
    resource_schema: str
    errors: list[str] = Field(default_factory=list)
    warnings: list[str] = Field(default_factory=list)
    checks: list[str] = Field(default_factory=list)


class ResourceToolResult(StrictModel):
    summary: str
    resource_name: str
    resource_ref: ResourceRef


class NetworkMapToolResult(StrictModel):
    summary: str
    map_resource_name: str
    map_ref: ResourceRef
    geojson_resource_name: str
    geojson_ref: ResourceRef
    feature_count: int = Field(ge=0)
    title: str
    layers: list[dict[str, Any]]
    extensions: dict[str, Any] = Field(default_factory=dict)


class NetworkSnapshotPreparationToolResult(StrictModel):
    summary: str
    snapshot_resource_name: str
    snapshot_ref: ResourceRef
    route_matrix_resource_name: str
    route_matrix_ref: ResourceRef


class NetworkMapRenderToolResult(StrictModel):
    summary: str
    map_manifest_resource_name: str
    geojson_resource_name: str
    geojson_ref: ResourceRef
    title: str
    layers: list[dict[str, Any]]
    extensions: dict[str, Any] = Field(default_factory=dict)


class NetworkPlanningReportToolResult(StrictModel):
    summary: str
    resource_name: str
    resource_ref: ResourceRef
    artifact_type: Literal["report.v1"] = "report.v1"
    title: str
    markdown_sha256: str = Field(pattern=r"^[a-f0-9]{64}$")
    report_resource_name: str
    report_ref: ResourceRef


class CurrentCoverageToolResult(CurrentCoverageResult):
    summary: str
    resource_name: str
    resource_ref: ResourceRef


class ComparisonToolResult(ScenarioComparison):
    summary: str
    resource_name: str
    resource_ref: ResourceRef


class FacilityLocationToolResult(FacilityLocationSolution):
    summary: str
    resource_name: str
    solution_ref: ResourceRef


class FinancialEvaluationToolResult(FinancialEvaluation):
    summary: str
    resource_name: str
    resource_ref: ResourceRef


class RiskRegisterToolResult(RiskRegister):
    summary: str
    resource_name: str
    resource_ref: ResourceRef
