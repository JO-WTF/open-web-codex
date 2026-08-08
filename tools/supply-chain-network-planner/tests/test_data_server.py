from __future__ import annotations

import pytest

from supply_chain_planner import data_server
from supply_chain_planner.models import DataAgentRef
from supply_chain_planner.resource_store import ResourceStore
from supply_chain_planner.workspace_intake import discover


def _source_profile(*, flattened: bool = False) -> dict[str, object]:
    source: dict[str, object] = {
        "source_ref": "source-cities",
        "display_name": "cities.csv",
    }
    if flattened:
        source["columns"] = ["name", "latitude"]
    else:
        source["structure"] = {
            "kind": "table",
            "delimiter": ",",
            "columns": ["name", "latitude"],
            "rows": [["Jakarta", "-6.2"]],
        }
    return {
        "schemaVersion": "source_profile.v1",
        "sources": [source],
    }


def _requirement_profile() -> dict[str, object]:
    return {
        "schemaVersion": "data_requirement_profile.v2",
        "entities": [
            {
                "name": "City",
                "requiredFields": [{"name": "name"}, {"name": "latitude"}],
            }
        ],
    }


def _publish_profile(tmp_path, monkeypatch, payload: dict[str, object]) -> DataAgentRef:
    store = ResourceStore(tmp_path / "resources", uri_prefix=data_server.RESOURCE_URI_PREFIX)
    monkeypatch.setattr(data_server, "_resource_store", store)
    published = store.publish("source_profile.v1", payload)
    return DataAgentRef(uri=published.uri, resource_schema="source_profile.v1")


def test_mapping_loads_canonical_source_profile_and_publishes_candidates(
    tmp_path, monkeypatch
) -> None:
    profile_ref = _publish_profile(tmp_path, monkeypatch, _source_profile())

    result = data_server.publish_mapping_proposal(profile_ref, _requirement_profile())

    assert result.structuredContent is not None
    assert result.structuredContent["data_ref"]["resource_schema"] == "mapping_proposal.v1"
    proposal_ref = DataAgentRef.model_validate(result.structuredContent["data_ref"])
    proposal = data_server._store().load_uri(proposal_ref.uri)
    assert proposal["candidates"]


def test_mapping_rejects_profile_with_flattened_source_structure(tmp_path, monkeypatch) -> None:
    profile_ref = _publish_profile(tmp_path, monkeypatch, _source_profile(flattened=True))

    with pytest.raises(ValueError, match="source_profile_structure_missing"):
        data_server.publish_mapping_proposal(profile_ref, _requirement_profile())


def test_mapping_rejects_empty_candidate_result(tmp_path, monkeypatch) -> None:
    profile_ref = _publish_profile(tmp_path, monkeypatch, _source_profile())
    requirement = {
        "schemaVersion": "data_requirement_profile.v2",
        "entities": [
            {
                "name": "Facility",
                "requiredFields": [{"name": "unrelated_field"}],
            }
        ],
    }

    with pytest.raises(ValueError, match="mapping_candidates_empty"):
        data_server.publish_mapping_proposal(profile_ref, requirement)


def test_normalization_accepts_confirmed_entity_mapping_shape() -> None:
    mappings = data_server._canonicalize_mapping_items(
        {
            "City": {
                "source_file": "cities.csv",
                "fields": [
                    {
                        "target_entity": "City",
                        "target_field": "city_id",
                        "source_field": "city_id",
                    }
                ],
            }
        },
        [
            {
                "source_ref": "source-cities",
                "relative_path": "cities.csv",
            }
        ],
    )

    assert mappings == [
        {
            "source_ref": "source-cities",
            "source_field": "city_id",
            "target_entity": "City",
            "target_field": "city_id",
        }
    ]


def test_normalization_accepts_agent_field_mappings_and_source_aliases() -> None:
    mappings = data_server._canonicalize_mapping_items(
        {
            "warehouses": {
                "source": "existing-warehouses.csv",
                "source_refs": ["source-warehouses"],
                "field_mappings": {
                    "id": "warehouse_id",
                    "name": "warehouse_name",
                    "is_existing": "is_existing",
                },
            },
            "demand_points": {
                "source": "demand-cities.csv",
                "fields": {
                    "id": "city_id",
                    "name": "city_name",
                    "demand_weight": "demand_quantity",
                },
            },
        },
        [
            {"source_ref": "source-warehouses", "display_name": "existing-warehouses.csv"},
            {"source_ref": "source-demand", "display_name": "demand-cities.csv"},
        ],
    )

    assert {(item["target_entity"], item["target_field"], item["source_ref"]) for item in mappings} == {
        ("warehouses", "id", "source-warehouses"),
        ("warehouses", "name", "source-warehouses"),
        ("warehouses", "is_existing", "source-warehouses"),
        ("demand_points", "id", "source-demand"),
        ("demand_points", "name", "source-demand"),
        ("demand_points", "demand_weight", "source-demand"),
    }


