"""Typed contracts for the progressive Indonesia warehouse-network tutorials."""

from __future__ import annotations

from typing import Any, Literal

from pydantic import BaseModel, ConfigDict, Field

INDONESIA_MCP_SERVER_NAME = "supply_chain_indonesia"
INDONESIA_RESOURCE_URI_PREFIX = "supply-chain-indonesia://resources/"
INDONESIA_GEOJSON_URI_PREFIX = "supply-chain-indonesia://geojson/"
INDONESIA_RESOURCE_NAME_PATTERN = r"^[a-z0-9_.-]{1,160}$"


class StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid")


class DatasetReleaseBinding(StrictModel):
    workspace_id: str = Field(
        pattern=r"^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$"
    )
    release_id: str = Field(
        pattern=r"^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$"
    )
    dataset_id: str = Field(pattern=r"^[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?$")
    version: str = Field(pattern=r"^[A-Za-z0-9](?:[A-Za-z0-9._-]{0,62}[A-Za-z0-9])?$")
    content_sha256: str = Field(pattern=r"^[0-9a-f]{64}$")


class IndonesiaDataRef(StrictModel):
    type: Literal["mcp_resource"] = "mcp_resource"
    server: Literal["supply_chain_indonesia"] = INDONESIA_MCP_SERVER_NAME
    uri: str = Field(pattern=r"^supply-chain-indonesia://resources/[a-z0-9_.-]{1,160}$")
    format: Literal["json"] = "json"
    resource_schema: str = Field(min_length=1, max_length=128)


class IndonesiaGeoJsonRef(StrictModel):
    type: Literal["mcp_resource"] = "mcp_resource"
    server: Literal["supply_chain_indonesia"] = INDONESIA_MCP_SERVER_NAME
    uri: str = Field(pattern=r"^supply-chain-indonesia://geojson/[a-z0-9_.-]{1,160}$")
    format: Literal["geojson"] = "geojson"


class InspectDatasetReleaseInput(StrictModel):
    release: DatasetReleaseBinding


class InspectionResourceInput(StrictModel):
    inspection_resource_name: str = Field(
        pattern=r"^indonesia_dataset_inspection\.v1-[0-9a-f]{24}$"
    )


class CandidateScenarioInput(InspectionResourceInput):
    candidate_id: str = Field(pattern=r"^CAN-[A-Z0-9-]{1,96}$")
    opening_amortization_years: int = Field(default=5, ge=1, le=20)


class OptimizeWarehouseInput(InspectionResourceInput):
    target_service_days: int = Field(ge=1, le=3)
    target_demand_coverage: float = Field(gt=0, le=1)
    opening_amortization_years: int = Field(default=5, ge=1, le=20)


class PrepareMapInput(StrictModel):
    baseline_resource_name: str = Field(
        pattern=r"^indonesia_current_network_analysis\.v1-[0-9a-f]{24}$"
    )
    candidate_resource_name: str = Field(
        pattern=r"^indonesia_candidate_scenario\.v1-[0-9a-f]{24}$"
    )


class PrepareMapRenderInput(StrictModel):
    map_resource_name: str = Field(
        pattern=r"^indonesia_network_map\.v1-[0-9a-f]{24}$"
    )
    geojson_resource_name: str = Field(pattern=r"^geojson\.v1-[0-9a-f]{24}$")


class PrepareDecisionReportInput(StrictModel):
    inspection_resource_name: str = Field(
        pattern=r"^indonesia_dataset_inspection\.v1-[0-9a-f]{24}$"
    )
    service_resource_name: str = Field(
        pattern=r"^indonesia_service_baseline\.v1-[0-9a-f]{24}$"
    )
    current_resource_name: str = Field(
        pattern=r"^indonesia_current_network_analysis\.v1-[0-9a-f]{24}$"
    )
    optimization_resource_name: str = Field(
        pattern=r"^indonesia_location_optimization\.v1-[0-9a-f]{24}$"
    )
    candidate_resource_name: str = Field(
        pattern=r"^indonesia_candidate_scenario\.v1-[0-9a-f]{24}$"
    )
    map_resource_name: str = Field(
        pattern=r"^indonesia_network_map\.v1-[0-9a-f]{24}$"
    )
    geojson_resource_name: str = Field(pattern=r"^geojson\.v1-[0-9a-f]{24}$")


class ValidateResourceInput(StrictModel):
    resource_ref: IndonesiaDataRef


class DatasetFileInspection(StrictModel):
    logical_name: str
    role: str
    media_type: str
    byte_size: int = Field(gt=0)
    content_sha256: str = Field(pattern=r"^[0-9a-f]{64}$")
    row_count: int = Field(ge=0)


