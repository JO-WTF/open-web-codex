"""Typed public contracts for supply-chain network planning."""

from __future__ import annotations

from decimal import Decimal
from typing import Annotated, Literal

from open_web_codex_provider import ResourceRef as _ResourceRef
from pydantic import BaseModel, ConfigDict, Field, field_validator, model_validator
from supply_chain_planner.data.mapping import (
    REQUIRED_FIELDS,
    TARGET_ALIASES,
    SourceRole,
    TransformKind,
)
from supply_chain_planner.network.models import (
    CurrentAssignmentRecord,
    DataQualityIssue,
    DemandCityRecord,
    ProvidedRouteFactRecord,
    RouteQuoteRecord,
    WarehouseRecord,
    PlanningInputIdentity,
)
from supply_chain_planner.network.optimization_models import (
    AssignmentComparison,
    CityAssignmentChange,
    CoverageComparison,
    CoverageMetricSummary,
)


class StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid", populate_by_name=True)


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
    confirmed_sources: list["ConfirmedSourceDecision"] = Field(default_factory=list)
    parent_input_identity: PlanningInputIdentity | None = None


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
    def validate_role_mapping(self) -> ConfirmedSourceDecision:
        if self.role == SourceRole.ADMINISTRATIVE_CATALOG:
            raise ValueError("administrative catalog is prepared by the geography tool")
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
        missing = sorted(REQUIRED_FIELDS[self.role] - set(targets))
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


class DataInspectionToolResult(StrictModel):
    """A Data-Agent-local source-profile Resource used only during preparation."""

    summary: str
    resource_ref: _ResourceRef


class DataPreparationToolResult(StrictModel):
    """The only Data-to-Network handoff: one exact Workspace input file."""

    summary: str
    prepared_input_relative_path: str = Field(min_length=1, max_length=1024)
    input_identity: PlanningInputIdentity
    state: Literal["ready", "needs_input", "needs_geography"]
    issue_count: int = Field(ge=0)


class RouteMatrixPreparationToolResult(StrictModel):
    state: Literal["ready"]
    summary: str
    resource_ref: _ResourceRef

    @model_validator(mode="after")
    def validate_state_schema(self) -> RouteMatrixPreparationToolResult:
        if self.resource_ref.resource_schema != "route_matrix.v2":
            raise ValueError("ready requires route_matrix.v2")
        return self


class NavigationMatrixRequestToolResult(StrictModel):
    summary: str
    state: Literal["execution_required", "ready"]
    navigation_request_relative_path: str | None = Field(default=None, max_length=1024)
    input_identity: PlanningInputIdentity
    warehouse_scope: Literal["existing_only", "all_warehouses"]
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
        "facility_location_solution.v3",
    ]


class ComparableNetworkResultRef(_ResourceRef):
    resource_schema: Literal[
        "network_baseline.v2",
        "network_scenario.v2",
        "facility_location_solution.v3",
    ] = Field(description="Comparable network result schema.")


class NetworkPlanComparisonResourceRef(_ResourceRef):
    resource_schema: Literal["network_plan_comparison.v1"] = "network_plan_comparison.v1"


class NetworkPlanComparisonResource(StrictModel):
    """One complete, provenance-bound before-versus-after planning result."""

    schema_version: Literal["network_plan_comparison.v1"] = "network_plan_comparison.v1"
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
        "network_comparison_map_bundle.v1",
        "network_planning_report_markdown.v1",
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
    """Exact typed inputs for a baseline-versus-plan comparison brief."""

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
