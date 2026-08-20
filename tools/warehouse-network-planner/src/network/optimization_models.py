"""Assignment, service, scenario and facility-location contracts."""

from __future__ import annotations

from decimal import Decimal
from typing import Annotated, Literal

from pydantic import BaseModel, ConfigDict, Field, model_validator
from supply_chain_planner.network.models import PlanningInputIdentity


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


class CoverageMetricSummary(OptimizationModel):
    """Both city-count and demand-weighted service coverage for one target."""

    target_hours: float = Field(gt=0)
    covered_city_count: int = Field(ge=0)
    total_city_count: int = Field(ge=0)
    city_coverage_rate: float = Field(ge=0, le=1)
    covered_demand: Decimal = Field(ge=0)
    total_demand: Decimal = Field(ge=0)
    demand_weighted_coverage_rate: float = Field(ge=0, le=1)


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
    coverage: list[CoverageMetricSummary] = Field(default_factory=list)
    cost: CostSummary | None = None
    notice_code: str | None = None
    input_identity: PlanningInputIdentity


class WarehouseRelocation(OptimizationModel):
    remove_warehouse_id: str
    add_warehouse_id: str


class ScenarioSpec(OptimizationModel):
    add_warehouse_ids: list[str] = Field(default_factory=list)
    remove_warehouse_ids: list[str] = Field(default_factory=list)
    relocations: list[WarehouseRelocation] = Field(default_factory=list)
    objective: AssignmentObjective
    service_targets: list[float] = Field(default_factory=list)


class WarehouseChanges(OptimizationModel):
    added: list[str] = Field(default_factory=list)
    removed: list[str] = Field(default_factory=list)


class ScenarioResult(OptimizationModel):
    schema_version: Literal["network_scenario.v2"] = "network_scenario.v2"
    active_warehouse_ids: list[str]
    assignment: AssignmentResult
    cost: CostSummary | None = None
    service: list[ServiceMetric] = Field(default_factory=list)
    warehouse_changes: WarehouseChanges = Field(default_factory=WarehouseChanges)
    input_identity: PlanningInputIdentity


class CoverageMetricDelta(OptimizationModel):
    covered_city_count: int
    total_city_count: int
    city_coverage_rate: float = Field(ge=-1, le=1)
    covered_demand: Decimal
    total_demand: Decimal
    demand_weighted_coverage_rate: float = Field(ge=-1, le=1)


class CoverageComparison(OptimizationModel):
    target_hours: float = Field(gt=0)
    before: CoverageMetricSummary
    after: CoverageMetricSummary
    delta: CoverageMetricDelta


class CityAssignmentChange(OptimizationModel):
    demand_city_id: str = Field(min_length=1, max_length=128)
    before_warehouse_id: str | None = None
    after_warehouse_id: str | None = None
    affected: bool
    reassigned: bool
    duration_hours_delta: float | None = None
    cost_per_unit_delta: float | None = None


class AssignmentComparison(OptimizationModel):
    schema_version: Literal["network_assignment_comparison.v2"] = "network_assignment_comparison.v2"
    requested_service_targets: list[float]
    coverage: list[CoverageComparison]
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


class KeepAllExistingWarehousePolicy(OptimizationModel):
    mode: Literal["keep_all_existing"] = "keep_all_existing"


class AllowExistingWarehouseClosurePolicy(OptimizationModel):
    mode: Literal["allow_closure"] = "allow_closure"
    closable_existing_ids: list[str] = Field(min_length=1)


ExistingWarehousePolicy = Annotated[
    KeepAllExistingWarehousePolicy | AllowExistingWarehouseClosurePolicy,
    Field(discriminator="mode"),
]


class ExactOpeningPolicy(OptimizationModel):
    kind: Literal["exact"] = "exact"
    number_to_open: int = Field(ge=0)


class MinimumFeasibleOpeningPolicy(OptimizationModel):
    kind: Literal["minimum_feasible"] = "minimum_feasible"
    maximum_number_to_open: int | None = Field(default=None, ge=0)


OpeningPolicySelection = Annotated[
    ExactOpeningPolicy | MinimumFeasibleOpeningPolicy,
    Field(discriminator="kind"),
]


