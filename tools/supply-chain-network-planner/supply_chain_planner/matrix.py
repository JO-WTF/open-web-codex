"""Deterministic route and cost matrix builders for composable cases."""

from __future__ import annotations

import math
from typing import Any, Literal

from .geo import haversine_km
from .matrix_models import (
    CostCalculationPolicy,
    CostMatrix,
    CostMatrixRow,
    DemandUnitCostRule,
    NetworkLayer,
    RouteCostQuote,
    RouteFactProvenance,
    RouteMatrix,
    RouteMatrixPlan,
    RouteMatrixRow,
    WarehouseScope,
)
from .network_models import (
    DemandCityRecord,
    ProvidedRouteFactRecord,
    RouteQuoteRecord,
    WarehouseRecord,
)

HAVERSINE_TOOL_VERSION = "haversine.v1"
QUOTE_UNIT_COST_TOOL_VERSION = "quote-unit-cost.v1"
DISTANCE_UNIT_COST_TOOL_VERSION = "distance-unit-cost.v1"
PROVIDED_INPUT_TOOL_VERSION = "provided-input.v1"


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
    expected = _expected_route_pairs(demand_cities, warehouses)
    return RouteMatrixPlan(
        origin_count=len({origin for origin, _, _ in expected}),
        destination_count=len({destination for _, destination, _ in expected}),
        route_count=len(expected),
        method=method,
        detour_coefficient=detour_coefficient,
        average_speed_kph=average_speed_kph,
        estimated_billable_calls=(len(expected) if method == "navigation" else 0),
    )


def build_haversine_route_matrix(
    demand_cities: list[DemandCityRecord],
    warehouses: list[WarehouseRecord],
    detour_coefficient: float,
    average_speed_kph: float,
) -> RouteMatrix:
    return build_route_matrix_with_reuse(
        demand_cities,
        warehouses,
        [],
        detour_coefficient,
        average_speed_kph,
        warehouse_scope="all_warehouses",
    )


def build_provided_route_matrix(
    demand_cities: list[DemandCityRecord],
    warehouses: list[WarehouseRecord],
    provided_route_facts: list[ProvidedRouteFactRecord],
    *,
    warehouse_scope: WarehouseScope,
) -> RouteMatrix:
    """Materialize exact uploaded distance/duration facts for expected route pairs."""

    expected = _expected_route_pairs(demand_cities, warehouses)
    expected_set = set(expected)
    supplied: dict[tuple[str, str, NetworkLayer], ProvidedRouteFactRecord] = {}
    ignored = 0
    source_methods: dict[str, int] = {}
    for fact in provided_route_facts:
        key = (fact.origin_id, fact.destination_id, fact.layer)
        if key not in expected_set:
            ignored += 1
            continue
        if key in supplied:
            raise ValueError(
                f"provided_route_fact_duplicate_pair:{fact.origin_id}:"
                f"{fact.destination_id}:{fact.layer}"
            )
        supplied[key] = fact
        source_methods[fact.source_method] = source_methods.get(fact.source_method, 0) + 1
    missing = sorted(expected_set - set(supplied))
    rows = [
        RouteMatrixRow(
            origin_id=fact.origin_id,
            destination_id=fact.destination_id,
            layer=fact.layer,
            distance_km=fact.distance_km,
            duration_hours=fact.duration_hours,
            method="provided",
            tool_version=PROVIDED_INPUT_TOOL_VERSION,
        )
        for key in expected
        if (fact := supplied.get(key)) is not None
    ]
    return RouteMatrix(
        method="provided",
        warehouse_scope=warehouse_scope,
        rows=rows,
        missing_routes=missing,
        validation={
            "expected_pair_count": len(expected),
            "provided_pair_count": len(rows),
            "ignored_input_pair_count": ignored,
            "missing_pair_count": len(missing),
            "complete": not missing,
            "source_method_counts": dict(sorted(source_methods.items())),
        },
    )


