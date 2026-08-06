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


def test_csv_and_json_profiles_are_structural_samples_with_exact_counts(tmp_path: Path) -> None:
    csv_path = tmp_path / "demand.csv"
    csv_path.write_text(
        "origin;qty\n" + "\n".join(f"city-{index};{index}" for index in range(25)) + "\n",
        encoding="utf-8",
    )
    json_path = tmp_path / "nested.json"
    json_path.write_text(
        json.dumps(
            {
                "orders": [
                    {"origin": f"city-{index}", "quantity": index} for index in range(25)
                ]
            }
        ),
        encoding="utf-8",
    )

    sources = discover(tmp_path)
    profiles = {
        source["display_name"]: inspect(tmp_path, source["source_ref"]) for source in sources
    }

    assert profiles["demand.csv"]["structure"]["delimiter"] == ";"
    assert profiles["demand.csv"]["structure"]["columns"] == ["origin", "qty"]
    csv_structure = profiles["demand.csv"]["structure"]
    assert csv_structure["record_count"] == 25
    assert csv_structure["record_count_exact"] is True
    assert csv_structure["preview"]["strategy"] == "head"
    assert csv_structure["preview"]["returned_count"] == 20
    assert csv_structure["preview"]["complete"] is False
    assert profiles["nested.json"]["structure"]["arrays"][0]["path"] == "$.orders"
    json_array = profiles["nested.json"]["structure"]["arrays"][0]
    assert json_array["length"] == 25
    assert json_array["length_exact"] is True
    assert json_array["preview"]["returned_count"] == 3
    assert json_array["preview"]["complete"] is False


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

    assert (
        source["media_type"] == "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
    )
    assert [sheet["sheet"] for sheet in profile["structure"]["sheets"]] == ["需求点", "仓库"]
    assert profile["structure"]["sheets"][0]["columns"] == [
        "Demand Location ID",
        "Name",
        "Region",
    ]
    warehouse_sheet = profile["structure"]["sheets"][1]
    assert warehouse_sheet["preview"]["rows"] == [["F1", "Existing", "existing"]]
    assert warehouse_sheet["record_count"] == 1
    assert warehouse_sheet["record_count_exact"] is True


def test_mapping_candidates_do_not_promote_cross_entity_substring_matches(tmp_path: Path) -> None:
    (tmp_path / "facilities.csv").write_text(
        "facility_id,existing_or_candidate\nF1,existing\n",
        encoding="utf-8",
    )
    (tmp_path / "lanes.csv").write_text(
        "origin_city_id,destination_city_id,distance_km,travel_time_hours,"
        "base_cost_per_unit,distance_cost_per_km_per_unit,currency\n"
        "CITY-JKT,CITY-JKT,0,0,1,0.1,IDR\n",
        encoding="utf-8",
    )
    contract = json.loads(
        (
            Path(__file__).resolve().parents[1] / "contracts/warehouse-network-planning-1.0.0.json"
        ).read_text()
    )
    profiles = [inspect(tmp_path, source["source_ref"]) for source in discover(tmp_path)]

    proposal = propose_mapping(profiles, contract)

    assert any(
        item["source_field"] == "origin_city_id" and item["target_entity"] == "Lane"
        for item in proposal["candidates"]
    )
    assert not proposal["conflicts"]


def test_confirmed_mapping_builds_strict_planning_source(tmp_path: Path) -> None:
    files = {
        "cities.csv": (
            "city_id,name,region,latitude,longitude\n"
            "CITY-JKT,Jakarta,ID-JK,-6.2,106.8\n"
            "CITY-BDG,Bandung,ID-JB,-6.9,107.6\n"
        ),
        "city-demand.csv": "city_id,date,quantity\nCITY-JKT,2026-01-01,10\n",
        "facilities.csv": (
            "facility_id,name,city_id,existing_or_candidate,capacity,"
            "fixed_cost,opening_cost,handling_cost\n"
            "F1,Existing,CITY-JKT,existing,100,100,0,1\n"
            "F2,Candidate,CITY-BDG,candidate,100,100,20,1\n"
        ),
        "coverage.csv": "facility_id,city_id,is_current\nF1,CITY-JKT,true\n",
        "lanes.csv": (
            "origin_city_id,destination_city_id,distance_km,travel_time_hours,"
            "base_cost_per_unit,distance_cost_per_km_per_unit,currency\n"
            "CITY-JKT,CITY-JKT,0,0,1,0.1,IDR\n"
            "CITY-BDG,CITY-JKT,150,4,1,0.1,IDR\n"
        ),
    }
    for name, content in files.items():
        (tmp_path / name).write_text(content, encoding="utf-8")

    sources = discover(tmp_path)
    refs = {source["display_name"]: source["source_ref"] for source in sources}
    mapping = [
        {
            "source_ref": refs["cities.csv"],
            "source_field": field,
            "target_entity": "City",
            "target_field": field,
        }
        for field in ("city_id", "name", "region", "latitude", "longitude")
    ]
    mapping += [
        {
            "source_ref": refs["city-demand.csv"],
            "source_field": field,
            "target_entity": "CityDemand",
            "target_field": field,
        }
        for field in ("city_id", "date", "quantity")
    ]
    mapping += [
        {
            "source_ref": refs["facilities.csv"],
            "source_field": field,
            "target_entity": "Facility",
            "target_field": field,
        }
        for field in (
            "facility_id",
            "name",
            "city_id",
            "existing_or_candidate",
            "capacity",
            "fixed_cost",
            "opening_cost",
            "handling_cost",
        )
    ]
    mapping += [
        {
            "source_ref": refs["coverage.csv"],
            "source_field": field,
            "target_entity": "Coverage",
            "target_field": field,
        }
        for field in ("facility_id", "city_id", "is_current")
    ]
    mapping += [
        {
            "source_ref": refs["lanes.csv"],
            "source_field": field,
            "target_entity": "Lane",
            "target_field": field,
        }
        for field in (
            "origin_city_id",
            "destination_city_id",
            "distance_km",
            "travel_time_hours",
            "base_cost_per_unit",
            "distance_cost_per_km_per_unit",
            "currency",
        )
    ]
    source = _build_planning_source(
        tmp_path,
        list(refs.values()),
        mapping,
        [
            {"name": "market", "value": "ID"},
            {"name": "planning_mode", "value": "candidate_warehouse_optimization"},
            {"name": "cost_scope", "value": "fixed, opening, handling, transport"},
            {"name": "target_sla_hours", "value": 48},
            {"name": "coverage_target", "value": 95},
        ],
    )

    assert source.currency == "IDR"
    assert source.planning_period == "2026-01"
    assert source.route_method == "quoted"
    assert source.route_provider == "workspace-quoted-lanes"
    assert len(source.city_demands) == 1
    assert len(source.warehouse_city_coverage) == 1
    assert len(source.lanes) == 2
