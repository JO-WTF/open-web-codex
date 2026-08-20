from __future__ import annotations

import asyncio
import json
from types import SimpleNamespace

import pytest
from open_web_codex_provider import ProviderContractError, ResourceRef, ResourceStore
from supply_chain_planner.data import server as data_server
from supply_chain_planner.data.mapping import SourceRole
from supply_chain_planner.shared.models import (
    ConfirmedSourceDecision,
    DataPreparationToolResult,
)
from supply_chain_planner.shared.resources import SupplyChainResources

PREPARED_OUTPUT_DIR = "outputs/warehouse-network/prepared"


def _use_store(tmp_path, monkeypatch) -> ResourceStore:
    resources = SupplyChainResources(tmp_path, tmp_path / "profile")
    monkeypatch.setattr(data_server, "_supply_chain_resources", resources)
    monkeypatch.setattr(data_server, "_workspace", lambda _ctx: tmp_path)
    return resources.store


def _context(workspace) -> SimpleNamespace:
    meta = SimpleNamespace(
        model_extra={"codex/sandbox-state-meta": {"sandboxCwd": workspace.as_uri()}}
    )
    return SimpleNamespace(request_context=SimpleNamespace(meta=meta))


def _decisions() -> list[ConfirmedSourceDecision]:
    return [
        ConfirmedSourceDecision(relative_path="demand.csv", role=SourceRole.DEMAND),
        ConfirmedSourceDecision(
            relative_path="warehouses.csv",
            role=SourceRole.EXISTING_WAREHOUSE,
        ),
    ]


def test_data_server_exposes_workspace_preparation_not_cross_agent_data_resources() -> None:
    tools = {tool.name: tool for tool in asyncio.run(data_server.mcp.list_tools())}

    assert set(tools) == {
        "discover_workspace_sources",
        "inspect_workspace_sources",
        "prepare_network_input",
        "prepare_network_geography",
    }
    assert "normalize_candidate_delta" not in tools
    assert "derive_normalized_network_input" not in tools

    prepare = tools["prepare_network_input"]
    assert set(prepare.inputSchema["required"]) == {
        "source_profile_ref",
        "confirmed_sources",
        "country_code",
        "output_relative_path",
    }
    assert set(prepare.outputSchema["required"]) == {
        "summary",
        "prepared_input_relative_path",
        "input_identity",
        "state",
        "issue_count",
        "candidate_warehouse_count",
        "candidate_warehouses",
        "candidate_warehouses_truncated",
    }
    assert "resource_ref" not in prepare.outputSchema["properties"]

    geography = tools["prepare_network_geography"]
    assert set(geography.inputSchema["required"]) == {
        "prepared_input_relative_path",
        "administrative_catalog_relative_path",
        "output_relative_path",
    }
    assert prepare.annotations is not None
    assert prepare.annotations.model_dump(by_alias=True, exclude_none=True) == {
        "readOnlyHint": False,
        "destructiveHint": False,
        "idempotentHint": False,
        "openWorldHint": False,
    }


def test_source_profile_distinguishes_preview_samples_from_exact_total(
    tmp_path, monkeypatch
) -> None:
    rows = [
        "warehouse_id,warehouse_name,warehouse_type,city_id,city_name,longitude,latitude,"
        "is_existing"
    ]
    rows.extend(
        f"CAND-{index},Candidate {index},cross_docking,CITY-{index},City {index},"
        f"{106 + index / 10:.1f},{-6 - index / 10:.1f},false"
        for index in range(1, 13)
    )
    rows[7] = (
        "WH-CANDIDATE-BALIKPAPAN,Balikpapan Candidate Cross Docking,cross_docking,"
        "IDN-CITY-021,Balikpapan,116.996737,-1.202072,false"
    )
    (tmp_path / "candidate.csv").write_text("\n".join(rows) + "\n", encoding="utf-8")
    store = _use_store(tmp_path, monkeypatch)

    result = data_server.inspect_workspace_sources(["candidate.csv"], object())
    assert result.structuredContent is not None
    profile_ref = ResourceRef.model_validate(result.structuredContent["resource_ref"])
    source = store.load(profile_ref)["sources"][0]
    preview = source["structure"]["preview"]

    assert source["structure"]["record_count"] == 12
    assert preview["preview_sample_count"] == 3
    assert preview["total_count"] == 12
    assert preview["total_count_exact"] is True
    assert "returned_count" not in preview
    assert "Balikpapan" not in json.dumps(preview["rows"])

    with pytest.raises(ProviderContractError, match="generated_output_path_invalid"):
        data_server.prepare_network_input(
            profile_ref,
            [
                ConfirmedSourceDecision(
                    relative_path="candidate.csv",
                    role=SourceRole.CANDIDATE_WAREHOUSE,
                )
            ],
            "ID",
            "candidate-catalog.json",
            _context(tmp_path),
        )

    prepared = data_server.prepare_network_input(
        profile_ref,
        [
            ConfirmedSourceDecision(
                relative_path="candidate.csv",
                role=SourceRole.CANDIDATE_WAREHOUSE,
            )
        ],
        "ID",
        f"{PREPARED_OUTPUT_DIR}/candidate-catalog.json",
        _context(tmp_path),
    )
    assert prepared.candidate_warehouse_count == 12
    assert prepared.candidate_warehouses_truncated is False
    assert any(
        warehouse.warehouse_id == "WH-CANDIDATE-BALIKPAPAN"
        and warehouse.city_name == "Balikpapan"
        for warehouse in prepared.candidate_warehouses
    )