def build_route_matrix_with_reuse(
    demand_cities: list[DemandCityRecord],
    warehouses: list[WarehouseRecord],
    existing_rows: list[RouteMatrixRow],
    detour_coefficient: float,
    average_speed_kph: float,
    *,
    warehouse_scope: WarehouseScope,
) -> RouteMatrix:
    """Reuse exact layered route facts and compute only missing expected pairs."""

    if detour_coefficient <= 0 or average_speed_kph <= 0:
        raise ValueError("route_parameters_must_be_positive")
    expected = _expected_route_pairs(demand_cities, warehouses)
    expected_set = set(expected)
    existing_index: dict[tuple[str, str, NetworkLayer], list[RouteMatrixRow]] = {}
    ignored_prior_rows = 0
    for row in existing_rows:
        key = (row.origin_id, row.destination_id, row.layer)
        if key not in expected_set:
            ignored_prior_rows += 1
            continue
        existing_index.setdefault(key, []).append(row)
    warehouse_by_id = {warehouse.warehouse_id: warehouse for warehouse in warehouses}
    demand_by_id = {demand.city_id: demand for demand in demand_cities}
    rows: list[RouteMatrixRow] = []
    missing: list[tuple[str, str, NetworkLayer]] = []
    reused = 0
    computed = 0
    stale = 0
    for origin, destination, layer in expected:
        origin_warehouse = warehouse_by_id[origin]
        destination_point = (
            demand_by_id[destination] if layer == "last_mile" else warehouse_by_id[destination]
        )
        candidates = existing_index.get((origin, destination, layer), [])
        if len(candidates) > 1:
            raise ValueError(f"route_fact_duplicate_pair:{origin}:{destination}:{layer}")
        exact = [
            row
            for row in candidates
            if _is_exact_haversine_fact(
                row,
                origin_warehouse,
                destination_point,
                detour_coefficient,
                average_speed_kph,
            )
        ]
        if exact:
            rows.append(exact[0])
            reused += 1
            continue
        if candidates:
            stale += 1
        if (
            origin_warehouse.longitude is None
            or origin_warehouse.latitude is None
            or destination_point.longitude is None
            or destination_point.latitude is None
        ):
            missing.append((origin, destination, layer))
            continue
        distance = (
            haversine_km(
                (origin_warehouse.longitude, origin_warehouse.latitude),
                (destination_point.longitude, destination_point.latitude),
            )
            * detour_coefficient
        )
        rows.append(
            RouteMatrixRow(
                origin_id=origin,
                destination_id=destination,
                layer=layer,
                distance_km=round(distance, 6),
                duration_hours=round(distance / average_speed_kph, 6),
                method="haversine",
                tool_version=HAVERSINE_TOOL_VERSION,
                origin_longitude=origin_warehouse.longitude,
                origin_latitude=origin_warehouse.latitude,
                destination_longitude=destination_point.longitude,
                destination_latitude=destination_point.latitude,
                detour_coefficient=detour_coefficient,
                average_speed_kph=average_speed_kph,
            )
        )
        computed += 1
    return RouteMatrix(
        method="haversine",
        warehouse_scope=warehouse_scope,
        rows=rows,
        missing_routes=missing,
        validation={
            "expected_pair_count": len(expected),
            "reused_pair_count": reused,
            "computed_pair_count": computed,
            "stale_pair_count": stale,
            "ignored_prior_row_count": ignored_prior_rows,
            "missing_pair_count": len(missing),
            "last_mile_pair_count": sum(1 for _, _, layer in expected if layer == "last_mile"),
            "linehaul_pair_count": sum(1 for _, _, layer in expected if layer == "linehaul"),
            "detour_coefficient": detour_coefficient,
            "average_speed_kph": average_speed_kph,
        },
    )


def _is_exact_haversine_fact(
    row: RouteMatrixRow,
    origin: WarehouseRecord,
    destination: DemandCityRecord | WarehouseRecord,
    detour_coefficient: float,
    average_speed_kph: float,
) -> bool:
    if (
        origin.longitude is None
        or origin.latitude is None
        or destination.longitude is None
        or destination.latitude is None
    ):
        return False
    raw_distance = (
        haversine_km(
            (origin.longitude, origin.latitude),
            (destination.longitude, destination.latitude),
        )
        * detour_coefficient
    )
    expected_distance = round(raw_distance, 6)
    expected_duration = round(raw_distance / average_speed_kph, 6)
    expected_values = (
        origin.longitude,
        origin.latitude,
        destination.longitude,
        destination.latitude,
        detour_coefficient,
        average_speed_kph,
    )
    actual_values = (
        row.origin_longitude,
        row.origin_latitude,
        row.destination_longitude,
        row.destination_latitude,
        row.detour_coefficient,
        row.average_speed_kph,
    )
    return (
        row.method == "haversine"
        and row.tool_version == HAVERSINE_TOOL_VERSION
        and row.status == "ready"
        and all(value is not None for value in expected_values)
        and all(value is not None for value in actual_values)
        and math.isclose(row.distance_km, expected_distance, rel_tol=0, abs_tol=1e-6)
        and math.isclose(
            row.duration_hours,
            expected_duration,
            rel_tol=0,
            abs_tol=1e-6,
        )
        and all(
            math.isclose(float(actual), float(expected), rel_tol=0, abs_tol=1e-9)
            for actual, expected in zip(actual_values, expected_values, strict=True)
        )
    )


