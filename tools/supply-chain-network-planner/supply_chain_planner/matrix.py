"""Deterministic route and cost matrix builders for composable cases."""

from __future__ import annotations

from typing import Any, Literal

from .geo import haversine_km
from .matrix_models import (
    CostMatrix,
    CostMatrixRow,
    RouteCostQuote,
    RouteMatrix,
    RouteMatrixPlan,
    RouteMatrixRow,
)
from .network_models import DemandCityRecord, RouteQuoteRecord, WarehouseRecord


def plan_route_matrix(
    demand_cities: list[DemandCityRecord],
    warehouses: list[WarehouseRecord],
    method: Literal["haversine", "navigation", "provided"],
    detour_coefficient: float | None,
    average_speed_kph: float | None,
) -> RouteMatrixPlan:
    if method == "haversine" and (detour_coefficient is None or average_speed_kph is None):
        raise ValueError("haversine_requires_detour_coefficient_and_average_speed")
    if not demand_cities or not warehouses:
        raise ValueError("route_matrix_requires_demand_and_warehouses")
    return RouteMatrixPlan(
        origin_count=len(warehouses),
        destination_count=len(demand_cities),
        route_count=len(warehouses) * len(demand_cities),
        method=method,
        detour_coefficient=detour_coefficient,
        average_speed_kph=average_speed_kph,
        estimated_billable_calls=(
            len(warehouses) * len(demand_cities) if method == "navigation" else 0
        ),
    )


def build_haversine_route_matrix(
    demand_cities: list[DemandCityRecord],
    warehouses: list[WarehouseRecord],
    detour_coefficient: float,
    average_speed_kph: float,
) -> RouteMatrix:
    if detour_coefficient <= 0 or average_speed_kph <= 0:
        raise ValueError("route_parameters_must_be_positive")
    rows: list[RouteMatrixRow] = []
    for warehouse in sorted(warehouses, key=lambda item: item.warehouse_id):
        if warehouse.longitude is None or warehouse.latitude is None:
            raise ValueError(f"warehouse_coordinates_missing:{warehouse.warehouse_id}")
        for demand in sorted(demand_cities, key=lambda item: item.city_id):
            if demand.longitude is None or demand.latitude is None:
                raise ValueError(f"demand_coordinates_missing:{demand.city_id}")
            distance = (
                haversine_km(
                    (warehouse.longitude, warehouse.latitude),
                    (demand.longitude, demand.latitude),
                )
                * detour_coefficient
            )
            rows.append(
                RouteMatrixRow(
                    origin_id=warehouse.warehouse_id,
                    destination_id=demand.city_id,
                    distance_km=round(distance, 6),
                    duration_hours=round(distance / average_speed_kph, 6),
                    method="haversine",
                )
            )
    return RouteMatrix(
        method="haversine",
        rows=rows,
        validation={
            "route_count": len(rows),
            "detour_coefficient": detour_coefficient,
            "average_speed_kph": average_speed_kph,
        },
    )


def register_navigation_route_matrix(
    demand_cities: list[DemandCityRecord],
    warehouses: list[WarehouseRecord],
    rows: list[RouteMatrixRow],
) -> RouteMatrix:
    expected = {
        (warehouse.warehouse_id, demand.city_id)
        for warehouse in warehouses
        for demand in demand_cities
    }
    supplied = {(row.origin_id, row.destination_id) for row in rows}
    missing = sorted(expected - supplied)
    return RouteMatrix(
        method="navigation",
        rows=rows,
        missing_routes=missing,
        validation={"route_count": len(rows), "complete": not missing},
    )


def validate_route_matrix(
    demand_cities: list[DemandCityRecord],
    warehouses: list[WarehouseRecord],
    matrix: RouteMatrix,
) -> dict[str, Any]:
    expected = {
        (warehouse.warehouse_id, demand.city_id)
        for warehouse in warehouses
        for demand in demand_cities
    }
    supplied = {(row.origin_id, row.destination_id) for row in matrix.rows}
    unknown = sorted(supplied - expected)
    duplicate_count = len(matrix.rows) - len(supplied)
    missing = sorted(expected - supplied)
    errors: list[str] = []
    if unknown:
        errors.append(f"unknown_routes:{len(unknown)}")
    if duplicate_count:
        errors.append(f"duplicate_routes:{duplicate_count}")
    return {
        "schema": "route_matrix_validation.v1",
        "valid": not errors and not missing,
        "errors": errors,
        "missing_routes": missing,
        "route_count": len(matrix.rows),
    }


def build_cost_matrix(
    demand_cities: list[DemandCityRecord],
    warehouses: list[WarehouseRecord],
    route_quotes: list[RouteCostQuote | RouteQuoteRecord],
    fallback_rule: dict[str, Any] | None,
    route_matrix: RouteMatrix | None = None,
) -> CostMatrix:
    currency = str((fallback_rule or {}).get("currency") or "IDR").upper()
    if len(currency) != 3:
        raise ValueError("cost_currency_required")
    expected = [
        (warehouse.warehouse_id, demand.city_id, "last_mile")
        for warehouse in warehouses
        for demand in demand_cities
    ]
    quote_index = {
        (quote.origin_id, quote.destination_id, quote.layer): quote for quote in route_quotes
    }
    rows: list[CostMatrixRow] = []
    missing: list[tuple[str, str, str]] = []
    price_per_km = None if fallback_rule is None else fallback_rule.get("price_per_km")
    fixed_price = None if fallback_rule is None else fallback_rule.get("fixed_price")
    for origin, destination, layer in expected:
        quote = quote_index.get((origin, destination, layer))
        if quote is not None:
            rows.append(
                CostMatrixRow(
                    origin_id=origin,
                    destination_id=destination,
                    layer=layer,
                    cost_per_demand_unit=quote.price_per_vehicle / quote.vehicle_capacity,
                    currency=quote.currency,
                    source="quote",
                )
            )
            continue
        if price_per_km is None:
            missing.append((origin, destination, layer))
            continue
        if float(price_per_km) < 0 or float(fixed_price or 0) < 0:
            raise ValueError("cost_fallback_rule_must_be_non_negative")
        if route_matrix is None:
            raise ValueError("cost_fallback_requires_route_matrix")
        route = next(
            (
                item
                for item in route_matrix.rows
                if item.origin_id == origin and item.destination_id == destination
            ),
            None,
        )
        if route is None:
            missing.append((origin, destination, layer))
            continue
        rows.append(
            CostMatrixRow(
                origin_id=origin,
                destination_id=destination,
                layer=layer,
                cost_per_demand_unit=float(fixed_price or 0)
                + float(price_per_km) * route.distance_km,
                currency=currency,
                source="calculated",
            )
        )
    # Linehaul quotes are independent of the warehouse-to-demand route matrix:
    # their destination is a cross-docking warehouse rather than a demand city.
    # Preserve them as first-class cost rows instead of dropping them silently.
    for quote in route_quotes:
        if quote.layer != "linehaul":
            continue
        key = (quote.origin_id, quote.destination_id, quote.layer)
        if not any((row.origin_id, row.destination_id, row.layer) == key for row in rows):
            rows.append(
                CostMatrixRow(
                    origin_id=quote.origin_id,
                    destination_id=quote.destination_id,
                    layer=quote.layer,
                    cost_per_demand_unit=quote.price_per_vehicle / quote.vehicle_capacity,
                    currency=quote.currency,
                    source="quote",
                )
            )
    return CostMatrix(
        currency=currency,
        rows=rows,
        missing_routes=missing,
        calculation_rule=fallback_rule,
    )
