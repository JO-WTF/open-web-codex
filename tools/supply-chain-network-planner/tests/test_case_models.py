from __future__ import annotations

from supply_chain_planner.case_models import (
    ArtifactRef,
    DataQualityReport,
    DemandCity,
    NormalizedNetworkInput,
    Warehouse,
)


def test_normalized_input_requires_only_demand_and_existing_warehouses() -> None:
    normalized = NormalizedNetworkInput(
        country_code="ID",
        demand=[
            DemandCity(
                city_id="city-1",
                city_name="City 1",
                province_id="province-1",
                province_name="Province 1",
                demand_quantity=4,
                longitude=106.8,
                latitude=-6.2,
            )
        ],
        existing_warehouses=[
            Warehouse(
                warehouse_id="warehouse-1",
                warehouse_name="Warehouse 1",
                warehouse_type="center",
                city_id="city-1",
                city_name="City 1",
                longitude=106.8,
                latitude=-6.2,
            )
        ],
        quality=DataQualityReport(state="ready"),
    )

    assert normalized.candidate_warehouses == []
    assert normalized.current_assignments == []
    assert normalized.route_quotes == []


def test_artifact_refs_keep_server_schema_name_and_hash_together() -> None:
    ref = ArtifactRef(
        server_name="supply_chain_data",
        resource_schema="normalized_network_input.v1",
        resource_name="input-1",
        content_sha256="a" * 64,
    )
    assert ref.model_dump() == {
        "server_name": "supply_chain_data",
        "resource_schema": "normalized_network_input.v1",
        "resource_name": "input-1",
        "content_sha256": "a" * 64,
    }
