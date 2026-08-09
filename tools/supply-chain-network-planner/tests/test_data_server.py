from __future__ import annotations

import asyncio
import json

from supply_chain_planner import data_server
from supply_chain_planner.mcp_resources import bind_runtime
from supply_chain_planner.models import MCP_SERVER_NAME, ConfirmedSourceDecision, ResourceRef
from supply_chain_planner.resource_store import ResourceStore
from supply_chain_planner.workspace_intake import discover

RESOURCE_URI_PREFIX = "supply-chain://resources/"


def _use_store(tmp_path, monkeypatch) -> ResourceStore:
    store = ResourceStore(tmp_path / "profile-state")
    runtime = bind_runtime(
        tmp_path,
        tmp_path / "profile",
        MCP_SERVER_NAME,
        RESOURCE_URI_PREFIX,
        store=store,
    )
    monkeypatch.setattr(data_server, "_mcp_resource_runtime", runtime)
    monkeypatch.setattr(data_server, "_workspace", lambda _ctx: tmp_path)
    return store


def test_data_server_exposes_only_four_composable_tools() -> None:
    tools = asyncio.run(data_server.mcp.list_tools())

    assert [tool.name for tool in tools] == [
        "discover_workspace_sources",
        "inspect_workspace_sources",
        "normalize_network_input",
        "prepare_network_geography",
    ]


def test_sample_one_inspect_publishes_counts_and_fields_resource(tmp_path, monkeypatch) -> None:
    (tmp_path / "demand-cities.csv").write_text(
        "city_id,city_name,demand_quantity\n"
        + "".join(f"CITY-{index:03d},City {index},{index + 1}\n" for index in range(50)),
        encoding="utf-8",
    )
    (tmp_path / "existing-warehouses.csv").write_text(
        "warehouse_id,warehouse_name,city_id\n"
        + "".join(f"WH-{index:02d},Warehouse {index},CITY-{index:03d}\n" for index in range(11)),
        encoding="utf-8",
    )
    (tmp_path / "administrative-areas.json").write_text(
        json.dumps(
            {
                "rows": [
                    {
                        "city_id": f"CITY-{index:03d}",
                        "province_id": f"PROV-{index % 5}",
                    }
                    for index in range(50)
                ]
            }
        ),
        encoding="utf-8",
    )
    store = _use_store(tmp_path, monkeypatch)

    result = data_server.inspect_workspace_sources(
        [
            "demand-cities.csv",
            "existing-warehouses.csv",
            "administrative-areas.json",
        ],
        object(),
    )

    assert result.structuredContent is not None
    assert set(result.structuredContent["resource_ref"]) == {
        "type",
        "server",
        "uri",
        "resource_schema",
    }
    ref = ResourceRef.model_validate(result.structuredContent["resource_ref"])
    profile = store.load(ref)
    sources = {source["relative_path"]: source for source in profile["sources"]}
    assert sources["demand-cities.csv"]["structure"]["record_count"] == 50
    assert sources["existing-warehouses.csv"]["structure"]["record_count"] == 11
    assert sources["administrative-areas.json"]["structure"]["arrays"][0]["length"] == 50
    assert [item["role"] for item in sources["demand-cities.csv"]["mapping_suggestions"]] == [
        "demand"
    ]
    assert sources["existing-warehouses.csv"]["mapping_suggestions"] == []
    assert "capacity" not in sources["existing-warehouses.csv"]["structure"]["columns"]


