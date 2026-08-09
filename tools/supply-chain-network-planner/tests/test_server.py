from __future__ import annotations

import pytest

from supply_chain_planner import server
from supply_chain_planner.mcp_resources import bind_runtime
from supply_chain_planner.models import ResourceRef
from supply_chain_planner.resource_store import RESOURCE_URI_PREFIX, ResourceStore
from supply_chain_planner.server import MAX_PROFILE_GOAL_CHARS, _normalize_profile_goal


def _use_store(tmp_path, monkeypatch) -> ResourceStore:
    store = ResourceStore(tmp_path / "resources")
    monkeypatch.setattr(
        server,
        "_mcp_resource_runtime",
        bind_runtime(
            tmp_path,
            tmp_path / "profile",
            server.MCP_SERVER_NAME,
            RESOURCE_URI_PREFIX,
            store=store,
        ),
    )
    return store


def test_profile_goal_has_a_bounded_business_summary_limit() -> None:
    assert len(_normalize_profile_goal("x" * MAX_PROFILE_GOAL_CHARS)) == MAX_PROFILE_GOAL_CHARS
    assert len(_normalize_profile_goal("x" * 5368)) == 5368

    with pytest.raises(ValueError, match="1-8000"):
        _normalize_profile_goal("x" * (MAX_PROFILE_GOAL_CHARS + 1))


def test_profile_goal_rejects_blank_business_summary() -> None:
    with pytest.raises(ValueError, match="1-8000"):
        _normalize_profile_goal("  \n  ")


def test_requirement_profile_is_published_without_a_confirmation_request(
    tmp_path, monkeypatch
) -> None:
    _use_store(tmp_path, monkeypatch)

    result = server.publish_data_requirement_profile(
        "Optimize warehouse coverage for the current demand"
    )

    assert result.structuredContent is not None
    payload = server._store().load_uri(result.structuredContent["resource_ref"]["uri"])
    assert payload["schemaVersion"] == "data_requirement_profile.v2"
    assert payload["inputRequest"] == {"kind": "data_requirement", "status": "published"}
    assert "confirmation" not in result.content[0].text.lower()


def test_input_gap_loads_data_agent_resource_references(tmp_path, monkeypatch) -> None:
    store = _use_store(tmp_path, monkeypatch)

    profile_result = server.publish_data_requirement_profile("Optimize warehouse coverage")
    assert profile_result.structuredContent is not None
    profile_ref = ResourceRef.model_validate(profile_result.structuredContent["resource_ref"])

    source = store.publish(
        "source_profile.v1",
        {"schemaVersion": "source_profile.v1", "sources": [{"display_name": "cities.csv"}]},
    )
    mapping = store.publish(
        "mapping_proposal.v1",
        {
            "schemaVersion": "mapping_proposal.v1",
            "candidates": [{"target_field": "city_id"}],
        },
    )
    source_ref = ResourceRef(uri=source.uri, resource_schema="source_profile.v1")
    mapping_ref = ResourceRef(uri=mapping.uri, resource_schema="mapping_proposal.v1")

    result = server.publish_input_gap(
        profile_ref,
        source_profile_ref=source_ref,
        mapping_proposal_ref=mapping_ref,
    )

    assert result.structuredContent is not None
    payload = store.load_uri(result.structuredContent["resource_ref"]["uri"])
    assert payload["schemaVersion"] == "input_gap.v1"
    assert payload["inputRequest"]["kind"] == "confirm_mapping"
    assert {gap["code"] for gap in payload["gaps"]} == {
        "confirm_mapping",
        "answer_parameters",
    }


def test_planner_accepts_normalized_data_agent_ref_at_network_boundary(
    tmp_path, monkeypatch
) -> None:
    store = _use_store(tmp_path, monkeypatch)
    published = store.publish(
        "normalized_network_input.v1",
        {
            "schemaVersion": "normalized_network_input.v1",
            "country_code": "ID",
            "demand": [
                {
                    "city_id": "city-a",
                    "city_name": "Jakarta",
                    "province_id": "province-a",
                    "province_name": "Jakarta",
                    "demand_quantity": 10,
                    "longitude": 106.8,
                    "latitude": -6.2,
                }
            ],
            "existing_warehouses": [
                {
                    "warehouse_id": "warehouse-a",
                    "warehouse_name": "Jakarta Center",
                    "warehouse_type": "center",
                    "city_id": "city-a",
                    "city_name": "Jakarta",
                    "longitude": 106.8,
                    "latitude": -6.2,
                    "is_existing": True,
                    "is_fixed": True,
                }
            ],
            "quality": {
                "state": "ready",
                "issues": [],
                "ready_entities": ["demand", "existing_warehouses"],
                "missing_entities": [],
            },
        },
    )
    ref = ResourceRef(
        uri=published.uri,
        resource_schema="normalized_network_input.v1",
    )

    case = server._case_from_input_ref(ref)

    assert case.country_code == "ID"
    assert case.input_ref.server_name == "supply_chain"
    assert case.input_ref.resource_name == published.resource_id
    assert len(case.input_ref.content_sha256) == 64