class IndonesiaDatasetInspection(StrictModel):
    schema_version: Literal["indonesia_dataset_inspection.v1"] = "indonesia_dataset_inspection.v1"
    release: DatasetReleaseBinding
    internal_content_sha256: str = Field(pattern=r"^[0-9a-f]{64}$")
    data_classification: Literal["synthetic_tutorial_data"]
    customer_count: int = Field(gt=0)
    annual_demand_units: int = Field(gt=0)
    province_count: int = Field(gt=0)
    central_warehouse_count: int = Field(gt=0)
    forward_warehouse_count: int = Field(gt=0)
    candidate_location_count: int = Field(gt=0)
    quote_row_count: int = Field(gt=0)
    files: list[DatasetFileInspection]
    policy: dict[str, Any]
    source_attribution: list[str]
    checks: list[str]
    warnings: list[str] = Field(default_factory=list)


class CoverageMetrics(StrictModel):
    total_customers: int = Field(ge=0)
    total_demand_units: int = Field(ge=0)
    customer_coverage: dict[str, float]
    demand_coverage: dict[str, float]


class CostMetrics(StrictModel):
    currency: Literal["IDR"] = "IDR"
    linehaul_idr: int = Field(ge=0)
    last_mile_idr: int = Field(ge=0)
    transport_total_idr: int = Field(ge=0)
    opening_cost_idr: int = Field(ge=0)
    annual_fixed_cost_idr: int = Field(ge=0)
    annualized_opening_cost_idr: int = Field(ge=0)
    annual_decision_cost_idr: int = Field(ge=0)


class ProvinceServiceMetric(StrictModel):
    province_code: str
    province_name: str
    customer_count: int = Field(ge=0)
    demand_units: int = Field(ge=0)
    demand_weighted_average_service_days: float | None
    maximum_service_days: int | None
    demand_coverage: dict[str, float]
    demand_centroid: tuple[float, float] | None


class ProvinceRankingPolicy(StrictModel):
    eligible_provinces: Literal["positive_demand_only"] = "positive_demand_only"
    primary_metric: Literal["two_day_demand_coverage"] = "two_day_demand_coverage"
    priority_order: Literal[
        "ascending_coverage_then_descending_average_service_days"
    ] = "ascending_coverage_then_descending_average_service_days"
    best_order: Literal[
        "descending_coverage_then_ascending_average_service_days"
    ] = "descending_coverage_then_ascending_average_service_days"
    returned_limit: Literal[6] = 6


class ProvinceServiceComparison(StrictModel):
    province_code: str
    province_name: str
    demand_units: int = Field(ge=0)
    baseline_average_service_days: float | None
    candidate_average_service_days: float | None
    average_service_days_delta: float | None
    baseline_demand_coverage: dict[str, float]
    candidate_demand_coverage: dict[str, float]
    demand_centroid: tuple[float, float] | None


class WarehouseMetric(StrictModel):
    warehouse_id: str
    warehouse_name: str
    warehouse_type: Literal["central", "forward", "candidate"]
    province_name: str
    longitude: float
    latitude: float
    annual_capacity_units: int = Field(ge=0)
    assigned_demand_units: int = Field(ge=0)
    utilization_ratio: float = Field(ge=0)
    parent_center_id: str | None = None


class NetworkLink(StrictModel):
    origin_id: str
    destination_id: str
    leg_type: Literal["linehaul", "candidate_linehaul"]
    demand_units: int = Field(ge=0)


class IndonesiaServiceBaseline(StrictModel):
    schema_version: Literal["indonesia_service_baseline.v1"] = (
        "indonesia_service_baseline.v1"
    )
    release: DatasetReleaseBinding
    method: Literal["current_forward_assignment_last_mile_only"] = (
        "current_forward_assignment_last_mile_only"
    )
    coverage: CoverageMetrics
    provinces: list[ProvinceServiceMetric]
    province_ranking_policy: ProvinceRankingPolicy
    priority_province_codes: list[str]
    best_province_codes: list[str]
    assumptions: list[str]
    checks: list[str]


class IndonesiaCurrentNetworkAnalysis(StrictModel):
    schema_version: Literal["indonesia_current_network_analysis.v1"] = (
        "indonesia_current_network_analysis.v1"
    )
    release: DatasetReleaseBinding
    coverage: CoverageMetrics
    costs: CostMetrics
    provinces: list[ProvinceServiceMetric]
    warehouses: list[WarehouseMetric]
    links: list[NetworkLink]
    province_ranking_policy: ProvinceRankingPolicy
    priority_province_codes: list[str]
    best_province_codes: list[str]
    assumptions: list[str]
    checks: list[str]


class CandidateSummary(StrictModel):
    candidate_id: str
    candidate_name: str
    province_name: str
    longitude: float
    latitude: float
    parent_center_id: str
    annual_capacity_units: int = Field(gt=0)
    selected_customer_count: int = Field(ge=0)
    selected_demand_units: int = Field(ge=0)