def test_requirement_entities_compose_into_network_domain_rows() -> None:
    entities = {
        "City": [
            {
                "city_id": "IDN-CITY-001",
                "name": "Jakarta",
                "region": "Jakarta",
                "longitude": "106.78",
                "latitude": "-6.25",
            }
        ],
        "CityDemand": [
            {
                "city_id": "IDN-CITY-001",
                "quantity": "10563",
            }
        ],
        "Facility": [
            {
                "facility_id": "WH-CENTER-JAKARTA",
                "name": "Jakarta Center",
                "city_id": "IDN-CITY-001",
                "existing_or_candidate": "true",
            }
        ],
    }

    demand_rows = data_server._compose_demand_rows(entities)
    facility_rows = data_server._compose_facility_rows(entities)

    assert demand_rows == [
        {
            "city_id": "IDN-CITY-001",
            "name": "Jakarta",
            "region": "Jakarta",
            "longitude": "106.78",
            "latitude": "-6.25",
            "quantity": "10563",
            "city_name": "Jakarta",
            "province_name": "Jakarta",
            "province_id": "Jakarta",
        }
    ]
    assert facility_rows == [
        (
            {
                "facility_id": "WH-CENTER-JAKARTA",
                "name": "Jakarta Center",
                "city_id": "IDN-CITY-001",
                "existing_or_candidate": "true",
                "city_name": "Jakarta",
                "longitude": "106.78",
                "latitude": "-6.25",
            },
            True,
        )
    ]


def test_requirement_entities_accept_runtime_demand_and_warehouse_names() -> None:
    entities = {
        "demand_points": [
            {
                "id": "IDN-CITY-001",
                "name": "Jakarta",
                "province_id": "IDN-PROV-JAKARTA",
                "province_name": "Jakarta",
                "lat": "-6.25",
                "lon": "106.78",
                "demand_weight": "10",
            }
        ],
        "warehouses": [
            {
                "id": "WH-CENTER-JAKARTA",
                "name": "Jakarta Center",
                "city_id": "IDN-CITY-001",
                "city_name": "Jakarta",
                "is_existing": "true",
                "lat": "-6.25",
                "lon": "106.78",
            }
        ],
    }

    demand_rows = data_server._compose_demand_rows(entities)

    assert demand_rows[0]["demand_weight"] == "10"
    assert demand_rows[0]["province_id"] == "IDN-PROV-JAKARTA"


def test_requirement_entities_accept_network_demand_cities_name() -> None:
    entities = {
        "demand_cities": [
            {
                "city_id": "IDN-CITY-001",
                "city_name": "Jakarta",
                "province_id": "IDN-PROV-JAKARTA",
                "province_name": "Jakarta",
                "longitude": "106.78",
                "latitude": "-6.25",
                "demand_quantity": "10",
            }
        ]
    }

    demand_rows = data_server._compose_demand_rows(entities)

    assert demand_rows == [entities["demand_cities"][0]]


def test_mapping_rows_resolves_fields_relative_to_explicit_json_array(tmp_path) -> None:
    (tmp_path / "administrative-areas.json").write_text(
        '{"rows":[{"city_id":"IDN-CITY-001","city_name":"Jakarta"}]}',
        encoding="utf-8",
    )
    source = discover(tmp_path)[0]

    entities = data_server._mapping_rows(
        tmp_path,
        [source["source_ref"]],
        [
            {
                "source_ref": source["source_ref"],
                "source_field": "rows[].city_id",
                "target_entity": "City",
                "target_field": "city_id",
            },
            {
                "source_ref": source["source_ref"],
                "source_field": "rows[].city_name",
                "target_entity": "City",
                "target_field": "name",
            },
        ],
    )

    assert entities["City"] == [
        {
            "_source_ref": source["source_ref"],
            "_row": 0,
            "city_id": "IDN-CITY-001",
            "name": "Jakarta",
        }
    ]
