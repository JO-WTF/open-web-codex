"""Typed lifecycle contracts for an authoritative network-planning case."""

from __future__ import annotations

from enum import StrEnum
from typing import Literal
from uuid import UUID

from pydantic import BaseModel, ConfigDict, Field


class CaseContract(BaseModel):
    model_config = ConfigDict(extra="forbid")


class CaseState(StrEnum):
    ACTIVE = "active"
    COMPLETED = "completed"
    FAILED = "failed"
    ARCHIVED = "archived"


class FacetState(StrEnum):
    NOT_REQUIRED = "not_required"
    MISSING = "missing"
    NEEDS_INPUT = "needs_input"
    READY = "ready"
    STALE = "stale"
    UNAVAILABLE = "unavailable"
    FAILED = "failed"


class FacetName(StrEnum):
    REQUIREMENTS = "requirements"
    SOURCES = "sources"
    MAPPING = "mapping"
    NORMALIZED_INPUT = "normalized_input"
    GEOGRAPHY = "geography"
    CURRENT_ASSIGNMENT = "current_assignment"
    ROUTE_MATRIX = "route_matrix"
    COST_MATRIX = "cost_matrix"
    BASELINE = "baseline"
    SCENARIO = "scenario"
    FACILITY_LOCATION = "facility_location"
    REPORT = "report"
    MAP = "map"


class OperationState(StrEnum):
    RUNNING = "running"
    COMPLETED = "completed"
    FAILED = "failed"
    INTERRUPTED = "interrupted"
    TIMEOUT = "timeout"


class AnalysisKind(StrEnum):
    SERVICE_BASELINE = "service_baseline"
    ACTUAL_SERVICE = "actual_service"
    COST_BASELINE = "cost_baseline"
    ACTUAL_COST = "actual_cost"
    SCENARIO = "scenario"
    P_MEDIAN = "p_median"
    SERVICE_CONSTRAINED = "service_constrained"
    REPORT = "report"
    MAP = "map"


class CaseSummary(CaseContract):
    case_id: UUID
    country_code: str = Field(pattern=r"^[A-Z]{2,3}$")
    state: CaseState
    revision: int = Field(ge=1)


class FacetSummary(CaseContract):
    name: FacetName
    state: FacetState
    component_revision: int | None = Field(default=None, ge=1)
    row_count: int | None = Field(default=None, ge=0)
    issue_codes: list[str] = Field(default_factory=list, max_length=20)


class BlockingQuestionSummary(CaseContract):
    code: str = Field(min_length=1, max_length=128)
    business_message: str = Field(min_length=1, max_length=400)
    owner: Literal["data", "network"]


class CaseStatusSummary(CaseContract):
    case_id: UUID
    revision: int = Field(ge=1)
    state: CaseState
    facets: list[FacetSummary]
    blocking_questions: list[BlockingQuestionSummary] = Field(default_factory=list)
    available_actions: list[str] = Field(default_factory=list, max_length=20)


class ToolOperationSummary(CaseContract):
    operation_id: UUID
    operation_kind: str = Field(min_length=1, max_length=128)
    state: OperationState
    reused: bool = False


class OperationLease(CaseContract):
    operation_id: UUID
    case_id: UUID
    operation_kind: str
    input_sha256: str = Field(pattern=r"^[0-9a-f]{64}$")
    reused_component_id: UUID | None = None


class ComponentWrite(CaseContract):
    component_kind: str = Field(min_length=1, max_length=128)
    content_sha256: str = Field(pattern=r"^[0-9a-f]{64}$")
    row_count: int | None = Field(default=None, ge=0)
    metadata_json: str = "{}"
    facet: FacetName
    depends_on: list[UUID] = Field(default_factory=list)


class ComponentSummary(CaseContract):
    component_id: UUID
    component_kind: str
    component_revision: int = Field(ge=1)
    content_sha256: str = Field(pattern=r"^[0-9a-f]{64}$")
    row_count: int | None = Field(default=None, ge=0)


class CaseOperationResult(CaseContract):
    operation: ToolOperationSummary
    components: list[ComponentSummary] = Field(default_factory=list)


class StorageStatus(CaseContract):
    database_bytes: int = Field(ge=0)
    wal_bytes: int = Field(ge=0)
    archived_case_count: int = Field(ge=0)
    over_high_water: bool


class PurgeResult(CaseContract):
    purged_cases: int = Field(ge=0)
    reclaimed_candidate_bytes: int = Field(ge=0)


class SourceSnapshot(CaseContract):
    source_ref: str = Field(min_length=1, max_length=256)
    display_name: str = Field(min_length=1, max_length=256)
    media_type: str = Field(min_length=1, max_length=128)
    content_sha256: str = Field(pattern=r"^[0-9a-f]{64}$")
    byte_size: int = Field(ge=0)
    row_count: int | None = Field(default=None, ge=0)
    metadata_json: str = "{}"


class SourceSummary(CaseContract):
    source_id: UUID
    source_ref: str
    display_name: str
    media_type: str
    content_sha256: str = Field(pattern=r"^[0-9a-f]{64}$")
    byte_size: int = Field(ge=0)
    row_count: int | None = Field(default=None, ge=0)
    source_revision: int = Field(ge=1)


class StoredMappingCandidate(CaseContract):
    candidate_id: str = Field(pattern=r"^[0-9a-f]{64}$")
    source_id: UUID
    source_role: str = Field(min_length=1, max_length=64)
    target_entity: str = Field(min_length=1, max_length=64)
    target_field: str = Field(min_length=1, max_length=128)
    source_field: str = Field(min_length=1, max_length=256)
    transform_json: str
    score: float = Field(ge=0, le=1)
    reason_code: str = Field(min_length=1, max_length=128)


class SelectedMapping(CaseContract):
    candidate_id: str = Field(pattern=r"^[0-9a-f]{64}$")
    source_id: UUID
    source_ref: str
    source_role: str
    target_entity: str
    target_field: str
    source_field: str
    transform_json: str
