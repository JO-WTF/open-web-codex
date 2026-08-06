from __future__ import annotations

import pytest

from supply_chain_planner import data_server
from supply_chain_planner.models import DataAgentRef
from supply_chain_planner.resource_store import ResourceStore


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
        "schemaVersion": "data_requirement_profile.v1",
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
        "schemaVersion": "data_requirement_profile.v1",
        "entities": [
            {
                "name": "Facility",
                "requiredFields": [{"name": "unrelated_field"}],
            }
        ],
    }

    with pytest.raises(ValueError, match="mapping_candidates_empty"):
        data_server.publish_mapping_proposal(profile_ref, requirement)