class IndonesiaCandidateScenario(StrictModel):
    schema_version: Literal["indonesia_candidate_scenario.v1"] = "indonesia_candidate_scenario.v1"
    release: DatasetReleaseBinding
    candidate: CandidateSummary
    opening_amortization_years: int = Field(ge=1, le=20)
    baseline_coverage: CoverageMetrics
    candidate_coverage: CoverageMetrics
    baseline_costs: CostMetrics
    candidate_costs: CostMetrics
    demand_coverage_delta: dict[str, float]
    transport_cost_delta_idr: int
    annual_decision_cost_delta_idr: int
    provinces: list[ProvinceServiceComparison]
    warehouses: list[WarehouseMetric]
    links: list[NetworkLink]
    assignment_policy: str
    constraints: list[str]
    checks: list[str]


class CandidateEvaluation(StrictModel):
    candidate_id: str
    candidate_name: str
    selected_demand_units: int = Field(ge=0)
    target_demand_coverage: float = Field(ge=0, le=1)
    target_met: bool
    transport_total_idr: int = Field(ge=0)
    annual_decision_cost_idr: int = Field(ge=0)
    demand_coverage_delta: float


class IndonesiaLocationOptimization(StrictModel):
    schema_version: Literal["indonesia_location_optimization.v1"] = (
        "indonesia_location_optimization.v1"
    )
    release: DatasetReleaseBinding
    target_service_days: int = Field(ge=1, le=3)
    target_demand_coverage: float = Field(gt=0, le=1)
    status: Literal["target_already_met", "target_met", "best_available"]
    selected_candidate_id: str | None
    evaluated_candidate_count: int = Field(ge=0)
    target_met_candidate_count: int = Field(ge=0)
    evaluations: list[CandidateEvaluation]
    selected_scenario_resource_name: str | None
    selected_scenario_ref: IndonesiaDataRef | None
    method: Literal["exact_finite_candidate_evaluation"] = "exact_finite_candidate_evaluation"
    assumptions: list[str]


class IndonesiaNetworkMap(StrictModel):
    schema_version: Literal["indonesia_network_map.v1"] = "indonesia_network_map.v1"
    release: DatasetReleaseBinding
    baseline_resource_name: str
    candidate_resource_name: str
    title: str
    summary: str
    geojson_resource_name: str
    geojson_ref: IndonesiaGeoJsonRef
    feature_count: int = Field(gt=0, le=200)
    layers: list[dict[str, Any]]
    extensions: dict[str, Any]


class IndonesiaDecisionReportSources(StrictModel):
    inspection_resource_name: str
    service_resource_name: str
    current_resource_name: str
    optimization_resource_name: str
    candidate_resource_name: str
    map_resource_name: str
    geojson_resource_name: str


class IndonesiaDecisionReport(StrictModel):
    schema_version: Literal["indonesia_decision_report.v1"] = (
        "indonesia_decision_report.v1"
    )
    release: DatasetReleaseBinding
    sources: IndonesiaDecisionReportSources
    markdown: str = Field(min_length=1, max_length=64_000)
    markdown_sha256: str = Field(pattern=r"^[0-9a-f]{64}$")
    checks: list[str]


class IndonesiaValidationResult(StrictModel):
    schema_version: Literal["indonesia_resource_validation.v1"] = "indonesia_resource_validation.v1"
    resource_schema: str
    valid: bool
    checks: list[str] = Field(default_factory=list)
    warnings: list[str] = Field(default_factory=list)
    errors: list[str] = Field(default_factory=list)


class IndonesiaResourceToolResult(StrictModel):
    summary: str
    resource_name: str
    data_ref: IndonesiaDataRef


class IndonesiaOptimizationToolResult(IndonesiaResourceToolResult):
    evaluated_candidate_count: int = Field(ge=0)
    target_met_candidate_count: int = Field(ge=0)
    selected_candidate_id: str | None
    status: Literal["target_already_met", "target_met", "best_available"]
    selected_scenario_resource_name: str | None
    selected_scenario_ref: IndonesiaDataRef | None


class IndonesiaMapToolResult(IndonesiaResourceToolResult):
    geojson_resource_name: str
    geojson_ref: IndonesiaGeoJsonRef


class IndonesiaMapRenderToolResult(StrictModel):
    summary: str
    map_resource_name: str
    geojson_resource_name: str
    geojson_ref: IndonesiaGeoJsonRef
    title: str
    feature_count: int = Field(gt=0, le=200)
    layers: list[dict[str, Any]]
    extensions: dict[str, Any]


class IndonesiaDecisionReportToolResult(IndonesiaResourceToolResult):
    report_markdown: str = Field(min_length=1, max_length=64_000)