def _expected_route_pairs(
    demand_cities: list[DemandCityRecord],
    warehouses: list[WarehouseRecord],
) -> list[tuple[str, str, NetworkLayer]]:
    if not demand_cities or not warehouses:
        raise ValueError("route_matrix_requires_demand_and_warehouses")
    warehouse_by_id = {warehouse.warehouse_id: warehouse for warehouse in warehouses}
    if len(warehouse_by_id) != len(warehouses):
        raise ValueError("warehouse_id_duplicate")
    pairs: set[tuple[str, str, NetworkLayer]] = {
        (warehouse.warehouse_id, demand.city_id, "last_mile")
        for warehouse in warehouses
        for demand in demand_cities
    }
    for warehouse in warehouses:
        if warehouse.warehouse_type != "cross_docking":
            continue
        if not warehouse.upstream_center_id:
            raise ValueError(f"warehouse_upstream_center_required:{warehouse.warehouse_id}")
        upstream = warehouse_by_id.get(warehouse.upstream_center_id)
        if upstream is None:
            raise ValueError(f"warehouse_upstream_center_missing:{warehouse.warehouse_id}")
        if upstream.warehouse_type != "center":
            raise ValueError(f"warehouse_upstream_must_be_center:{warehouse.warehouse_id}")
        pairs.add((upstream.warehouse_id, warehouse.warehouse_id, "linehaul"))
    return sorted(pairs)


def register_navigation_route_matrix(
    demand_cities: list[DemandCityRecord],
    warehouses: list[WarehouseRecord],
    rows: list[RouteMatrixRow],
    *,
    warehouse_scope: WarehouseScope,
) -> RouteMatrix:
    expected = set(_expected_route_pairs(demand_cities, warehouses))
    warehouse_by_id = {warehouse.warehouse_id: warehouse for warehouse in warehouses}
    demand_by_id = {demand.city_id: demand for demand in demand_cities}
    supplied: set[tuple[str, str, NetworkLayer]] = set()
    provenance: set[tuple[str, str, str]] = set()
    for row in rows:
        key = (row.origin_id, row.destination_id, row.layer)
        if key not in expected:
            raise ValueError(
                f"navigation_route_unknown_pair:{row.origin_id}:{row.destination_id}:{row.layer}"
            )
        if key in supplied:
            raise ValueError(
                f"navigation_route_duplicate_pair:{row.origin_id}:{row.destination_id}:{row.layer}"
            )
        supplied.add(key)
        if row.method != "navigation":
            raise ValueError("navigation_matrix_requires_navigation_method")
        if not row.navigation_provider or not row.navigation_profile:
            raise ValueError("navigation_parameters_unavailable")
        provenance.add((row.navigation_provider, row.navigation_profile, row.tool_version))
        origin = warehouse_by_id.get(row.origin_id)
        destination = (
            demand_by_id.get(row.destination_id)
            if row.layer == "last_mile"
            else warehouse_by_id.get(row.destination_id)
        )
        if (
            origin is None
            or destination is None
            or not _route_endpoints_match(row, origin, destination)
        ):
            raise ValueError(
                f"navigation_route_endpoint_mismatch:{row.origin_id}:{row.destination_id}:{row.layer}"
            )
    if len(provenance) > 1:
        raise ValueError("navigation_route_provenance_conflict")
    missing = sorted(expected - supplied)
    return RouteMatrix(
        method="navigation",
        warehouse_scope=warehouse_scope,
        rows=rows,
        missing_routes=missing,
        validation={"route_count": len(rows), "complete": not missing},
    )


def _route_endpoints_match(
    row: RouteMatrixRow,
    origin: WarehouseRecord,
    destination: DemandCityRecord | WarehouseRecord,
) -> bool:
    actual = (
        row.origin_longitude,
        row.origin_latitude,
        row.destination_longitude,
        row.destination_latitude,
    )
    expected = (
        origin.longitude,
        origin.latitude,
        destination.longitude,
        destination.latitude,
    )
    return (
        all(value is not None for value in actual)
        and all(value is not None for value in expected)
        and all(
            math.isclose(float(left), float(right), rel_tol=0, abs_tol=1e-9)
            for left, right in zip(actual, expected, strict=True)
        )
    )


