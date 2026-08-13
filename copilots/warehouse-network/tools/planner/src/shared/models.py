"""Typed public contracts for supply-chain network planning."""

from __future__ import annotations

from decimal import Decimal
from typing import Annotated, Literal

from pydantic import BaseModel, ConfigDict, Field, model_validator
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
)
from supply_chain_planner.network.optimization_models import (
    CityAssignmentChange,
    CoverageComparison,
    CoverageMetricSummary,
)
from supply_chain_planner.resources.contracts import ResourceRef as _ResourceRef

MCP_SERVER_NAME = "supply_chain"


class StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid", populate_by_name=True)


class PreparedNetworkResource(StrictModel):
    """Typed Data-to-Network Resource payload owned by supply_chain."""

    schema_version: Literal["normalized_network_input.v1"] = Field(
        default="normalized_network_input.v1",
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


class DataAgentResourceToolResult(StrictModel):
    summary: str
    resource_ref: _ResourceRef


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
    uncovered_cities: list[UncoveredCitySummary] = Field(max_length=100)
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


class NetworkAssignmentComparisonResourceRef(_ResourceRef):
    resource_schema: Literal["network_assignment_comparison.v2"] = (
        "network_assignment_comparison.v2"
    )


class FacilityChangeAssessmentToolResult(StrictModel):
    """Bounded result of one deterministic facility-change assessment."""

    summary: str = Field(min_length=1, max_length=2048)
    scenario_ref: NetworkScenarioResourceRef
    comparison_ref: NetworkAssignmentComparisonResourceRef
    active_warehouse_count: int = Field(ge=0)
    added_warehouse_ids: list[str] = Field(max_length=256)
    removed_warehouse_ids: list[str] = Field(max_length=256)
    cost: FacilityChangeCostComparison
    coverage: list[CoverageComparison] = Field(max_length=32)
    affected_city_count: int = Field(ge=0)
    affected_city_ids: list[str] = Field(max_length=100)
    affected_city_ids_truncated: bool
    affected_city_changes: list[CityAssignmentChange] = Field(max_length=100)
    affected_city_changes_truncated: bool
    reassigned_city_count: int = Field(ge=0)
    reassigned_city_ids: list[str] = Field(max_length=100)
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
    normalized_input_ref: _ResourceRef
    baseline_ref: _ResourceRef


class NetworkComparisonReportInput(StrictModel):
    """Exact typed inputs for a baseline-versus-plan comparison brief."""

    mode: Literal["comparison"] = "comparison"
    normalized_input_ref: Annotated[
        _ResourceRef,
        Field(description="The exact normalized input used by all report results."),
    ]
    baseline_ref: Annotated[
        _ResourceRef,
        Field(description="The exact baseline used as the comparison before subject."),
    ]
    facility_location_ref: Annotated[
        _ResourceRef,
        Field(description="The exact selected result used as the comparison after subject."),
    ]
    comparison_ref: Annotated[
        _ResourceRef,
        Field(
            description=(
                "The comparison produced from the same exact result referenced by "
                "facility_location_ref and this exact baseline_ref; a semantically "
                "equivalent recomputation is invalid."
            )
        ),
    ]


NetworkReportInput = Annotated[
    NetworkBaselineReportInput | NetworkComparisonReportInput,
    Field(discriminator="mode"),
]
