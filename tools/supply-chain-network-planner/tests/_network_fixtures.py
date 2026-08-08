from __future__ import annotations

from supply_chain_planner.case_models import ArtifactRef, DemandCity, NetworkCase, Warehouse
from supply_chain_planner.matrix import build_haversine_route_matrix
from supply_chain_planner.matrix_models import CostMatrix, CostMatrixRow


def network_case() -> NetworkCase:
    return NetworkCase(
        country_code="ID",
        input_ref=ArtifactRef(
            server_name="supply_chain_data",
            resource_schema="normalized_network_input.v1",
            resource_name="normalized-network-input-test",
            content_sha256="0" * 64,
        ),
        demand=[
            DemandCity(
                city_id="city-a",
                city_name="City A",
                province_id="province-a",
                province_name="Province A",
                demand_quantity=10,
                longitude=106.8,
                latitude=-6.2,
            ),
            DemandCity(
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
            Warehouse(
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
            Warehouse(
                warehouse_id="cross-b",
                warehouse_name="Cross B",
                warehouse_type="cross_docking",
                city_id="city-b",
                city_name="City B",
                longitude=110.4,
                latitude=-7.8,
                is_existing=True,
                is_fixed=True,
            ),
            Warehouse(
                warehouse_id="candidate-c",
                warehouse_name="Candidate C",
                warehouse_type="cross_docking",
                city_id="city-b",
                city_name="City B",
                longitude=110.4,
                latitude=-7.8,
                is_existing=False,
                is_fixed=False,
            ),
        ],
    )


def route_matrix(case: NetworkCase):
    return build_haversine_route_matrix(
        case.demand,
        case.warehouses,
        detour_coefficient=1.2,
        average_speed_kph=40,
    )


def complete_cost_matrix(case: NetworkCase) -> CostMatrix:
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
        rows=[
            CostMatrixRow(
                origin_id=origin,
                destination_id=destination,
                layer="last_mile",
                cost_per_demand_unit=price,
                currency="IDR",
                source="quote",
            )
            for (origin, destination), price in route_costs.items()
        ],
    )
