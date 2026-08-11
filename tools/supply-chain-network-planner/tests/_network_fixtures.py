from __future__ import annotations

import csv
from dataclasses import dataclass
from pathlib import Path

from supply_chain_planner.matrix import build_haversine_route_matrix
from supply_chain_planner.matrix_models import CostMatrix, CostMatrixRow, RouteCostQuote
from supply_chain_planner.network_models import (
    CurrentAssignmentRecord,
    DemandCityRecord,
    ProvidedRouteFactRecord,
    WarehouseRecord,
)


@dataclass(frozen=True)
class NetworkFixture:
    demand: list[DemandCityRecord]
    warehouses: list[WarehouseRecord]


def indonesia_network_fixture() -> NetworkFixture:
    root = Path(__file__).parents[1] / "examples" / "indonesia-network" / "base"
    demand = [
        DemandCityRecord(
            city_id=row["city_id"],
            city_name=row["city_name"],
            province_id=row["province_id"],
            province_name=row["province_name"],
            demand_quantity=row["demand_quantity"],
            longitude=row["longitude"],
            latitude=row["latitude"],
        )
        for row in _csv_rows(root / "demand-cities.csv")
    ]
    warehouses = [
        WarehouseRecord(
            warehouse_id=row["warehouse_id"],
            warehouse_name=row["warehouse_name"],
            warehouse_type=row["warehouse_type"],
            city_id=row["city_id"],
            city_name=row["city_name"],
            province_id=row["province_id"],
            province_name=row["province_name"],
            longitude=row["longitude"],
            latitude=row["latitude"],
            upstream_center_id=row["upstream_center_id"] or None,
            is_existing=row["is_existing"].lower() == "true",
            is_fixed=row["is_fixed"].lower() == "true",
        )
        for path in ("existing-warehouses.csv", "candidate-warehouses.csv")
        for row in _csv_rows(root / path)
    ]
    return NetworkFixture(demand=demand, warehouses=warehouses)


def indonesia_route_quotes() -> list[RouteCostQuote]:
    path = (
        Path(__file__).parents[1]
        / "examples"
        / "indonesia-network"
        / "base"
        / "route-quotes.csv"
    )
    return [
        RouteCostQuote(
            origin_id=row["origin_id"],
            destination_id=row["destination_id"],
            layer=row["layer"],
            price_per_vehicle=row["price_per_vehicle"],
            currency=row["currency"],
            vehicle_capacity=row["vehicle_capacity"],
        )
        for row in _csv_rows(path)
    ]


def indonesia_provided_route_facts() -> list[ProvidedRouteFactRecord]:
    path = (
        Path(__file__).parents[1]
        / "examples"
        / "indonesia-network"
        / "base"
        / "route-quotes.csv"
    )
    return [
        ProvidedRouteFactRecord(
            origin_id=row["origin_id"],
            destination_id=row["destination_id"],
            destination_name=row["destination_name"],
            layer=row["layer"],
            distance_km=row["distance_km"],
            duration_hours=row["duration_hours"],
            source_method=row["method"],
        )
        for row in _csv_rows(path)
    ]


def indonesia_current_assignments() -> list[CurrentAssignmentRecord]:
    path = (
        Path(__file__).parents[1]
        / "examples"
        / "indonesia-network"
        / "current-coverage-extension"
        / "current-coverage.csv"
    )
    return [
        CurrentAssignmentRecord(
            demand_city_id=row["demand_city_id"],
            serving_warehouse_id=row["serving_warehouse_id"],
            upstream_center_id=row["upstream_center_id"] or None,
        )
        for row in _csv_rows(path)
    ]


def _csv_rows(path: Path) -> list[dict[str, str]]:
    with path.open(encoding="utf-8", newline="") as handle:
        return list(csv.DictReader(handle))


def network_case() -> NetworkFixture:
    return NetworkFixture(
        demand=[
            DemandCityRecord(
                city_id="city-a",
                city_name="City A",
                province_id="province-a",
                province_name="Province A",
                demand_quantity=10,
                longitude=106.8,
                latitude=-6.2,
            ),
            DemandCityRecord(
                city_id="city-b",
                city_name="City B",
                province_id="province-b",
                province_name="Province B",
                demand_quantity=20,
                longitude=110.4,
                latitude=-7.8,
            ),
        ],
        warehouses=[
            WarehouseRecord(
                warehouse_id="center-a",
                warehouse_name="Center A",
                warehouse_type="center",
                city_id="city-a",
                city_name="City A",
                longitude=106.8,
                latitude=-6.2,
                is_existing=True,
                is_fixed=True,
            ),
            WarehouseRecord(
                warehouse_id="cross-b",
                warehouse_name="Cross B",
                warehouse_type="cross_docking",
                city_id="city-b",
                city_name="City B",
                longitude=110.4,
                latitude=-7.8,
                upstream_center_id="center-a",
                is_existing=True,
                is_fixed=True,
            ),
            WarehouseRecord(
                warehouse_id="candidate-c",
                warehouse_name="Candidate C",
                warehouse_type="cross_docking",
                city_id="city-b",
                city_name="City B",
                longitude=110.4,
                latitude=-7.8,
                upstream_center_id="center-a",
                is_existing=False,
                is_fixed=False,
            ),
        ],
    )


def route_matrix(case: NetworkFixture):
    return build_haversine_route_matrix(
        case.demand,
        case.warehouses,
        detour_coefficient=1.2,
        average_speed_kph=40,
    )


def complete_cost_matrix(case: NetworkFixture) -> CostMatrix:
    route_costs = {
        ("center-a", "city-a"): 10.0,
        ("center-a", "city-b"): 100.0,
        ("cross-b", "city-a"): 100.0,
        ("cross-b", "city-b"): 10.0,
        ("candidate-c", "city-a"): 20.0,
        ("candidate-c", "city-b"): 20.0,
    }
    return CostMatrix(
        currency="IDR",
        warehouse_scope="all_warehouses",
        rows=[
            CostMatrixRow(
                origin_id=origin,
                destination_id=destination,
                layer="last_mile",
                cost_per_demand_unit=price,
                currency="IDR",
                source="quote",
                tool_version="quote-unit-cost.v1",
                quote_price_per_vehicle=price,
                quote_vehicle_capacity=1,
            )
            for (origin, destination), price in route_costs.items()
        ]
        + [
            CostMatrixRow(
                origin_id="center-a",
                destination_id=destination,
                layer="linehaul",
                cost_per_demand_unit=price,
                currency="IDR",
                source="quote",
                tool_version="quote-unit-cost.v1",
                quote_price_per_vehicle=price,
                quote_vehicle_capacity=1,
            )
            for destination, price in {"cross-b": 5.0, "candidate-c": 2.0}.items()
        ],
    )
