from __future__ import annotations

import pytest
from pydantic import ValidationError

from supply_chain_planner import server
from supply_chain_planner.data_core import build_planning_dataset, inspect_source
from supply_chain_planner.models import PlanningSource, ResourceRef
from supply_chain_planner.resource_store import ResourceStore


def _source() -> PlanningSource:
    city_ids = ("north", "central", "south")
    cities = [
        {
            "city_id": city_id,
            "name": city_id.title(),
            "region": city_id,
            "location": {"latitude": 40 + index, "longitude": -87 + index},
        }
        for index, city_id in enumerate(city_ids)
    ]
    return PlanningSource.model_validate(
        {
            "source_id": "warehouse-network-fixture",
            "market": "US",
            "label": "Test-only warehouse network fixture",
            "source_updated_at": "2026-07-25T16:00:00Z",
            "planning_period": "2026 annual planning units",
            "currency": "USD",
            "service_policy": {"policy_id": "one-day", "max_delivery_seconds": 86400},
            "cities": cities,
            "city_demands": [
                {"city_id": city_id, "demand_date": "2026-06-01", "demand_units": units}
                for city_id, units in zip(city_ids, (35, 40, 25), strict=True)
            ],
            "facilities": [
                {
                    "facility_id": "facility-a",
                    "city_id": "north",
                    "location": cities[0]["location"],
                    "capacity_units": 100,
                    "is_existing": True,
                },
                {
                    "facility_id": "facility-b",
                    "city_id": "central",
                    "location": cities[1]["location"],
                    "capacity_units": 100,
                    "is_existing": True,
                },
            ],
            "warehouse_city_coverage": [
                {"facility_id": "facility-a", "city_id": city_id, "is_current": True}
                for city_id in city_ids
            ],
            "lanes": [
                {
                    "origin_city_id": origin,
                    "destination_city_id": destination,
                    "distance_km": 10,
                    "travel_time_hours": 1,
                    "base_cost_per_unit": "2.00",
                    "distance_cost_per_km_per_unit": "0.01",
                    "currency": "USD",
                }
                for origin in ("north", "central")
                for destination in city_ids
            ],
            "route_provider": "workspace",
            "route_method": "quoted",
        }
    )


@pytest.fixture
def source() -> PlanningSource:
    return _source()


def test_inspection_is_bounded_and_reports_source_scope(source: PlanningSource) -> None:
    inspection = inspect_source(source)
    assert inspection.source_summary.source_id == "warehouse-network-fixture"
    assert inspection.source_summary.city_demand_row_count == 3
    assert inspection.source_summary.demand_units == 100
    assert inspection.source_summary.demand_node_count == 3
    assert inspection.source_summary.lane_count == 6
    assert inspection.source_summary.truncated is False


def test_builds_planning_dataset_and_network_handoff(source: PlanningSource) -> None:
    dataset = build_planning_dataset(source)
    assert dataset.schema_version == "planning-dataset.v2"
    assert dataset.source_summary.demand_units == 100
    assert [item.demand_units for item in dataset.demand_distribution] == [40, 35, 25]
    assert dataset.delivery_baseline.observed_demand_units == 0
    assert dataset.delivery_baseline.unobserved_demand_units == 100
    assert dataset.delivery_baseline.on_time_ratio is None
    assert sum(item.demand_units for item in dataset.network_input.demand_points) == 100
    assert len(dataset.route_entries) == 6
    assert dataset.data_quality.valid is True
    assert any("no observed delivery" in item for item in dataset.data_quality.warnings)


def test_dataset_identity_is_stable_for_same_source(source: PlanningSource) -> None:
    assert build_planning_dataset(source).dataset_id == build_planning_dataset(source).dataset_id


def test_source_contract_rejects_direct_pii_fields() -> None:
    payload = _source().model_dump(mode="json")
    payload["city_demands"][0]["customer_name"] = "not allowed"
    with pytest.raises(ValidationError, match="Extra inputs are not permitted"):
        PlanningSource.model_validate(payload)


def test_source_contract_rejects_candidate_as_current_assignment() -> None:
    payload = _source().model_dump(mode="json")
    payload["facilities"][0]["is_existing"] = False
    with pytest.raises(
        ValidationError, match="current coverage must reference an existing facility"
    ):
        PlanningSource.model_validate(payload)


def test_planner_rejects_historical_dataset_without_current_provenance(tmp_path, monkeypatch):
    dataset = build_planning_dataset(_source())
    store = ResourceStore(tmp_path / "resources")
    monkeypatch.setattr(server, "_resource_store", store)
    published = store.publish("planning-dataset.v2", dataset)
    ref = ResourceRef(uri=published.uri, resource_schema="planning-dataset.v2")
    with pytest.raises(ValueError, match="current provenance"):
        server._load_planning_dataset(ref)


def test_planner_accepts_published_requirement_profile_without_profile_confirmation(
    tmp_path, monkeypatch
) -> None:
    dataset = build_planning_dataset(_source())
    requirement_profile = {
        "schemaVersion": "data_requirement_profile.v2",
        "entities": [{"name": "City", "requiredFields": [{"name": "city_id"}]}],
    }
    payload = dataset.model_dump(mode="json", by_alias=True, exclude_none=True)
    payload.update(
        {
            "schemaVersion": "planning-dataset.v2",
            "contract": {"contractId": "warehouse-network-planning", "version": "1.0.0"},
            "normalization": {
                "profile": requirement_profile,
                "mapping": {"confirmed": True, "mappings": [{"target": "city_id"}]},
                "parameters": {"confirmed": True, "answers": []},
            },
            "dataClassification": "workspace_data",
        }
    )
    store = ResourceStore(tmp_path / "resources")
    monkeypatch.setattr(server, "_resource_store", store)
    published = store.publish("planning-dataset.v2", payload)
    ref = ResourceRef(uri=published.uri, resource_schema="planning-dataset.v2")

    loaded = server._load_planning_dataset(ref)

    assert loaded.normalization["profile"] == requirement_profile
    assert "profile_confirmation" not in loaded.normalization