def test_confirmed_rows_normalize_then_prepare_geography_with_country(
    tmp_path, monkeypatch
) -> None:
    (tmp_path / "demand.csv").write_text(
        "city_id,city_name,demand_quantity\nCITY-1,Jakarta,50\n",
        encoding="utf-8",
    )
    (tmp_path / "warehouses.csv").write_text(
        "warehouse_id,warehouse_name,warehouse_type,city_id,city_name\n"
        "WH-1,Jakarta Center,center,CITY-1,Jakarta\n",
        encoding="utf-8",
    )
    (tmp_path / "admin.json").write_text(
        json.dumps(
            {
                "country_code": "ID",
                "admin_level": "city",
                "rows": [
                    {
                        "city_id": "CITY-1",
                        "city_name": "Jakarta",
                        "province_id": "PROV-1",
                        "province_name": "Jakarta",
                        "longitude": 106.8,
                        "latitude": -6.2,
                    }
                ],
            }
        ),
        encoding="utf-8",
    )
    store = _use_store(tmp_path, monkeypatch)
    profiled = data_server.inspect_workspace_sources(
        ["demand.csv", "warehouses.csv", "admin.json"], object()
    )
    profile_ref = ResourceRef.model_validate(profiled.structuredContent["resource_ref"])
    decisions = [
        ConfirmedSourceDecision.model_validate(
            {
                "relative_path": "demand.csv",
                "role": "demand",
                "mappings": [
                    {
                        "source_field": field,
                        "target_field": field,
                        "transform": transform,
                    }
                    for field, transform in (
                        ("city_id", "normalize_identifier"),
                        ("city_name", "trim"),
                        ("demand_quantity", "parse_integer"),
                    )
                ],
            }
        ),
        ConfirmedSourceDecision.model_validate(
            {
                "relative_path": "warehouses.csv",
                "role": "existing_warehouse",
                "mappings": [
                    {
                        "source_field": field,
                        "target_field": field,
                        "transform": transform,
                    }
                    for field, transform in (
                        ("warehouse_id", "normalize_identifier"),
                        ("warehouse_name", "trim"),
                        ("warehouse_type", "normalize_warehouse_type"),
                        ("city_id", "normalize_identifier"),
                        ("city_name", "trim"),
                    )
                ],
            }
        ),
    ]

    normalized = data_server.normalize_network_input(profile_ref, decisions, "ID", object())
    normalized_ref = ResourceRef.model_validate(normalized.structuredContent["resource_ref"])
    normalized_payload = store.load(normalized_ref)
    assert normalized_payload["country_code"] == "ID"
    assert normalized_payload["state"] == "needs_geography"

    prepared = data_server.prepare_network_geography(
        normalized_ref,
        "admin.json",
        "city",
        object(),
    )
    prepared_ref = ResourceRef.model_validate(prepared.structuredContent["resource_ref"])
    prepared_payload = store.load(prepared_ref)
    assert prepared_payload["country_code"] == "ID"
    assert prepared_payload["state"] == "ready"
    assert prepared_payload["demand_cities"][0]["longitude"] == 106.8


def test_normalization_accepts_confirmed_entity_mapping_shape() -> None:
    mappings = data_server._canonicalize_mapping_items(
        {
            "City": {
                "relative_path": "cities.csv",
                "fields": [
                    {
                        "target_entity": "City",
                        "target_field": "city_id",
                        "source_field": "city_id",
                    }
                ],
            }
        },
        [{"relative_path": "cities.csv"}],
    )

    assert mappings == [
        {
            "relative_path": "cities.csv",
            "source_field": "city_id",
            "target_entity": "City",
            "target_field": "city_id",
        }
    ]


def test_normalization_accepts_explicit_relative_paths_only() -> None:
    mappings = data_server._canonicalize_mapping_items(
        {
            "warehouses": {
                "relative_path": "existing-warehouses.csv",
                "fields": {
                    "id": "warehouse_id",
                    "name": "warehouse_name",
                    "is_existing": "is_existing",
                },
            },
            "demand_points": {
                "relative_path": "demand-cities.csv",
                "fields": {
                    "id": "city_id",
                    "name": "city_name",
                    "demand_weight": "demand_quantity",
                },
            },
        },
        [
            {"relative_path": "existing-warehouses.csv"},
            {"relative_path": "demand-cities.csv"},
        ],
    )

    assert {
        (item["target_entity"], item["target_field"], item["relative_path"]) for item in mappings
    } == {
        ("warehouses", "id", "existing-warehouses.csv"),
        ("warehouses", "name", "existing-warehouses.csv"),
        ("warehouses", "is_existing", "existing-warehouses.csv"),
        ("demand_points", "id", "demand-cities.csv"),
        ("demand_points", "name", "demand-cities.csv"),
        ("demand_points", "demand_weight", "demand-cities.csv"),
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
        [source["relative_path"]],
        [
            {
                "relative_path": source["relative_path"],
                "source_field": "rows[].city_id",
                "target_entity": "City",
                "target_field": "city_id",
            },
            {
                "relative_path": source["relative_path"],
                "source_field": "rows[].city_name",
                "target_entity": "City",
                "target_field": "name",
            },
        ],
    )

    assert entities["City"] == [
        {
            "_relative_path": source["relative_path"],
            "_row": 0,
            "city_id": "IDN-CITY-001",
            "name": "Jakarta",
        }
    ]
