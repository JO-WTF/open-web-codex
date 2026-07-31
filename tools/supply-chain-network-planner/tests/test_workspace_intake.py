from __future__ import annotations

import json
from pathlib import Path

import pytest
from openpyxl import Workbook

from supply_chain_planner.data_server import _build_planning_source
from supply_chain_planner.workspace_intake import discover, inspect, propose_mapping


def test_workspace_discovery_is_bounded_and_returns_opaque_refs(tmp_path: Path) -> None:
    (tmp_path / "demand.csv").write_text(
        "origin,quantity,date\nJakarta,10,2026-01-01\n",
        encoding="utf-8",
    )
    (tmp_path / "nested").mkdir()
    (tmp_path / "nested" / "facilities.json").write_text(
        json.dumps({"facilities": [{"facility_id": "JKT", "latitude": -6.2}]}),
        encoding="utf-8",
    )
    (tmp_path / "notes.txt").write_text("ignored", encoding="utf-8")

    sources = discover(tmp_path)

    assert [source["display_name"] for source in sources] == [
        "demand.csv",
        "facilities.json",
    ]
    assert all(source["source_ref"].startswith("source-") for source in sources)
    assert all("/" not in source["source_ref"] for source in sources)


def test_workspace_discovery_rejects_legacy_excel_formats(tmp_path: Path) -> None:
    (tmp_path / "legacy.xlsm").write_bytes(b"not a supported workbook")

    with pytest.raises(ValueError, match="unsupported_source_format"):
        discover(tmp_path)


def test_csv_and_json_profiles_are_structural_samples(tmp_path: Path) -> None:
    csv_path = tmp_path / "demand.csv"
    csv_path.write_text("origin;qty\nJakarta;10\n", encoding="utf-8")
    json_path = tmp_path / "nested.json"
    json_path.write_text(
        json.dumps({"orders": [{"origin": "Jakarta", "quantity": 10}]}),
        encoding="utf-8",
    )

    sources = discover(tmp_path)
    profiles = {source["display_name"]: inspect(tmp_path, source["source_ref"]) for source in sources}

    assert profiles["demand.csv"]["structure"]["delimiter"] == ";"
    assert profiles["demand.csv"]["structure"]["columns"] == ["origin", "qty"]
    assert profiles["nested.json"]["structure"]["arrays"][0]["path"] == "$.orders"
    assert profiles["nested.json"]["structure"]["arrays"][0]["length"] == 1


def test_xlsx_profile_preserves_multiple_sheets_and_bounded_rows(tmp_path: Path) -> None:
    workbook = Workbook()
    locations = workbook.active
    locations.title = "需求点"
    locations.append(["Demand Location ID", "Name", "Region"])
    locations.append(["D1", "Jakarta", "ID-JK"])
    facilities = workbook.create_sheet("仓库")
    facilities.append(["Facility ID", "Name", "Existing or Candidate"])
    facilities.append(["F1", "Existing", "existing"])
    workbook.save(tmp_path / "network.xlsx")

    source = discover(tmp_path)[0]
    profile = inspect(tmp_path, source["source_ref"])

    assert source["media_type"] == "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
    assert [sheet["sheet"] for sheet in profile["structure"]["sheets"]] == ["需求点", "仓库"]
    assert profile["structure"]["sheets"][0]["columns"] == [
        "Demand Location ID",
        "Name",
        "Region",
    ]
    assert profile["structure"]["sheets"][1]["rows"] == [["F1", "Existing", "existing"]]


def test_mapping_candidates_do_not_promote_cross_entity_substring_matches(tmp_path: Path) -> None:
    (tmp_path / "facilities.csv").write_text(
        "facility_id,existing_or_candidate\nF1,existing\n",
        encoding="utf-8",
    )
    (tmp_path / "rates.csv").write_text(
        "origin_facility_id,destination_region,rate_value,currency\nF1,ID-JK,1,IDR\n",
        encoding="utf-8",
    )
    contract = json.loads(
        (Path(__file__).resolve().parents[1] / "contracts/indonesia-warehouse-network-2.0.0.json").read_text()
    )
    profiles = [inspect(tmp_path, source["source_ref"]) for source in discover(tmp_path)]

    proposal = propose_mapping(profiles, contract)

    assert not any(
        item["source_field"] == "origin_facility_id" and item["target_entity"] == "Route"
        for item in proposal["candidates"]
    )
    assert not proposal["conflicts"]


def test_confirmed_mapping_builds_strict_planning_source(tmp_path: Path) -> None:
    files = {
        "locations.csv": "demand_location_id,name,region,latitude,longitude\nD1,Jakarta,ID-JK,-6.2,106.8\n",
        "demand.csv": "demand_id,demand_location_id,date,quantity\nO1,D1,2026-01-01,10\n",
        "facilities.csv": "facility_id,name,existing_or_candidate,latitude,longitude,capacity,fixed_cost,opening_cost,handling_cost\nF1,Existing,existing,-6.2,106.8,100,100,0,1\nF2,Candidate,candidate,-6.3,106.9,100,100,20,1\n",
        "assignments.csv": "demand_location_id,facility_id\nD1,F1\n",
        "rates.csv": "origin_facility_id,destination_region,rate_value,currency\nF1,ID-JK,2,IDR\nF2,ID-JK,2,IDR\n",
    }
    for name, content in files.items():
        (tmp_path / name).write_text(content, encoding="utf-8")

    sources = discover(tmp_path)
    refs = {source["display_name"]: source["source_ref"] for source in sources}
    mapping = [
        {"source_ref": refs["locations.csv"], "source_field": field, "target_entity": "DemandLocation", "target_field": field}
        for field in ("demand_location_id", "name", "region", "latitude", "longitude")
    ]
    mapping += [
        {"source_ref": refs["demand.csv"], "source_field": field, "target_entity": "Demand", "target_field": field}
        for field in ("demand_id", "demand_location_id", "date", "quantity")
    ]
    mapping += [
        {"source_ref": refs["facilities.csv"], "source_field": field, "target_entity": "Facility", "target_field": field}
        for field in ("facility_id", "name", "existing_or_candidate", "latitude", "longitude", "capacity", "fixed_cost", "opening_cost", "handling_cost")
    ]
    mapping += [
        {"source_ref": refs["assignments.csv"], "source_field": field, "target_entity": "Assignment", "target_field": field}
        for field in ("demand_location_id", "facility_id")
    ]
    mapping += [
        {"source_ref": refs["rates.csv"], "source_field": field, "target_entity": "Rate", "target_field": field}
        for field in ("origin_facility_id", "destination_region", "rate_value", "currency")
    ]
    source = _build_planning_source(
        tmp_path,
        list(refs.values()),
        mapping,
        [
            {"name": "planning_mode", "value": "optimization"},
            {"name": "planning_period", "value": "2026-01"},
            {"name": "currency", "value": "IDR"},
            {"name": "cost_scope", "value": "fixed, opening, handling, transport"},
            {"name": "target_sla_hours", "value": 48},
            {"name": "coverage_target", "value": 95},
            {"name": "route_source", "value": "estimated"},
            {"name": "detour_factor", "value": 1.2},
            {"name": "average_speed_kmh", "value": 40},
            {"name": "daily_transport_hours", "value": 10},
        ],
    )

    assert source.currency == "IDR"
    assert len(source.demand_locations) == 1
    assert len(source.route_entries) == 2
