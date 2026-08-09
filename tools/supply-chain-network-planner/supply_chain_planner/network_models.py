"""Normalized business records stored inside a Network Case."""

from __future__ import annotations

from decimal import Decimal
from typing import Literal

from pydantic import BaseModel, ConfigDict, Field


class NetworkRecord(BaseModel):
    model_config = ConfigDict(extra="forbid")


class DemandCityRecord(NetworkRecord):
    city_id: str = Field(min_length=1, max_length=128)
    city_name: str = Field(min_length=1, max_length=256)
    province_id: str | None = Field(default=None, max_length=128)
    province_name: str | None = Field(default=None, max_length=256)
    demand_quantity: Decimal = Field(ge=0)
    longitude: float | None = Field(default=None, ge=-180, le=180)
    latitude: float | None = Field(default=None, ge=-90, le=90)


class WarehouseRecord(NetworkRecord):
    warehouse_id: str = Field(min_length=1, max_length=128)
    warehouse_name: str = Field(min_length=1, max_length=256)
    warehouse_type: Literal["center", "cross_docking"]
    city_id: str = Field(min_length=1, max_length=128)
    city_name: str = Field(min_length=1, max_length=256)
    province_id: str | None = Field(default=None, max_length=128)
    province_name: str | None = Field(default=None, max_length=256)
    longitude: float | None = Field(default=None, ge=-180, le=180)
    latitude: float | None = Field(default=None, ge=-90, le=90)
    upstream_center_id: str | None = Field(default=None, max_length=128)
    is_existing: bool
    is_fixed: bool | None = None


class CurrentAssignmentRecord(NetworkRecord):
    demand_city_id: str = Field(min_length=1, max_length=128)
    serving_warehouse_id: str = Field(min_length=1, max_length=128)
    upstream_center_id: str | None = Field(default=None, max_length=128)


class RouteQuoteRecord(NetworkRecord):
    origin_id: str = Field(min_length=1, max_length=128)
    destination_id: str = Field(min_length=1, max_length=128)
    layer: Literal["linehaul", "last_mile"]
    price_per_vehicle: Decimal = Field(ge=0)
    currency: str = Field(pattern=r"^[A-Z]{3}$")
    vehicle_capacity: Decimal = Field(gt=0)


class DataQualityIssue(NetworkRecord):
    code: str = Field(min_length=1, max_length=128)
    severity: Literal["info", "warning", "error"]
    business_message: str = Field(min_length=1, max_length=400)
    source_id: str | None = Field(default=None, max_length=64)
    field_name: str | None = Field(default=None, max_length=256)


class NormalizedInputBatch(NetworkRecord):
    demand_cities: list[DemandCityRecord]
    warehouses: list[WarehouseRecord]
    current_assignments: list[CurrentAssignmentRecord]
    route_quotes: list[RouteQuoteRecord]
    issues: list[DataQualityIssue] = Field(default_factory=list)