def test_prepare_input_writes_a_complete_auditable_workspace_document(
    tmp_path, monkeypatch
) -> None:
    (tmp_path / "demand.csv").write_text(
        "city_id,city_name,demand_quantity\nCITY-1,Jakarta,50\n",
        encoding="utf-8",
    )
    (tmp_path / "warehouses.csv").write_text(
        "warehouse_id,warehouse_name,warehouse_type,city_id,city_name,is_existing\n"
        "WH-1,Jakarta Center,center,CITY-1,Jakarta,true\n",
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
    ctx = _context(tmp_path)
    profile = data_server.inspect_workspace_sources(
        ["demand.csv", "warehouses.csv", "admin.json"], object()
    )
    assert profile.structuredContent is not None
    profile_ref = ResourceRef.model_validate(profile.structuredContent["resource_ref"])
    assert store.load(profile_ref)["schemaVersion"] == "source_profile.v1"

    with pytest.raises(ValueError, match="country_code_required_iso_alpha2"):
        data_server.prepare_network_input(
            profile_ref,
            _decisions(),
            "IDN",
            f"{PREPARED_OUTPUT_DIR}/prepared-input.json",
            ctx,
        )

    result = data_server.prepare_network_input(
        profile_ref,
        _decisions(),
        "ID",
        f"{PREPARED_OUTPUT_DIR}/prepared-input.json",
        ctx,
    )
    assert isinstance(result, DataPreparationToolResult)
    assert result.state == "needs_geography"
    assert result.prepared_input_relative_path == f"{PREPARED_OUTPUT_DIR}/prepared-input.json"
    payload = json.loads((tmp_path / result.prepared_input_relative_path).read_text())
    assert payload["schemaVersion"] == "prepared_network_input.v1"
    assert all(source["mappings"] for source in payload["confirmed_sources"])
    assert payload["issues"]
    assert result.input_identity.content_sha256

    geography = data_server.prepare_network_geography(
        result.prepared_input_relative_path,
        "admin.json",
        f"{PREPARED_OUTPUT_DIR}/prepared-input-geography.json",
        ctx,
    )
    assert geography.state == "ready"
    enriched = json.loads((tmp_path / geography.prepared_input_relative_path).read_text())
    assert enriched["demand_cities"][0]["longitude"] == 106.8
    assert enriched["parent_input_identity"] == result.input_identity.model_dump(mode="json")
    assert geography.input_identity != result.input_identity

    atomic = data_server.prepare_network_input(
        profile_ref,
        _decisions(),
        "ID",
        f"{PREPARED_OUTPUT_DIR}/prepared-input-atomic.json",
        ctx,
        administrative_catalog_relative_path="admin.json",
    )
    assert atomic.state == "ready"
    atomic_payload = json.loads((tmp_path / atomic.prepared_input_relative_path).read_text())
    assert atomic_payload["demand_cities"][0]["longitude"] == 106.8
    assert atomic_payload["parent_input_identity"] is None

    manual_demand = ConfirmedSourceDecision.model_validate(
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
    )
    with pytest.raises(
        ValueError,
        match="confirmed_mappings_not_allowed_for_unambiguous_source",
    ):
        data_server.prepare_network_input(
            profile_ref,
            [manual_demand, _decisions()[1]],
            "ID",
            f"{PREPARED_OUTPUT_DIR}/prepared-input-manual.json",
            ctx,
            administrative_catalog_relative_path="admin.json",
        )

    with pytest.raises(ProviderContractError, match="workspace_file_invalid"):
        data_server.prepare_network_input(
            profile_ref,
            _decisions(),
            "ID",
            f"{PREPARED_OUTPUT_DIR}/prepared-input.json",
            ctx,
        )


def test_candidate_changes_require_a_complete_new_workspace_preparation(
    tmp_path, monkeypatch
) -> None:
    (tmp_path / "candidate.csv").write_text(
        "warehouse_id,warehouse_name,warehouse_type,city_id,city_name,longitude,latitude,"
        "is_existing\n"
        "CAND-1,Jakarta Candidate,center,CITY-1,Jakarta,106.9,-6.3,false\n",
        encoding="utf-8",
    )
    _use_store(tmp_path, monkeypatch)
    ctx = _context(tmp_path)
    profile = data_server.inspect_workspace_sources(["candidate.csv"], object())
    assert profile.structuredContent is not None
    profile_ref = ResourceRef.model_validate(profile.structuredContent["resource_ref"])
    candidate = ConfirmedSourceDecision(
        relative_path="candidate.csv",
        role=SourceRole.CANDIDATE_WAREHOUSE,
    )

    result = data_server.prepare_network_input(
        profile_ref,
        [candidate],
        "ID",
        f"{PREPARED_OUTPUT_DIR}/candidate-only-preparation.json",
        ctx,
    )
    assert result.prepared_input_relative_path == (
        f"{PREPARED_OUTPUT_DIR}/candidate-only-preparation.json"
    )
    assert result.state == "needs_input"
    assert not hasattr(data_server, "normalize_candidate_delta")
    assert not hasattr(data_server, "derive_normalized_network_input")
