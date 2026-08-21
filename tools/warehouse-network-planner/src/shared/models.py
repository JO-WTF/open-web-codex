"""Typed public contracts for supply-chain network planning."""

from __future__ import annotations

from decimal import Decimal
from typing import Annotated, Any, Literal

from open_web_codex_provider import ResourceRef as _ResourceRef
from pydantic import BaseModel, ConfigDict, Field, RootModel, field_validator, model_validator
from supply_chain_planner.data.mapping import (
    REQUIRED_FIELDS,
    TARGET_ALIASES,
    SourceRole,
    TransformKind,
)
from supply_chain_planner.network.matrix_models import ObservedQuoteMeanCostEvidence, WarehouseScope
from supply_chain_planner.network.models import (
    CurrentAssignmentRecord,
    DataQualityIssue,
    DemandCityRecord,
    PlanningInputIdentity,
    ProvidedRouteFactRecord,
    RouteQuoteRecord,
    WarehouseRecord,
)
from supply_chain_planner.network.optimization_models import (
    AssignmentComparison,
    CityAssignmentChange,
    CostSummary,
    CoverageComparison,
    CoverageMetricSummary,
    OpeningPolicySelection,
    PMedianSolverStage,
)


class StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid", populate_by_name=True)

    @model_validator(mode="after")
    def validate_canonical_warehouse_ids(self):
        values = getattr(self, "warehouse_ids", None)
        if values is not None and (not values or values != sorted(set(values))):
            raise ValueError("warehouse_ids_not_canonical")
        return self


class PreparedNetworkResource(StrictModel):
    """Validated, user-visible Workspace input for one network-planning run."""

    schema_version: Literal["prepared_network_input.v1"] = Field(
        default="prepared_network_input.v1",
        alias="schemaVersion",
    )
    country_code: str = Field(pattern=r"^[A-Z]{2}$")
    state: Literal["ready", "needs_input", "needs_geography"]
    demand_cities: list[DemandCityRecord]
    warehouses: list[WarehouseRecord]
    current_assignments: list[CurrentAssignmentRecord]
    route_quotes: list[RouteQuoteRecord]
    provided_route_facts: list[ProvidedRouteFactRecord] = Field(default_factory=list)
    issues: list[DataQualityIssue] = Field(default_factory=list)
    confirmed_sources: list[ConfirmedSourceDecision] = Field(default_factory=list)
    parent_input_identity: PlanningInputIdentity | None = None


class ConfirmedFieldDecision(StrictModel):
    source_field: str = Field(min_length=1, max_length=256)
    target_field: str = Field(min_length=1, max_length=128)
    transform: TransformKind
    factor: Decimal | None = None


class ConfirmedSourceInputDecision(StrictModel):
    """One caller decision before required-field completeness is resolved."""

    relative_path: str = Field(min_length=1, max_length=1024)
    role: SourceRole
    mappings: list[ConfirmedFieldDecision] = Field(default_factory=list, max_length=64)

    @model_validator(mode="after")
    def validate_role_mapping_shape(self) -> ConfirmedSourceInputDecision:
        if self.role == SourceRole.ADMINISTRATIVE_CATALOG:
            raise ValueError("administrative catalog is prepared by the geography tool")
        if not self.mappings:
            return self
        allowed = set(TARGET_ALIASES[self.role])
        targets = [mapping.target_field for mapping in self.mappings]
        sources = [mapping.source_field for mapping in self.mappings]
        unknown = sorted(set(targets) - allowed)
        if unknown:
            raise ValueError(
                "confirmed target fields are not allowed for "
                f"{self.role.value}: {', '.join(unknown)}; allowed: "
                f"{', '.join(sorted(allowed))}"
            )
        if len(set(targets)) != len(targets):
            raise ValueError("confirmed target fields must be unique")
        if len(set(sources)) != len(sources):
            raise ValueError("confirmed source fields must be unique")
        return self


class ConfirmedSourceDecision(ConfirmedSourceInputDecision):
    """Complete mapping persisted in one prepared network input."""

    @model_validator(mode="after")
    def validate_required_role_mapping(self) -> ConfirmedSourceDecision:
        if not self.mappings:
            return self
        missing = sorted(
            REQUIRED_FIELDS[self.role] - {mapping.target_field for mapping in self.mappings}
        )
        if missing:
            raise ValueError(
                f"confirmed mapping for {self.role.value} is missing required fields: "
                f"{', '.join(missing)}"
            )
        return self


