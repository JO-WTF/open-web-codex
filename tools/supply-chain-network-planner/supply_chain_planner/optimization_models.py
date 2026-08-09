"""Assignment, service, scenario and facility-location contracts."""

from __future__ import annotations

from decimal import Decimal
from typing import Literal

from pydantic import BaseModel, ConfigDict, Field


class OptimizationModel(BaseModel):
    model_config = ConfigDict(extra="forbid", populate_by_name=True)


AssignmentObjective = Literal["min_time", "min_cost"]


class AssignmentRow(OptimizationModel):
    demand_city_id: str
    warehouse_id: str | None
    upstream_center_id: str | None = None
    demand_quantity: Decimal = Field(ge=0)
    distance_km: float | None = Field(default=None, ge=0)
    duration_hours: float | None = Field(default=None, ge=0)
    cost: float | None = Field(default=None, ge=0)
    reason: str | None = None


class AssignmentResult(OptimizationModel):
    schema_version: Literal["network_assignment.v1"] = "network_assignment.v1"
    objective: AssignmentObjective
    rows: list[AssignmentRow] = Field(min_length=1)
    total_demand: Decimal = Field(ge=0)
    unassigned_demand: Decimal = Field(ge=0)


class ServiceMetric(OptimizationModel):
    target_hours: float = Field(gt=0)
    covered_demand: Decimal = Field(ge=0)
    total_demand: Decimal = Field(ge=0)
    coverage_rate: float = Field(ge=0, le=1)


class CostSummary(OptimizationModel):
    schema_version: Literal["network_cost_summary.v1"] = "network_cost_summary.v1"
    currency: str = Field(pattern=r"^[A-Z]{3}$")
    total: float = Field(ge=0)
    linehaul: float = Field(ge=0)
    last_mile: float = Field(ge=0)
    by_warehouse: dict[str, float] = Field(default_factory=dict)
    linehaul_by_warehouse: dict[str, float] = Field(default_factory=dict)
    last_mile_by_warehouse: dict[str, float] = Field(default_factory=dict)
    complete: bool = True
    missing_routes: list[tuple[str, str, str]] = Field(default_factory=list)


class BaselineResult(OptimizationModel):
    schema_version: Literal["network_baseline.v2"] = "network_baseline.v2"
    label: Literal["actual_current", "optimized_existing_footprint"]
    active_warehouse_ids: list[str]
    assignment: AssignmentResult
    service: list[ServiceMetric] = Field(default_factory=list)
    cost: CostSummary | None = None
    notice_code: str | None = None


class WarehouseRelocation(OptimizationModel):
    remove_warehouse_id: str
    add_warehouse_id: str


class ScenarioSpec(OptimizationModel):
    add_warehouse_ids: list[str] = Field(default_factory=list)
    remove_warehouse_ids: list[str] = Field(default_factory=list)
    relocations: list[WarehouseRelocation] = Field(default_factory=list)
    objective: AssignmentObjective
    service_targets: list[float] = Field(default_factory=list)


class ScenarioResult(OptimizationModel):
    schema_version: Literal["network_scenario.v2"] = "network_scenario.v2"
    active_warehouse_ids: list[str]
    assignment: AssignmentResult
    cost: CostSummary | None = None
    service: list[ServiceMetric] = Field(default_factory=list)
    warehouse_changes: dict[str, list[str]] = Field(default_factory=dict)


class ServiceComparison(OptimizationModel):
    target_hours: float = Field(gt=0)
    before_coverage_rate: float = Field(ge=0, le=1)
    after_coverage_rate: float = Field(ge=0, le=1)
    coverage_rate_delta: float = Field(ge=-1, le=1)


class CityAssignmentChange(OptimizationModel):
    demand_city_id: str = Field(min_length=1, max_length=128)
    before_warehouse_id: str | None = None
    after_warehouse_id: str | None = None
    affected: bool
    reassigned: bool
    duration_hours_delta: float | None = None
    cost_per_unit_delta: float | None = None


class AssignmentComparison(OptimizationModel):
    schema_version: Literal["network_assignment_comparison.v1"] = (
        "network_assignment_comparison.v1"
    )
    requested_service_targets: list[float]
    service: list[ServiceComparison]
    before_cost: float | None = Field(default=None, ge=0)
    after_cost: float | None = Field(default=None, ge=0)
    cost_delta: float | None = None
    selected_warehouse_ids: list[str]
    removed_warehouse_ids: list[str]
    affected_city_ids: list[str]
    reassigned_city_ids: list[str]
    city_changes: list[CityAssignmentChange]


class FacilityLocationResult(OptimizationModel):
    schema_version: Literal["facility_location_result.v1"] = "facility_location_result.v1"
    objective_value: float = Field(ge=0)
    active_warehouse_ids: list[str]
    opened_candidate_ids: list[str]
    closed_existing_ids: list[str]
    assignment: AssignmentResult


class PMedianRequest(OptimizationModel):
    number_to_open: int = Field(ge=0)
    fixed_existing_ids: list[str]
    optional_existing_ids: list[str]
    time_limit_seconds: float = Field(default=30, gt=0, le=300)


class ServiceCoverageConstraint(OptimizationModel):
    target_hours: float = Field(gt=0)
    minimum_coverage: float = Field(gt=0, le=1)


class PMedianSolution(OptimizationModel):
    schema_version: Literal["facility_location_solution.v3"] = "facility_location_solution.v3"
    status: Literal["optimal", "feasible", "timeout", "infeasible", "unavailable"]
    active_warehouse_ids: list[str]
    opened_candidate_ids: list[str]
    closed_existing_ids: list[str]
    assignment: AssignmentResult | None = None
    objective_value: float | None = None
    cost: CostSummary | None = None
    best_bound: float | None = None
    service: list[ServiceMetric] = Field(default_factory=list)
    optimality: Literal["proven", "feasible_only", "not_available"]
    message: str | None = None


class ServiceConstrainedRequest(OptimizationModel):
    service_targets: list[ServiceMetric] = Field(min_length=1)
    minimum_coverage: float = Field(gt=0, le=1)
    number_to_open: int = Field(ge=0)


class ServiceConstrainedSolution(OptimizationModel):
    schema_version: Literal["service_constrained_solution.v1"] = "service_constrained_solution.v1"
    status: Literal["optimal", "feasible", "timeout", "infeasible", "unavailable"]
    selected_warehouse_ids: list[str]
    cost: float | None = None
    service: list[ServiceMetric] = Field(default_factory=list)
    optimality: Literal["proven", "feasible_only", "not_available"]
