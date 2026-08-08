"""Distance, duration and transport-cost matrix contracts."""

from __future__ import annotations

from typing import Literal
from uuid import UUID

from pydantic import BaseModel, ConfigDict, Field


class MatrixModel(BaseModel):
    model_config = ConfigDict(extra="forbid", populate_by_name=True)


RouteMethod = Literal["haversine", "navigation", "provided"]
NetworkLayer = Literal["linehaul", "last_mile"]
WarehouseScope = Literal["existing_only", "all_warehouses"]


class RouteMatrixPlan(MatrixModel):
    schema_version: Literal["route_matrix_plan.v1"] = "route_matrix_plan.v1"
    case_id: UUID | None = None
    origin_count: int = Field(ge=1)
    destination_count: int = Field(ge=1)
    route_count: int = Field(ge=1)
    method: RouteMethod
    detour_coefficient: float | None = Field(default=None, gt=0)
    average_speed_kph: float | None = Field(default=None, gt=0)
    estimated_billable_calls: int = Field(ge=0)


class RouteMatrixRow(MatrixModel):
    origin_id: str = Field(min_length=1, max_length=128)
    destination_id: str = Field(min_length=1, max_length=128)
    distance_km: float = Field(ge=0)
    duration_hours: float = Field(ge=0)
    method: RouteMethod
    status: Literal["ready", "unreachable", "error"] = "ready"


class RouteMatrix(MatrixModel):
    schema_version: Literal["route_matrix.v1"] = "route_matrix.v1"
    method: RouteMethod
    rows: list[RouteMatrixRow] = Field(min_length=1)
    missing_routes: list[tuple[str, str]] = Field(default_factory=list)
    validation: dict[str, object] = Field(default_factory=dict)


class RouteCostQuote(MatrixModel):
    origin_id: str = Field(min_length=1, max_length=128)
    destination_id: str = Field(min_length=1, max_length=128)
    layer: NetworkLayer = "last_mile"
    price_per_vehicle: float = Field(ge=0)
    currency: str = Field(pattern=r"^[A-Z]{3}$")
    vehicle_capacity: int = Field(default=1, gt=0)


class CostMatrixRow(MatrixModel):
    origin_id: str = Field(min_length=1, max_length=128)
    destination_id: str = Field(min_length=1, max_length=128)
    layer: NetworkLayer
    cost_per_demand_unit: float = Field(ge=0)
    currency: str = Field(pattern=r"^[A-Z]{3}$")
    source: Literal["quote", "calculated"]


class CostMatrix(MatrixModel):
    schema_version: Literal["cost_matrix.v1"] = "cost_matrix.v1"
    currency: str = Field(pattern=r"^[A-Z]{3}$")
    warehouse_scope: WarehouseScope = "all_warehouses"
    rows: list[CostMatrixRow] = Field(default_factory=list)
    missing_routes: list[tuple[str, str, NetworkLayer]] = Field(default_factory=list)
    calculation_rule: dict[str, object] | None = None