class GeographyOverride(StrictModel):
    entity: Literal["demand", "warehouse"]
    entity_id: str = Field(min_length=1, max_length=128)
    catalog_city_id: str = Field(min_length=1, max_length=128)


class SourceInspectionIdentity(StrictModel):
    """Identity of the exact regular Workspace files inspected by Data."""

    schema_version: Literal["workspace_source_inspection.v1"] = Field(
        default="workspace_source_inspection.v1",
        alias="schemaVersion",
    )
    content_sha256: str = Field(pattern=r"^[0-9a-f]{64}$")
    source_count: int = Field(ge=1, le=500)


class DataSourceRequirement(StrictModel):
    """A source decision that cannot progress without explicit user input."""

    code: Literal[
        "required_fields_missing",
        "source_role_confirmation_required",
        "field_mapping_confirmation_required",
    ]
    relative_path: str = Field(min_length=1, max_length=1024)
    candidate_roles: list[SourceRole] = Field(min_length=1, max_length=5)
    missing_required_fields: list[str] = Field(max_length=16)
    question: str = Field(min_length=1, max_length=400)

    @model_validator(mode="after")
    def validate_canonical_requirement(self) -> DataSourceRequirement:
        if self.candidate_roles != sorted(set(self.candidate_roles), key=lambda item: item.value):
            raise ValueError("candidate_roles_not_canonical")
        if self.missing_required_fields != sorted(set(self.missing_required_fields)):
            raise ValueError("missing_required_fields_not_canonical")
        if self.code == "required_fields_missing" and not self.missing_required_fields:
            raise ValueError("missing_required_fields_required")
        return self


class InspectionLimitCounts(StrictModel):
    """Bounded observed and allowed inspection dimensions."""

    files: int = Field(ge=0, le=500)
    units: int = Field(ge=0, le=500)
    bytes: int = Field(ge=0, le=100 * 1024 * 1024)


class DataInspectionInspected(StrictModel):
    """Successful v2 source-unit inspection."""

    outcome: Literal["inspected"]
    schema_version: Literal["workspace_source_profile.v2"] = Field(alias="schemaVersion")
    summary: str
    next_action: Literal["confirm_sources"]
    retryable: Literal[False]
    source_profile: dict[str, Any]
    inspection_identity: SourceInspectionIdentity
    inspected_relative_paths: list[str] = Field(min_length=1, max_length=64)


class DataInspectionSelectionRequired(StrictModel):
    """Typed bounded request to reduce inspection scope before retrying."""

    outcome: Literal["selection_required"]
    schema_version: Literal["workspace_source_profile.v2"] = Field(alias="schemaVersion")
    summary: str
    code: Literal["inspection_selection_required"]
    next_action: Literal["select_fewer_sources"]
    retryable: Literal[False]
    observed: InspectionLimitCounts
    limit: InspectionLimitCounts


class DataInspectionToolResult(
    RootModel[
        Annotated[
            DataInspectionInspected | DataInspectionSelectionRequired,
            Field(discriminator="outcome"),
        ]
    ]
):
    """Discriminated v2 inspection contract; no global business blockers."""


class CandidateWarehouseSummary(StrictModel):
    warehouse_id: str = Field(min_length=1, max_length=128)
    warehouse_name: str = Field(min_length=1, max_length=256)
    city_name: str = Field(min_length=1, max_length=256)


class DataPreparationToolResult(StrictModel):
    """The only Data-to-Network handoff: one exact Workspace input file."""

    outcome: Literal["prepared", "needs_input"]
    summary: str
    next_action: Literal["handoff", "prepare_geography", "request_user_input"]
    retryable: Literal[False]
    requirements: list[DataSourceRequirement] = Field(max_length=500)
    prepared_input_relative_path: str | None = Field(max_length=1024)
    input_identity: PlanningInputIdentity | None
    state: Literal["ready", "needs_input", "needs_geography"]
    issue_count: int = Field(ge=0)
    issues: list[DataQualityIssue] = Field(max_length=64)
    issues_truncated: bool
    candidate_warehouse_count: int = Field(ge=0)
    candidate_warehouses: list[CandidateWarehouseSummary] = Field(max_length=64)
    candidate_warehouses_truncated: bool

    @model_validator(mode="after")
    def validate_preparation_outcome(self) -> DataPreparationToolResult:
        if self.outcome == "needs_input":
            if (
                self.state != "needs_input"
                or self.next_action != "request_user_input"
                or not self.requirements
                or self.prepared_input_relative_path is not None
                or self.input_identity is not None
            ):
                raise ValueError("blocked_preparation_outcome_invalid")
            return self
        if (
            self.prepared_input_relative_path is None
            or self.input_identity is None
            or self.requirements
        ):
            raise ValueError("prepared_outcome_invalid")
        expected_action = {
            "ready": "handoff",
            "needs_input": "request_user_input",
            "needs_geography": "prepare_geography",
        }[self.state]
        if self.next_action != expected_action:
            raise ValueError("prepared_next_action_invalid")
        return self