class PMedianRequest(OptimizationModel):
    opening_policy: OpeningPolicySelection
    existing_warehouse_policy: ExistingWarehousePolicy
    time_limit_seconds: float = Field(default=30, gt=0, le=300)


class ServiceCoverageConstraint(OptimizationModel):
    target_hours: float = Field(gt=0)
    minimum_coverage: float = Field(gt=0, le=1)


class PMedianSolverStage(OptimizationModel):
    kind: Literal["minimum_openings", "minimum_cost"]
    status: Literal["optimal", "feasible", "timeout", "infeasible", "unavailable"]
    optimality: Literal["proven", "feasible_only", "not_available"]
    selected_number_to_open: int | None = Field(default=None, ge=0)
    objective_value: float | None = Field(default=None, ge=0)
    best_bound: float | None = None
    coverage: list[CoverageMetricSummary] = Field(default_factory=list)

    @model_validator(mode="after")
    def validate_stage_outcome(self):
        expected = {
            "optimal": "proven",
            "feasible": "feasible_only",
            "timeout": "not_available",
            "infeasible": "proven",
            "unavailable": "not_available",
        }[self.status]
        if self.optimality != expected:
            raise ValueError("solver_stage_optimality_mismatch")
        if self.status in {"optimal", "feasible"}:
            if self.selected_number_to_open is None or self.objective_value is None:
                raise ValueError("solver_stage_solution_fields_required")
        elif (
            self.selected_number_to_open is not None
            or self.objective_value is not None
            or self.best_bound is not None
            or self.coverage
        ):
            raise ValueError("solver_stage_failure_fields_forbidden")
        return self


class PMedianSolution(OptimizationModel):
    schema_version: Literal["facility_location_solution.v4"] = "facility_location_solution.v4"
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
    opening_policy: OpeningPolicySelection
    selected_number_to_open: int | None = Field(default=None, ge=0)
    minimum_number_to_open_proven: bool = False
    solver_stages: list[PMedianSolverStage] = Field(default_factory=list, max_length=2)
    message: str | None = None
    input_identity: PlanningInputIdentity

    @model_validator(mode="after")
    def validate_selected_count(self):
        if self.selected_number_to_open is not None and self.selected_number_to_open != len(
            self.opened_candidate_ids
        ):
            raise ValueError("selected_number_to_open_mismatch")
        if self.assignment is None and self.selected_number_to_open is not None:
            raise ValueError("selected_number_to_open_requires_assignment")
        if self.assignment is not None and self.selected_number_to_open is None:
            raise ValueError("selected_number_to_open_required")
        if not self.solver_stages:
            raise ValueError("solver_stages_required")
        if self.assignment is not None:
            if (self.status, self.optimality) not in {
                ("optimal", "proven"),
                ("feasible", "feasible_only"),
            }:
                raise ValueError("solution_status_optimality_mismatch")
        else:
            if (self.status, self.optimality) not in {
                ("timeout", "not_available"),
                ("infeasible", "not_available"),
                ("unavailable", "not_available"),
            }:
                raise ValueError("solution_status_optimality_mismatch")
        if self.assignment is None and (
            self.active_warehouse_ids
            or self.opened_candidate_ids
            or self.closed_existing_ids
            or self.cost is not None
            or self.objective_value is not None
            or self.best_bound is not None
            or self.service
        ):
            raise ValueError("unsolved_solution_fields_forbidden")
        if self.minimum_number_to_open_proven:
            first = self.solver_stages[0]
            if not (
                self.opening_policy.kind == "minimum_feasible"
                and first.kind == "minimum_openings"
                and first.status == "optimal"
                and first.optimality == "proven"
            ):
                raise ValueError("minimum_number_proof_invalid")
        return self


class ComparableNetworkView(OptimizationModel):
    """Internal common view for any assignment-bearing before/after result."""

    label: str
    active_warehouse_ids: list[str]
    assignment: AssignmentResult
    cost: CostSummary | None = None
    service: list[ServiceMetric] = Field(default_factory=list)
    notice_code: str | None = None
    status: Literal["optimal", "feasible", "timeout", "infeasible", "unavailable"] | None = None
    optimality: Literal["proven", "feasible_only", "not_available"] | None = None
    objective_value: float | None = None
    best_bound: float | None = None
    input_identity: PlanningInputIdentity


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
