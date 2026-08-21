from __future__ import annotations

import asyncio
import json
from pathlib import Path
from types import SimpleNamespace

import pytest
from open_web_codex_provider import ProviderContractError, ResourceStore
from openpyxl import Workbook
from supply_chain_planner.data import server as data_server
from supply_chain_planner.data.mapping import SourceRole
from supply_chain_planner.shared.models import (
    ConfirmedSourceDecision,
    DataPreparationToolResult,
    SourceInspectionIdentity,
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
    assert asyncio.run(data_server.mcp.list_resources()) == []
    assert asyncio.run(data_server.mcp.list_resource_templates()) == []

    prepare = tools["prepare_network_input"]
    assert set(prepare.inputSchema["required"]) == {
        "inspection_identity",
        "inspected_relative_paths",
        "confirmed_sources",
        "country_code",
        "output_relative_path",
    }
    assert set(prepare.outputSchema["required"]) == {
        "outcome",
        "summary",
        "next_action",
        "retryable",
        "requirements",
        "prepared_input_relative_path",
        "input_identity",
        "state",
        "issue_count",
        "issues",
        "issues_truncated",
        "candidate_warehouse_count",
        "candidate_warehouses",
        "candidate_warehouses_truncated",
    }
    assert "resource_ref" not in prepare.outputSchema["properties"]

    inspect_tool = tools["inspect_workspace_sources"]
    inspection_schema = inspect_tool.outputSchema
    assert inspection_schema["discriminator"] == {
        "propertyName": "outcome",
        "mapping": {
            "inspected": "#/$defs/DataInspectionInspected",
            "selection_required": "#/$defs/DataInspectionSelectionRequired",
        },
    }
    assert len(inspection_schema["oneOf"]) == 2
    branch_required = [
        set(inspection_schema["$defs"][ref["$ref"].split("/")[-1]]["required"])
        for ref in inspection_schema["oneOf"]
    ]
    assert {
        "outcome",
        "schemaVersion",
        "summary",
        "next_action",
        "retryable",
        "source_profile",
        "inspection_identity",
        "inspected_relative_paths",
    } in branch_required
    assert {
        "outcome",
        "schemaVersion",
        "summary",
        "code",
        "next_action",
        "retryable",
        "observed",
        "limit",
    } in branch_required

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


def test_missing_warehouse_type_is_typed_non_retryable_user_input(
    tmp_path, monkeypatch
) -> None:
    (tmp_path / "demand.csv").write_text(
        "city_id,city_name,demand_quantity\nCITY-1,Jakarta,50\n",
        encoding="utf-8",
    )
    (tmp_path / "warehouses.csv").write_text(
        "warehouse_id,warehouse_name,city_id,city_name,is_existing\n"
        "WH-1,Jakarta Center,CITY-1,Jakarta,true\n",
        encoding="utf-8",
    )
    _use_store(tmp_path, monkeypatch)
    ctx = _context(tmp_path)
    inspected = data_server.inspect_workspace_sources(
        ["demand.csv", "warehouses.csv"], ctx
    )
    assert inspected.structuredContent is not None
    inspection = inspected.structuredContent

    assert inspection["outcome"] == "inspected"
    assert inspection["schemaVersion"] == "workspace_source_profile.v2"
    assert inspection["next_action"] == "confirm_sources"
    assert inspection["retryable"] is False
    warehouse_profile = next(
        source
        for source in inspection["source_profile"]["sources"]
        if source["relative_path"] == "warehouses.csv"
    )
    assert "role_assessments" not in warehouse_profile
    assert len(warehouse_profile["units"]) == 1
    assessment = warehouse_profile["units"][0]["role_assessments"][0]
    assert assessment["role"] == "existing_warehouse"
    assert assessment["state"] == "partial"
    assert assessment["ambiguous"] is False
    assert assessment["matched_required_fields"] == [
        "city_id",
        "city_name",
        "warehouse_id",
        "warehouse_name",
    ]
    assert assessment["missing_required_fields"] == ["warehouse_type"]

    output_path = f"{PREPARED_OUTPUT_DIR}/must-not-write.json"
    blocked = data_server.prepare_network_input(
        SourceInspectionIdentity.model_validate(inspection["inspection_identity"]),
        inspection["inspected_relative_paths"],
        _decisions(),
        "ID",
        output_path,
        ctx,
    )

    assert blocked.outcome == "needs_input"
    assert blocked.state == "needs_input"
    assert blocked.next_action == "request_user_input"
    assert blocked.retryable is False
    assert blocked.prepared_input_relative_path is None
    assert blocked.input_identity is None
    assert [item.model_dump(mode="json") for item in blocked.requirements] == [
        {
            "code": "required_fields_missing",
            "relative_path": "warehouses.csv",
            "candidate_roles": ["existing_warehouse"],
            "missing_required_fields": ["warehouse_type"],
            "question": (
                "文件 warehouses.csv 缺少仓型字段 warehouse_type。"
                "请在源数据中补充该列，每行使用 center 或 cross_docking，然后再继续。"
            ),
        }
    ]
    assert not (tmp_path / output_path).exists()


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
    _use_store(tmp_path, monkeypatch)

    result = data_server.inspect_workspace_sources(["candidate.csv"], object())
    assert result.structuredContent is not None
    inspection = result.structuredContent
    source = inspection["source_profile"]["sources"][0]
    assert "structure" not in source
    unit = source["units"][0]
    preview = unit["preview"]
    inspection_identity = SourceInspectionIdentity.model_validate(
        inspection["inspection_identity"]
    )
    inspected_paths = inspection["inspected_relative_paths"]

    assert unit["record_count"] == 12
    assert preview["preview_sample_count"] == 3
    assert preview["total_count"] == 12
    assert preview["total_count_exact"] is True
    assert "returned_count" not in preview
    assert "Balikpapan" not in json.dumps(preview["rows"])

    with pytest.raises(ProviderContractError, match="generated_output_path_invalid"):
        data_server.prepare_network_input(
            inspection_identity,
            inspected_paths,
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
        inspection_identity,
        inspected_paths,
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


def test_current_mock_sources_have_no_global_false_demand_blockers(tmp_path, monkeypatch) -> None:
    fixture = tmp_path / "fixture"
    fixture.mkdir()
    fixture_root = Path(__file__).parents[3] / "apps/web/scripts/fixtures/warehouse-network/mock_data"
    for path in fixture_root.iterdir():
        if path.suffix in {".csv", ".json"} and path.name != "manifest.json":
            (fixture / path.name).write_bytes(path.read_bytes())
    _use_store(fixture, monkeypatch)

    result = data_server.inspect_workspace_sources(
        sorted(path.name for path in fixture.iterdir() if path.suffix in {".csv", ".json"}), object()
    )
    assert result.structuredContent is not None
    inspection = result.structuredContent
    assert inspection["outcome"] == "inspected"
    assessments = {
        source["relative_path"]: [
            assessment
            for unit in source["units"]
            for assessment in unit["role_assessments"]
        ]
        for source in inspection["source_profile"]["sources"]
    }
    assert all(
        not (
            assessment["role"] == "demand"
            and assessment["state"] == "partial"
        )
        for source_assessments in assessments.values()
        for assessment in source_assessments
    )
    assert assessments["existing-warehouses.csv"][0]["role"] == "existing_warehouse"
    assert assessments["candidate-warehouses.csv"][0]["role"] == "candidate_warehouse"


def test_xlsx_and_json_units_are_not_aggregated(tmp_path, monkeypatch) -> None:
    workbook = Workbook()
    workbook.active.title = "需求"
    workbook.active.append(["city_id", "city_name", "demand_quantity"])
    workbook.active.append(["C-1", "Jakarta", 10])
    warehouses = workbook.create_sheet("仓库")
    warehouses.append(["warehouse_id", "warehouse_name", "warehouse_type", "city_id", "city_name"])
    warehouses.append(["W-1", "Jakarta", "center", "C-1", "Jakarta"])
    workbook.save(tmp_path / "network.xlsx")
    (tmp_path / "nested.json").write_text(
        json.dumps(
            {
                "payload": {
                    "demandRows": [{"city_id": "C-1", "city_name": "Jakarta", "quantity": 10}],
                    "warehouses": [{"warehouse_id": "W-1", "warehouse_name": "Jakarta"}],
                    "nested": [{"tags": [{"name": "primary"}]}],
                }
            }
        ),
        encoding="utf-8",
    )
    _use_store(tmp_path, monkeypatch)
    result = data_server.inspect_workspace_sources(["network.xlsx", "nested.json"], object())
    assert result.structuredContent is not None
    sources = {item["relative_path"]: item for item in result.structuredContent["source_profile"]["sources"]}
    xlsx_units = {unit["unit_ref"]: unit for unit in sources["network.xlsx"]["units"]}
    assert set(xlsx_units) == {"sheet:需求", "sheet:仓库"}
    assert {unit["kind"] for unit in xlsx_units.values()} == {"sheet"}
    assert any(item["role"] == "demand" for item in xlsx_units["sheet:需求"]["role_assessments"])
    assert any(item["role"] == "existing_warehouse" for item in xlsx_units["sheet:仓库"]["role_assessments"])
    json_units = {unit["unit_ref"]: unit for unit in sources["nested.json"]["units"]}
    assert set(json_units) == {
        "$.payload.demandRows",
        "$.payload.warehouses",
        "$.payload.nested",
        "$.payload.nested[*].tags",
    }
    assert all(unit["kind"] == "json_array" for unit in json_units.values())
    assert json_units["$.payload.nested[*].tags"]["locator"] == {
        "path": "$.payload.nested[*].tags",
        "array_prefix": "payload.nested.item.tags",
    }


def test_inspection_limits_are_explicit_selection_errors(tmp_path, monkeypatch) -> None:
    for name in ("a.csv", "b.csv"):
        (tmp_path / name).write_text("city_id,city_name\nC-1,Jakarta\n", encoding="utf-8")
    _use_store(tmp_path, monkeypatch)
    with pytest.raises(ValueError, match="at least one Workspace-relative path"):
        data_server.inspect_workspace_sources([], object())

    monkeypatch.setattr(data_server._workspace_intake, "MAX_INSPECTION_FILES", 1)
    file_limited = data_server.inspect_workspace_sources(["a.csv", "b.csv"], object())
    assert file_limited.structuredContent is not None
    assert file_limited.structuredContent["outcome"] == "selection_required"
    assert set(file_limited.structuredContent) == {
        "outcome",
        "schemaVersion",
        "summary",
        "code",
        "next_action",
        "retryable",
        "observed",
        "limit",
    }
    assert file_limited.structuredContent["code"] == "inspection_selection_required"
    assert file_limited.structuredContent["observed"]["files"] == 2
    assert file_limited.structuredContent["limit"]["files"] == 1

    workbook = Workbook()
    workbook.active.append(["city_id", "city_name", "demand_quantity"])
    workbook.create_sheet("second").append(["warehouse_id"])
    workbook.save(tmp_path / "multi.xlsx")
    monkeypatch.setattr(data_server._workspace_intake, "MAX_INSPECTION_FILES", 64)
    monkeypatch.setattr(data_server._workspace_intake, "MAX_INSPECTION_UNITS", 1)
    unit_limited = data_server.inspect_workspace_sources(["multi.xlsx"], object())
    assert unit_limited.structuredContent is not None
    assert unit_limited.structuredContent["outcome"] == "selection_required"
    assert unit_limited.structuredContent["observed"]["files"] == 1
    assert unit_limited.structuredContent["observed"]["units"] == 2
    assert unit_limited.structuredContent["limit"]["units"] == 1

    monkeypatch.setattr(data_server._workspace_intake, "MAX_INSPECTION_UNITS", 128)
    monkeypatch.setattr(data_server._workspace_intake, "MAX_INSPECTION_BYTES", 1)
    byte_limited = data_server.inspect_workspace_sources(["a.csv"], object())
    assert byte_limited.structuredContent is not None
    assert byte_limited.structuredContent["outcome"] == "selection_required"
    assert byte_limited.structuredContent["observed"]["bytes"] > 1
    assert byte_limited.structuredContent["limit"]["bytes"] == 1


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
    _use_store(tmp_path, monkeypatch)
    ctx = _context(tmp_path)
    profile = data_server.inspect_workspace_sources(
        ["demand.csv", "warehouses.csv", "admin.json"], object()
    )
    assert profile.structuredContent is not None
    inspection = profile.structuredContent
    inspection_identity = SourceInspectionIdentity.model_validate(
        inspection["inspection_identity"]
    )
    inspected_paths = inspection["inspected_relative_paths"]

    with pytest.raises(ValueError, match="country_code_required_iso_alpha2"):
        data_server.prepare_network_input(
            inspection_identity,
            inspected_paths,
            _decisions(),
            "IDN",
            f"{PREPARED_OUTPUT_DIR}/prepared-input.json",
            ctx,
        )

    result = data_server.prepare_network_input(
        inspection_identity,
        inspected_paths,
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
        inspection_identity,
        inspected_paths,
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
            inspection_identity,
            inspected_paths,
            [manual_demand, _decisions()[1]],
            "ID",
            f"{PREPARED_OUTPUT_DIR}/prepared-input-manual.json",
            ctx,
            administrative_catalog_relative_path="admin.json",
        )

    with pytest.raises(ProviderContractError, match="workspace_file_invalid"):
        data_server.prepare_network_input(
            inspection_identity,
            inspected_paths,
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
    inspection = profile.structuredContent
    inspection_identity = SourceInspectionIdentity.model_validate(
        inspection["inspection_identity"]
    )
    inspected_paths = inspection["inspected_relative_paths"]
    candidate = ConfirmedSourceDecision(
        relative_path="candidate.csv",
        role=SourceRole.CANDIDATE_WAREHOUSE,
    )

    result = data_server.prepare_network_input(
        inspection_identity,
        inspected_paths,
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


def test_inspection_identity_is_order_independent_and_rejects_changed_inputs(
    tmp_path, monkeypatch
) -> None:
    (tmp_path / "a.csv").write_text("city_id,city_name\nCITY-1,Jakarta\n", encoding="utf-8")
    (tmp_path / "b.csv").write_text("city_id,city_name\nCITY-2,Balikpapan\n", encoding="utf-8")
    _use_store(tmp_path, monkeypatch)
    ctx = _context(tmp_path)

    first = data_server.inspect_workspace_sources(["b.csv", "a.csv"], ctx)
    second = data_server.inspect_workspace_sources(["a.csv", "b.csv"], ctx)
    first_identity = first.structuredContent["inspection_identity"]
    assert first_identity == second.structuredContent["inspection_identity"]
    assert first.structuredContent["inspected_relative_paths"] == ["a.csv", "b.csv"]

    (tmp_path / "b.csv").write_text(
        "city_id,city_name\nCITY-2,Balikpapan changed\n", encoding="utf-8"
    )
    with pytest.raises(ProviderContractError, match="source_inspection_changed") as byte_error:
        data_server.prepare_network_input(
            SourceInspectionIdentity.model_validate(first_identity),
            ["a.csv", "b.csv"],
            [],
            "ID",
            f"{PREPARED_OUTPUT_DIR}/must-not-write.json",
            ctx,
        )
    assert byte_error.value.code == "source_inspection_changed"
    assert not (tmp_path / f"{PREPARED_OUTPUT_DIR}/must-not-write.json").exists()

    (tmp_path / "c.csv").write_text(
        "city_id,city_name\nCITY-3,Surabaya\n", encoding="utf-8"
    )
    with pytest.raises(ProviderContractError, match="source_inspection_changed") as path_set_error:
        data_server.prepare_network_input(
            SourceInspectionIdentity.model_validate(first_identity),
            ["a.csv", "b.csv", "c.csv"],
            [],
            "ID",
            f"{PREPARED_OUTPUT_DIR}/must-not-write-path-set.json",
            ctx,
        )
    assert path_set_error.value.code == "source_inspection_changed"
    assert not (tmp_path / f"{PREPARED_OUTPUT_DIR}/must-not-write-path-set.json").exists()


def test_inspection_rejects_symlink_and_data_resource_surface_is_empty(tmp_path, monkeypatch) -> None:
    (tmp_path / "source.csv").write_text("city_id,city_name\nCITY-1,Jakarta\n", encoding="utf-8")
    (tmp_path / "outside.csv").write_text("city_id,city_name\nCITY-2,Outside\n", encoding="utf-8")
    (tmp_path / "linked.csv").symlink_to(tmp_path / "outside.csv")
    _use_store(tmp_path, monkeypatch)
    with pytest.raises(ValueError, match="workspace_source_symlink_rejected"):
        data_server.inspect_workspace_sources(["linked.csv"], _context(tmp_path))
    with pytest.raises(ValueError, match="invalid_workspace_relative_path"):
        data_server.inspect_workspace_sources(["../outside.csv"], _context(tmp_path))
    assert asyncio.run(data_server.mcp.list_resources()) == []
    assert asyncio.run(data_server.mcp.list_resource_templates()) == []