class RouteMatrixPreparationToolResult(StrictModel):
    state: Literal["ready"]
    summary: str
    resource_ref: _ResourceRef
    warehouse_ids: list[str] = Field(min_length=1, max_length=256)

    @model_validator(mode="after")
    def validate_state_schema(self) -> RouteMatrixPreparationToolResult:
        if self.resource_ref.resource_schema != "route_matrix.v3":
            raise ValueError("ready requires route_matrix.v3")
        return self


class CostMatrixPlanningToolResult(StrictModel):
    """Bounded cost-matrix result with auditable fallback-policy evidence."""

    summary: str
    resource_ref: _ResourceRef
    input_identity: PlanningInputIdentity
    calculation_rule_source: Literal["explicit", "observed_quote_mean"] | None = None
    calculation_rule_evidence: ObservedQuoteMeanCostEvidence | None = None
    calculation_rule_evidence_path: str | None = Field(default=None, max_length=1024)
    expected_pair_count: int = Field(ge=0)
    reused_pair_count: int = Field(ge=0)
    computed_pair_count: int = Field(ge=0)
    missing_pair_count: int = Field(ge=0)
    warehouse_ids: list[str] = Field(min_length=1, max_length=256)


class PMedianSolutionToolResult(StrictModel):
    """Bounded facility-location result for model and E2E verification."""

    summary: str
    resource_ref: _ResourceRef
    input_identity: PlanningInputIdentity
    status: Literal["optimal", "feasible", "timeout", "infeasible", "unavailable"]
    optimality: Literal["proven", "feasible_only", "not_available"]
    opening_policy: OpeningPolicySelection
    selected_number_to_open: int | None = Field(default=None, ge=0)
    minimum_number_to_open_proven: bool
    solver_stages: list[PMedianSolverStage] = Field(min_length=1, max_length=2)
    active_warehouse_count: int = Field(ge=0)
    opened_candidate_ids: list[str] = Field(max_length=64)
    closed_existing_ids: list[str] = Field(max_length=64)
    cost: CostSummary | None = None
    coverage: list[CoverageMetricSummary] = Field(max_length=32)

    @model_validator(mode="after")
    def validate_solution_contract(self):
        if self.selected_number_to_open is not None and self.selected_number_to_open != len(
            self.opened_candidate_ids
        ):
            raise ValueError("selected_number_to_open_mismatch")
        if self.status in {"optimal", "feasible"}:
            if self.selected_number_to_open is None:
                raise ValueError("selected_number_to_open_required")
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
        if self.status not in {"optimal", "feasible"} and (
            self.selected_number_to_open is not None
            or self.active_warehouse_count
            or self.opened_candidate_ids
            or self.closed_existing_ids
            or self.cost is not None
            or self.coverage
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


class NavigationMatrixRequestToolResult(StrictModel):
    summary: str
    state: Literal["execution_required", "ready"]
    navigation_request_relative_path: str | None = Field(default=None, max_length=1024)
    input_identity: PlanningInputIdentity
    warehouse_scope: WarehouseScope
    warehouse_ids: list[str] = Field(min_length=1, max_length=256)
    route_count: int = Field(ge=0)
    estimated_billable_elements: int = Field(ge=0)


class UncoveredCitySummary(StrictModel):
    demand_city_id: str = Field(min_length=1, max_length=128)
    demand_city_name: str = Field(min_length=1, max_length=256)
    warehouse_id: str | None = Field(default=None, max_length=128)
    warehouse_name: str | None = Field(default=None, max_length=256)
    duration_hours: float | None = Field(default=None, ge=0)
    demand_quantity: Decimal = Field(ge=0)


class NetworkBaselineResourceToolResult(StrictModel):
    summary: str
    resource_ref: _ResourceRef
    coverage_metrics: list[CoverageMetricSummary] = Field(max_length=32)
    detail_target_hours: float = Field(gt=0)
    uncovered_city_count: int = Field(ge=0)
    uncovered_cities: list[UncoveredCitySummary] = Field(max_length=10)
    uncovered_cities_truncated: bool


class FacilityChangeCostComparison(StrictModel):
    """Bounded cost projection for a before-versus-after facility change."""

    currency: str | None = Field(default=None, pattern=r"^[A-Z]{3}$")
    before_total: float | None = Field(default=None, ge=0)
    after_total: float | None = Field(default=None, ge=0)
    delta: float | None = None
    complete: bool


class NetworkScenarioResourceRef(_ResourceRef):
    resource_schema: Literal["network_scenario.v2"] = "network_scenario.v2"


class AssignmentResultResourceRef(_ResourceRef):
    """A solved result that can be rendered as a single coverage map."""

    resource_schema: Literal[
        "network_baseline.v2",
        "network_scenario.v2",
        "facility_location_solution.v4",
    ]


class ComparableNetworkResultRef(_ResourceRef):
    resource_schema: Literal[
        "network_baseline.v2",
        "network_scenario.v2",
        "facility_location_solution.v4",
    ] = Field(description="Comparable network result schema.")


class NetworkPlanComparisonResourceRef(_ResourceRef):
    resource_schema: Literal["network_plan_comparison.v2"] = "network_plan_comparison.v2"


class NetworkPlanComparisonResource(StrictModel):
    """One complete, provenance-bound before-versus-after planning result."""

    schema_version: Literal["network_plan_comparison.v2"] = "network_plan_comparison.v2"
    prepared_input_relative_path: str = Field(min_length=1, max_length=1024)
    input_identity: PlanningInputIdentity
    before_ref: ComparableNetworkResultRef
    after_ref: ComparableNetworkResultRef
    comparison: AssignmentComparison


class FacilityChangeAssessmentToolResult(StrictModel):
    """Bounded result of one deterministic facility-change assessment."""

    summary: str = Field(min_length=1, max_length=2048)
    scenario_ref: NetworkScenarioResourceRef
    plan_comparison_ref: NetworkPlanComparisonResourceRef
    active_warehouse_count: int = Field(ge=0)
    added_warehouse_ids: list[str] = Field(max_length=256)
    removed_warehouse_ids: list[str] = Field(max_length=256)
    cost: FacilityChangeCostComparison
    coverage: list[CoverageComparison] = Field(max_length=32)
    affected_city_count: int = Field(ge=0)
    affected_city_ids: list[str] = Field(max_length=10)
    affected_city_ids_truncated: bool
    affected_city_changes: list[CityAssignmentChange] = Field(max_length=10)
    affected_city_changes_truncated: bool
    reassigned_city_count: int = Field(ge=0)
    reassigned_city_ids: list[str] = Field(max_length=10)
    reassigned_city_ids_truncated: bool


class NetworkFinalArtifactDescriptor(StrictModel):
    """Platform-readable descriptor for one explicit final domain deliverable."""

    artifact_schema: Literal[
        "network_comparison_map_bundle.v2",
        "network_planning_report_markdown.v2",
    ] = Field(alias="schema")
    display_name: str = Field(min_length=1, max_length=256, alias="displayName")
    mime_type: Literal["application/json", "text/markdown"] = Field(alias="mimeType")
    workspace_relative_path: str = Field(
        min_length=1,
        max_length=1024,
        alias="workspaceRelativePath",
    )
    byte_size: int = Field(ge=1, alias="byteSize")


class NetworkFinalArtifactToolResult(StrictModel):
    summary: str = Field(min_length=1, max_length=512)
    artifact: NetworkFinalArtifactDescriptor


class NetworkBaselineReportInput(StrictModel):
    """Exact typed inputs for a single current-network assessment brief."""

    mode: Literal["baseline"] = "baseline"
    prepared_input_relative_path: str = Field(min_length=1, max_length=1024)
    baseline_ref: _ResourceRef


class NetworkComparisonReportInput(StrictModel):
    """Exact typed inputs for a generic before-versus-after comparison brief."""

    mode: Literal["comparison"] = "comparison"
    plan_comparison_ref: Annotated[
        NetworkPlanComparisonResourceRef,
        Field(description="The exact provenance-bound planning comparison to report."),
    ]

    @field_validator("plan_comparison_ref", mode="before")
    @classmethod
    def validate_plan_comparison_ref(cls, value: object) -> NetworkPlanComparisonResourceRef:
        if isinstance(value, _ResourceRef):
            value = value.model_dump(mode="json")
        return NetworkPlanComparisonResourceRef.model_validate(value)


NetworkReportInput = Annotated[
    NetworkBaselineReportInput | NetworkComparisonReportInput,
    Field(discriminator="mode"),
]
