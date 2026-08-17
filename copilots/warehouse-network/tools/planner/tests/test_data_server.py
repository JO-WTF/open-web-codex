from __future__ import annotations

import asyncio
import json

import pytest
from open_web_codex_provider import ResourceRef, ResourceStore
from supply_chain_planner.data import server as data_server
from supply_chain_planner.network.models import DemandCityRecord, WarehouseRecord
from supply_chain_planner.shared.models import (
    CandidateWarehouseDeltaRef,
    ConfirmedSourceDecision,
    PreparedNetworkInputRef,
    PreparedNetworkResource,
)
from supply_chain_planner.shared.resources import SupplyChainResources

RESOURCE_URI_PREFIX = "supply-chain://resources/"


def _use_store(tmp_path, monkeypatch) -> ResourceStore:
    resources = SupplyChainResources(tmp_path, tmp_path / "profile")
    monkeypatch.setattr(data_server, "_supply_chain_resources", resources)
    monkeypatch.setattr(data_server, "_workspace", lambda _ctx: tmp_path)
    return resources.store


def test_data_server_exposes_only_candidate_delta_composable_tools() -> None:
    tools = asyncio.run(data_server.mcp.list_tools())

    assert [tool.name for tool in tools] == [
        "discover_workspace_sources",
        "inspect_workspace_sources",
        "normalize_network_input",
        "normalize_candidate_delta",
        "derive_normalized_network_input",
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
    normalize = next(tool for tool in tools if tool.name == "normalize_network_input")
    assert normalize.inputSchema["properties"]["country_code"]["pattern"] == (
        "^[A-Za-z]{2}$"
    )
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
    assert ref.server == "supply_chain_data"
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

    with pytest.raises(ValueError, match="country_code_required_iso_alpha2"):
        data_server.normalize_network_input(profile_ref, decisions, "IDN", object())

    normalized = data_server.normalize_network_input(profile_ref, decisions, "ID", object())
    normalized_ref = ResourceRef.model_validate(normalized.structuredContent["resource_ref"])
    assert normalized_ref.server == "supply_chain_data"
    normalized_payload = store.load(normalized_ref)
    assert normalized_payload["country_code"] == "ID"
    assert normalized_payload["state"] == "needs_geography"

    prepared = data_server.prepare_network_geography(
        normalized_ref,
        "admin.json",
        object(),
    )
    prepared_ref = ResourceRef.model_validate(prepared.structuredContent["resource_ref"])
    assert prepared_ref.server == "supply_chain_data"
    prepared_payload = store.load(prepared_ref)
    assert prepared_payload["country_code"] == "ID"
    assert prepared_payload["state"] == "ready"
    assert prepared_payload["demand_cities"][0]["longitude"] == 106.8


def test_candidate_delta_derives_new_snapshot_without_reparsing_base_facts(
    tmp_path, monkeypatch
) -> None:
    (tmp_path / "candidate-delta.csv").write_text(
        "warehouse_id,warehouse_name,warehouse_type,city_id,city_name,longitude,latitude\n"
        "CAND-DELTA,Delta Candidate,center,CITY-1,Jakarta,106.9,-6.3\n",
        encoding="utf-8",
    )
    store = _use_store(tmp_path, monkeypatch)
    source_profile = data_server.inspect_workspace_sources(["candidate-delta.csv"], object())
    assert source_profile.structuredContent is not None
    source_profile_ref = ResourceRef.model_validate(source_profile.structuredContent["resource_ref"])
    candidate_decision = ConfirmedSourceDecision.model_validate(
        {
            "relative_path": "candidate-delta.csv",
            "role": "candidate_warehouse",
            "mappings": [
                {"source_field": "warehouse_id", "target_field": "warehouse_id", "transform": "normalize_identifier"},
                {"source_field": "warehouse_name", "target_field": "warehouse_name", "transform": "trim"},
                {"source_field": "warehouse_type", "target_field": "warehouse_type", "transform": "normalize_warehouse_type"},
                {"source_field": "city_id", "target_field": "city_id", "transform": "normalize_identifier"},
                {"source_field": "city_name", "target_field": "city_name", "transform": "trim"},
                {"source_field": "longitude", "target_field": "longitude", "transform": "parse_decimal"},
                {"source_field": "latitude", "target_field": "latitude", "transform": "parse_decimal"},
            ],
        }
    )
    delta_result = data_server.normalize_candidate_delta(
        source_profile_ref,
        [candidate_decision],
        [],
        object(),
    )
    assert delta_result.structuredContent is not None
    delta_ref = CandidateWarehouseDeltaRef.model_validate(
        delta_result.structuredContent["resource_ref"]
    )

    base = PreparedNetworkResource(
        country_code="ID",
        state="ready",
        demand_cities=[
            DemandCityRecord(
                city_id="CITY-1",
                city_name="Jakarta",
                demand_quantity=10,
                longitude=106.8,
                latitude=-6.2,
            )
        ],
        warehouses=[
            WarehouseRecord(
                warehouse_id="EXISTING-1",
                warehouse_name="Existing",
                warehouse_type="center",
                city_id="CITY-1",
                city_name="Jakarta",
                longitude=106.8,
                latitude=-6.2,
                is_existing=True,
                is_fixed=True,
            ),
            WarehouseRecord(
                warehouse_id="CAND-ORIGINAL",
                warehouse_name="Original Candidate",
                warehouse_type="center",
                city_id="CITY-1",
                city_name="Jakarta",
                longitude=106.8,
                latitude=-6.2,
                is_existing=False,
                is_fixed=False,
            ),
        ],
        current_assignments=[],
        route_quotes=[],
    )
    published_base = store.publish(base.schema_version, base)
    base_ref = PreparedNetworkInputRef(
        server=data_server.DATA_MCP_SERVER_NAME,
        uri=published_base.uri,
        resource_schema=published_base.schema,
    )

    def reject_workspace_parse(*_args, **_kwargs):
        raise AssertionError("derivation must not reparse a Workspace source")

    monkeypatch.setattr(data_server, "read_rows", reject_workspace_parse)
    derived_result = data_server.derive_normalized_network_input(base_ref, delta_ref, object())
    assert derived_result.structuredContent is not None
    derived_ref = ResourceRef.model_validate(derived_result.structuredContent["resource_ref"])
    base_payload = store.load(base_ref)
    derived_payload = store.load(derived_ref)
    assert [item["warehouse_id"] for item in base_payload["warehouses"]] == [
        "EXISTING-1",
        "CAND-ORIGINAL",
    ]
    assert [item["warehouse_id"] for item in derived_payload["warehouses"]] == [
        "CAND-DELTA",
        "CAND-ORIGINAL",
        "EXISTING-1",
    ]
    assert derived_payload["parentResourceRef"] == base_ref.model_dump(mode="json")