def validate_route_matrix(
    demand_cities: list[DemandCityRecord],
    warehouses: list[WarehouseRecord],
    matrix: RouteMatrix,
) -> dict[str, Any]:
    expected = set(_expected_route_pairs(demand_cities, warehouses))
    supplied = {(row.origin_id, row.destination_id, row.layer) for row in matrix.rows}
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
    fallback_rule: CostCalculationPolicy | None,
    route_matrix: RouteMatrix | None = None,
    prior_rows: list[CostMatrixRow] | None = None,
    *,
    warehouse_scope: WarehouseScope,
) -> CostMatrix:
    expected = _expected_route_pairs(demand_cities, warehouses)
    expected_set = set(expected)
    quote_index: dict[tuple[str, str, NetworkLayer], RouteCostQuote | RouteQuoteRecord] = {}
    quote_currencies: set[str] = set()
    ignored_quotes = 0
    for quote in route_quotes:
        key = (quote.origin_id, quote.destination_id, quote.layer)
        if key not in expected_set:
            ignored_quotes += 1
            continue
        if key in quote_index:
            raise ValueError(f"cost_quote_duplicate_pair:{key}")
        quote_index[key] = quote
        quote_currencies.add(str(quote.currency).upper())
    policy = None if fallback_rule is None else CostCalculationPolicy.model_validate(fallback_rule)
    rules = _cost_rules_by_layer(policy)
    rule_currencies = {rule.currency for rule in rules.values()}
    currencies = quote_currencies | rule_currencies
    if len(currencies) > 1:
        raise ValueError("cost_currency_mismatch")
    if not currencies:
        raise ValueError("cost_currency_required")
    currency = next(iter(currencies))
    if len(currency) != 3 or not currency.isalpha():
        raise ValueError("cost_currency_required")
    rows: list[CostMatrixRow] = []
    missing: list[tuple[str, str, NetworkLayer]] = []
    route_index: dict[tuple[str, str, NetworkLayer], RouteMatrixRow] = {}
    for route in route_matrix.rows if route_matrix is not None else []:
        if route.status != "ready":
            continue
        key = (route.origin_id, route.destination_id, route.layer)
        if key in route_index:
            raise ValueError(f"route_matrix_duplicate_pair:{key}")
        route_index[key] = route
    prior_index: dict[tuple[str, str, NetworkLayer], list[CostMatrixRow]] = {}
    ignored_prior_rows = 0
    for row in prior_rows or []:
        key = (row.origin_id, row.destination_id, row.layer)
        if key not in expected_set:
            ignored_prior_rows += 1
            continue
        prior_index.setdefault(key, []).append(row)
    reused = 0
    computed = 0
    stale = 0
    for origin, destination, layer in expected:
        key = (origin, destination, layer)
        quote = quote_index.get(key)
        prior_candidates = prior_index.get(key, [])
        if len(prior_candidates) > 1:
            raise ValueError(f"cost_prior_duplicate_pair:{key}")
        if quote is not None:
            if str(quote.currency).upper() != currency:
                raise ValueError("cost_currency_mismatch")
            exact = [row for row in prior_candidates if _is_exact_quoted_cost(row, quote, currency)]
            if exact:
                rows.append(exact[0])
                reused += 1
                continue
            if prior_candidates:
                stale += 1
            rows.append(
                CostMatrixRow(
                    origin_id=origin,
                    destination_id=destination,
                    layer=layer,
                    cost_per_demand_unit=float(quote.price_per_vehicle)
                    / float(quote.vehicle_capacity),
                    currency=currency,
                    source="quote",
                    tool_version=QUOTE_UNIT_COST_TOOL_VERSION,
                    quote_price_per_vehicle=float(quote.price_per_vehicle),
                    quote_vehicle_capacity=float(quote.vehicle_capacity),
                )
            )
            computed += 1
            continue
        rule = rules.get(layer)
        if rule is None:
            missing.append((origin, destination, layer))
            continue
        route = route_index.get(key)
        if route is None:
            missing.append((origin, destination, layer))
            continue
        route_fact = _route_fact_provenance(route)
        if route_fact is None:
            missing.append((origin, destination, layer))
            continue
        exact = [row for row in prior_candidates if _is_exact_calculated_cost(row, route, rule)]
        if exact:
            rows.append(exact[0])
            reused += 1
            continue
        if prior_candidates:
            stale += 1
        rows.append(
            CostMatrixRow(
                origin_id=origin,
                destination_id=destination,
                layer=layer,
                cost_per_demand_unit=(
                    rule.fixed_cost_per_demand_unit
                    + rule.cost_per_km_per_demand_unit * route.distance_km
                ),
                currency=currency,
                source="calculated",
                tool_version=DISTANCE_UNIT_COST_TOOL_VERSION,
                route_distance_km=route.distance_km,
                fixed_cost_per_demand_unit=rule.fixed_cost_per_demand_unit,
                cost_per_km_per_demand_unit=rule.cost_per_km_per_demand_unit,
                route_fact=route_fact,
            )
        )
        computed += 1
    return CostMatrix(
        currency=currency,
        warehouse_scope=warehouse_scope,
        rows=rows,
        missing_routes=missing,
        calculation_rule=policy,
        validation={
            "expected_pair_count": len(expected),
            "reused_pair_count": reused,
            "computed_pair_count": computed,
            "missing_pair_count": len(missing),
            "ignored_quote_count": ignored_quotes,
            "ignored_prior_row_count": ignored_prior_rows,
            "stale_pair_count": stale,
        },
    )


