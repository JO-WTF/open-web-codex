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
    schema_version: Literal["network_baseline.v1"] = "network_baseline.v1"
    label: Literal["actual_current", "optimized_existing_footprint"]
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
    objective: AssignmentObjective = "min_cost"
    service_targets: list[float] = Field(default_factory=list)


class ScenarioResult(OptimizationModel):
    schema_version: Literal["network_scenario.v1"] = "network_scenario.v1"
    assignment: AssignmentResult
    cost: CostSummary | None = None
    service: list[ServiceMetric] = Field(default_factory=list)
    warehouse_changes: dict[str, list[str]] = Field(default_factory=dict)


class PMedianRequest(OptimizationModel):
    number_to_open: int = Field(ge=0)
    fixed_existing_ids: list[str] = Field(default_factory=list)
    optional_existing_ids: list[str] = Field(default_factory=list)
    time_limit_seconds: float = Field(default=30, gt=0, le=300)


class PMedianSolution(OptimizationModel):
    schema_version: Literal["facility_location_solution.v2"] = "facility_location_solution.v2"
    status: Literal["optimal", "feasible", "timeout", "infeasible", "unavailable"]
    selected_warehouse_ids: list[str]
    assignment: AssignmentResult | None = None
    objective_value: float | None = None
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
