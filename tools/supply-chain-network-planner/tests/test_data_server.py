from __future__ import annotations

import asyncio
import json

from supply_chain_planner import data_server
from supply_chain_planner.mcp_contracts import ResourceRef
from supply_chain_planner.mcp_resources import bind_runtime
from supply_chain_planner.models import MCP_SERVER_NAME, ConfirmedSourceDecision
from supply_chain_planner.resource_store import ResourceStore

RESOURCE_URI_PREFIX = "supply-chain://resources/"


def _use_store(tmp_path, monkeypatch) -> ResourceStore:
    store = ResourceStore(tmp_path / "profile-state", uri_prefix=RESOURCE_URI_PREFIX)
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
    for tool in tools[1:]:
        assert set(tool.outputSchema["required"]) == {"summary", "resource_ref"}
        assert "resource_name" not in tool.outputSchema["properties"]

    discover_annotations = tools[0].annotations
    assert discover_annotations is not None
    assert discover_annotations.model_dump(by_alias=True, exclude_none=True) == {
        "readOnlyHint": True,
        "destructiveHint": False,
        "idempotentHint": True,
        "openWorldHint": False,
    }
    for tool in tools[1:]:
        annotations = tool.annotations
        assert annotations is not None
        assert annotations.model_dump(by_alias=True, exclude_none=True) == {
            "readOnlyHint": False,
            "destructiveHint": False,
            "idempotentHint": True,
            "openWorldHint": False,
        }
    geography = next(tool for tool in tools if tool.name == "prepare_network_geography")
    assert "admin_level" not in geography.inputSchema["properties"]
    assert set(geography.inputSchema["required"]) == {
        "normalized_input_ref",
        "administrative_catalog_relative_path",
    }


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
        object(),
    )
    prepared_ref = ResourceRef.model_validate(prepared.structuredContent["resource_ref"])
    prepared_payload = store.load(prepared_ref)
    assert prepared_payload["country_code"] == "ID"
    assert prepared_payload["state"] == "ready"
    assert prepared_payload["demand_cities"][0]["longitude"] == 106.8