def _cost_rules_by_layer(
    policy: CostCalculationPolicy | None,
) -> dict[NetworkLayer, DemandUnitCostRule]:
    result: dict[NetworkLayer, DemandUnitCostRule] = {}
    for rule in policy.rules if policy is not None else []:
        if rule.layer in result:
            raise ValueError(f"cost_rule_duplicate_layer:{rule.layer}")
        result[rule.layer] = rule
    return result


def _is_exact_quoted_cost(
    row: CostMatrixRow,
    quote: RouteCostQuote | RouteQuoteRecord,
    currency: str,
) -> bool:
    expected_cost = float(quote.price_per_vehicle) / float(quote.vehicle_capacity)
    return (
        row.source == "quote"
        and row.tool_version == QUOTE_UNIT_COST_TOOL_VERSION
        and row.currency == currency
        and row.quote_price_per_vehicle is not None
        and row.quote_vehicle_capacity is not None
        and math.isclose(
            row.cost_per_demand_unit,
            expected_cost,
            rel_tol=0,
            abs_tol=1e-9,
        )
        and math.isclose(
            row.quote_price_per_vehicle,
            float(quote.price_per_vehicle),
            rel_tol=0,
            abs_tol=1e-9,
        )
        and math.isclose(
            row.quote_vehicle_capacity,
            float(quote.vehicle_capacity),
            rel_tol=0,
            abs_tol=1e-9,
        )
    )


def _is_exact_calculated_cost(
    row: CostMatrixRow,
    route: RouteMatrixRow,
    rule: DemandUnitCostRule,
) -> bool:
    route_fact = _route_fact_provenance(route)
    expected_cost = (
        rule.fixed_cost_per_demand_unit + rule.cost_per_km_per_demand_unit * route.distance_km
    )
    return (
        row.source == "calculated"
        and row.tool_version == DISTANCE_UNIT_COST_TOOL_VERSION
        and row.currency == rule.currency
        and row.route_distance_km is not None
        and row.fixed_cost_per_demand_unit is not None
        and row.cost_per_km_per_demand_unit is not None
        and route_fact is not None
        and row.route_fact == route_fact
        and math.isclose(
            row.cost_per_demand_unit,
            expected_cost,
            rel_tol=0,
            abs_tol=1e-9,
        )
        and math.isclose(row.route_distance_km, route.distance_km, rel_tol=0, abs_tol=1e-9)
        and math.isclose(
            row.fixed_cost_per_demand_unit,
            rule.fixed_cost_per_demand_unit,
            rel_tol=0,
            abs_tol=1e-9,
        )
        and math.isclose(
            row.cost_per_km_per_demand_unit,
            rule.cost_per_km_per_demand_unit,
            rel_tol=0,
            abs_tol=1e-9,
        )
    )


def _route_fact_provenance(route: RouteMatrixRow) -> RouteFactProvenance | None:
    coordinates = (
        route.origin_longitude,
        route.origin_latitude,
        route.destination_longitude,
        route.destination_latitude,
    )
    if any(value is None for value in coordinates):
        return None
    return RouteFactProvenance(
        method=route.method,
        tool_version=route.tool_version,
        origin_longitude=float(route.origin_longitude),
        origin_latitude=float(route.origin_latitude),
        destination_longitude=float(route.destination_longitude),
        destination_latitude=float(route.destination_latitude),
        detour_coefficient=route.detour_coefficient,
        average_speed_kph=route.average_speed_kph,
        navigation_provider=route.navigation_provider,
        navigation_profile=route.navigation_profile,
    )
