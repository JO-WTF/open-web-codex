"""Distance, duration and transport-cost matrix contracts."""

from __future__ import annotations

from typing import Literal

from pydantic import BaseModel, ConfigDict, Field


class MatrixModel(BaseModel):
    model_config = ConfigDict(extra="forbid", populate_by_name=True)


RouteMethod = Literal["haversine", "navigation", "provided"]
NetworkLayer = Literal["linehaul", "last_mile"]
WarehouseScope = Literal["existing_only", "all_warehouses"]


class RouteMatrixPlan(MatrixModel):
    schema_version: Literal["route_matrix_plan.v2"] = "route_matrix_plan.v2"
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
    layer: NetworkLayer
    distance_km: float = Field(ge=0)
    duration_hours: float = Field(ge=0)
    method: RouteMethod
    tool_version: str = Field(min_length=1, max_length=128)
    status: Literal["ready", "unreachable", "error"] = "ready"
    origin_longitude: float | None = Field(default=None, ge=-180, le=180)
    origin_latitude: float | None = Field(default=None, ge=-90, le=90)
    destination_longitude: float | None = Field(default=None, ge=-180, le=180)
    destination_latitude: float | None = Field(default=None, ge=-90, le=90)
    detour_coefficient: float | None = Field(default=None, gt=0)
    average_speed_kph: float | None = Field(default=None, gt=0)
    navigation_provider: str | None = Field(default=None, min_length=1, max_length=128)
    navigation_profile: str | None = Field(default=None, min_length=1, max_length=128)


class RouteMatrix(MatrixModel):
    schema_version: Literal["route_matrix.v2"] = "route_matrix.v2"
    method: RouteMethod
    rows: list[RouteMatrixRow] = Field(default_factory=list)
    missing_routes: list[tuple[str, str, NetworkLayer]] = Field(default_factory=list)
    validation: dict[str, object] = Field(default_factory=dict)


class RouteCostQuote(MatrixModel):
    origin_id: str = Field(min_length=1, max_length=128)
    destination_id: str = Field(min_length=1, max_length=128)
    layer: NetworkLayer
    price_per_vehicle: float = Field(ge=0)
    currency: str = Field(pattern=r"^[A-Z]{3}$")
    vehicle_capacity: int = Field(gt=0)


class RouteFactProvenance(MatrixModel):
    method: RouteMethod
    tool_version: str = Field(min_length=1, max_length=128)
    origin_longitude: float = Field(ge=-180, le=180)
    origin_latitude: float = Field(ge=-90, le=90)
    destination_longitude: float = Field(ge=-180, le=180)
    destination_latitude: float = Field(ge=-90, le=90)
    detour_coefficient: float | None = Field(default=None, gt=0)
    average_speed_kph: float | None = Field(default=None, gt=0)
    navigation_provider: str | None = Field(default=None, min_length=1, max_length=128)
    navigation_profile: str | None = Field(default=None, min_length=1, max_length=128)


class CostMatrixRow(MatrixModel):
    origin_id: str = Field(min_length=1, max_length=128)
    destination_id: str = Field(min_length=1, max_length=128)
    layer: NetworkLayer
    cost_per_demand_unit: float = Field(ge=0)
    currency: str = Field(pattern=r"^[A-Z]{3}$")
    source: Literal["quote", "calculated"]
    tool_version: str = Field(min_length=1, max_length=128)
    quote_price_per_vehicle: float | None = Field(default=None, ge=0)
    quote_vehicle_capacity: float | None = Field(default=None, gt=0)
    route_distance_km: float | None = Field(default=None, ge=0)
    fixed_cost_per_demand_unit: float | None = Field(default=None, ge=0)
    cost_per_km_per_demand_unit: float | None = Field(default=None, ge=0)
    route_fact: RouteFactProvenance | None = None


class DemandUnitCostRule(MatrixModel):
    """Explicit cost units for one route layer."""

    layer: NetworkLayer
    currency: str = Field(pattern=r"^[A-Z]{3}$")
    fixed_cost_per_demand_unit: float = Field(ge=0)
    cost_per_km_per_demand_unit: float = Field(ge=0)


class CostCalculationPolicy(MatrixModel):
    rules: list[DemandUnitCostRule] = Field(min_length=1)


class CostMatrix(MatrixModel):
    schema_version: Literal["cost_matrix.v2"] = "cost_matrix.v2"
    currency: str = Field(pattern=r"^[A-Z]{3}$")
    warehouse_scope: WarehouseScope
    rows: list[CostMatrixRow] = Field(default_factory=list)
    missing_routes: list[tuple[str, str, NetworkLayer]] = Field(default_factory=list)
    calculation_rule: CostCalculationPolicy | None = None
    validation: dict[str, object] = Field(default_factory=dict)
