"""Composable contracts for network-planning cases.

These models deliberately describe one analysis input at a time. Optional facts such
as current assignments, candidate sites and quotes are represented as optional data,
not as a global readiness gate.
"""

from __future__ import annotations

from typing import Any, Literal

from pydantic import BaseModel, ConfigDict, Field


class CaseModel(BaseModel):
    model_config = ConfigDict(extra="forbid", populate_by_name=True)


class ArtifactRef(CaseModel):
    server_name: str = Field(min_length=1, max_length=128)
    resource_schema: str = Field(min_length=1, max_length=128)
    resource_name: str = Field(pattern=r"^[a-z0-9_.-]{1,200}$")
    content_sha256: str = Field(pattern=r"^[0-9a-f]{64}$")


ReadinessState = Literal["ready", "needs_input", "unavailable", "failed"]
WarehouseType = Literal["center", "cross_docking"]


class DemandCity(CaseModel):
    city_id: str = Field(min_length=1, max_length=128)
    city_name: str = Field(min_length=1, max_length=256)
    province_id: str = Field(min_length=1, max_length=128)
    province_name: str = Field(min_length=1, max_length=256)
    demand_quantity: int = Field(ge=0)
    longitude: float = Field(ge=-180, le=180)
    latitude: float = Field(ge=-90, le=90)


class Warehouse(CaseModel):
    warehouse_id: str = Field(min_length=1, max_length=128)
    warehouse_name: str = Field(min_length=1, max_length=256)
    warehouse_type: WarehouseType
    city_id: str = Field(min_length=1, max_length=128)
    city_name: str = Field(min_length=1, max_length=256)
    longitude: float = Field(ge=-180, le=180)
    latitude: float = Field(ge=-90, le=90)
    upstream_center_id: str | None = Field(default=None, max_length=128)
    is_existing: bool = True
    is_fixed: bool = True


class CurrentAssignment(CaseModel):
    demand_city_id: str = Field(min_length=1, max_length=128)
    serving_warehouse_id: str = Field(min_length=1, max_length=128)
    upstream_center_id: str | None = Field(default=None, max_length=128)


class DataQualityIssue(CaseModel):
    code: str = Field(min_length=1, max_length=128)
    severity: Literal["info", "warning", "error"]
    business_message: str = Field(min_length=1, max_length=1_000)
    source_ref: ArtifactRef | None = None
    field_name: str | None = Field(default=None, max_length=128)


class DataQualityReport(CaseModel):
    schema_version: Literal["data_quality_report.v1"] = "data_quality_report.v1"
    state: ReadinessState
    issues: list[DataQualityIssue] = Field(default_factory=list)
    ready_entities: list[str] = Field(default_factory=list)
    missing_entities: list[str] = Field(default_factory=list)


class NormalizedNetworkInput(CaseModel):
    schema_version: Literal["normalized_network_input.v1"] = "normalized_network_input.v1"
    country_code: str = Field(pattern=r"^[A-Z]{2,3}$")
    demand: list[DemandCity] = Field(min_length=1)
    existing_warehouses: list[Warehouse] = Field(min_length=1)
    candidate_warehouses: list[Warehouse] = Field(default_factory=list)
    current_assignments: list[CurrentAssignment] = Field(default_factory=list)
    route_quotes: list[dict[str, Any]] = Field(default_factory=list)
    source_refs: list[ArtifactRef] = Field(default_factory=list)
    quality: DataQualityReport
    parameters: dict[str, Any] = Field(default_factory=dict)


class NetworkCase(CaseModel):
    schema_version: Literal["network_case.v1"] = "network_case.v1"
    country_code: str = Field(pattern=r"^[A-Z]{2,3}$")
    input_ref: ArtifactRef
    geography_ref: ArtifactRef | None = None
    route_matrix_ref: ArtifactRef | None = None
    cost_matrix_ref: ArtifactRef | None = None
    current_assignment_ref: ArtifactRef | None = None
    demand: list[DemandCity] = Field(min_length=1)
    warehouses: list[Warehouse] = Field(min_length=1)
    current_assignments: list[CurrentAssignment] = Field(default_factory=list)
    route_quotes: list[dict[str, Any]] = Field(default_factory=list)
