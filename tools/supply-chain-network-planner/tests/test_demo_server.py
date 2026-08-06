from __future__ import annotations

import asyncio
import csv
import json
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from types import SimpleNamespace

import pytest

from supply_chain_planner.demo_server import (
    LARGE_CITY_DEMAND_COUNT,
    SANDBOX_STATE_META_CAPABILITY,
    TEMPLATE_REF,
    create_sources,
    mcp,
)
from supply_chain_planner.workspace_intake import (
    discover,
    trusted_workspace_root,
    workspace_source_metadata,
)


def test_trusted_workspace_requires_runtime_metadata(tmp_path: Path) -> None:
    with pytest.raises(ValueError, match="metadata is unavailable"):
        trusted_workspace_root(SimpleNamespace(model_extra={}))

    meta = SimpleNamespace(
        model_extra={SANDBOX_STATE_META_CAPABILITY: {"sandboxCwd": tmp_path.as_uri()}}
    )
    assert trusted_workspace_root(meta) == tmp_path.resolve()


def test_empty_workspace_generation_is_complete_and_idempotent(tmp_path: Path) -> None:
    created = create_sources(tmp_path, TEMPLATE_REF, 42)
    reused = create_sources(tmp_path, TEMPLATE_REF, 42)

    assert created["status"] == "created"
    assert reused["status"] == "reused"
    assert created["dataClassification"] == "synthetic_demo"
    assert created["template"] == {
        "id": "warehouse-network-large",
        "version": "1.0.0",
        "seed": 42,
    }
    assert created["contentSummary"]["sourceFileCount"] == 6
    assert created["contentSummary"]["totalBytes"] > 0
    assert len(created["contentSummary"]["manifestSha256"]) == 64
    assert len(created["sources"]) == 7
    assert all("/" not in source["source_ref"] for source in created["sources"])
    assert all("path" not in source for source in created["sources"])
    assert workspace_source_metadata(tmp_path)["dataClassification"] == "synthetic_demo"
    assert len(discover(tmp_path)) == 7



def test_large_template_uses_city_grain_lanes(tmp_path: Path) -> None:
    created = create_sources(tmp_path, TEMPLATE_REF, 42)
    target = tmp_path / "demo-data" / "warehouse-network-large-1.0.0"

    assert created["status"] == "created"
    assert created["contentSummary"]["cityDemandCount"] == LARGE_CITY_DEMAND_COUNT
    assert created["contentSummary"]["routeCount"] == 144
    assert "ID1-Jakarta" in (target / "facilities.csv").read_text(encoding="utf-8")
    assert "ID6-Ambon" in (target / "facilities.csv").read_text(encoding="utf-8")
    assert "existing_or_candidate" in (target / "facilities.csv").read_text(encoding="utf-8")
    assert "Jakarta" in (target / "cities.csv").read_text(encoding="utf-8")
    cities = {
        row["city_id"]: (row["latitude"], row["longitude"])
        for row in csv.DictReader((target / "cities.csv").open(encoding="utf-8"))
    }
    city_demands = list(csv.DictReader((target / "city-demand.csv").open(encoding="utf-8")))
    assert len(city_demands) == LARGE_CITY_DEMAND_COUNT
    assert {row["city_id"] for row in city_demands} == set(cities)
    assert all(int(row["quantity"]) > 0 for row in city_demands)
    coverage = list(
        csv.DictReader((target / "warehouse-city-coverage.csv").open(encoding="utf-8"))
    )
    assert len(coverage) == LARGE_CITY_DEMAND_COUNT
    assert {row["city_id"] for row in coverage} == set(cities)
    lanes = list(csv.DictReader((target / "city-lanes.csv").open(encoding="utf-8")))
    assert len(lanes) == 144
    assert set(lanes[0]) == {
        "origin_city_id", "destination_city_id", "distance_km", "travel_time_hours",
        "base_cost_per_unit", "distance_cost_per_km_per_unit", "currency",
    }


def test_generation_rejects_non_empty_workspace(tmp_path: Path) -> None:
    (tmp_path / "real.csv").write_text("id,value\n1,2\n", encoding="utf-8")

    with pytest.raises(ValueError, match="workspace_contains_supported_sources"):
        create_sources(tmp_path, TEMPLATE_REF, 42)


def test_generation_rejects_empty_supported_file_and_source_symlink(tmp_path: Path) -> None:
    (tmp_path / "empty.csv").touch()
    with pytest.raises(ValueError, match="workspace_contains_supported_sources"):
        create_sources(tmp_path, TEMPLATE_REF, 42)

    (tmp_path / "empty.csv").unlink()
    outside = tmp_path.parent / f"{tmp_path.name}-outside.txt"
    outside.write_text("id\n1\n", encoding="utf-8")
    (tmp_path / "linked.json").symlink_to(outside)
    with pytest.raises(ValueError, match="workspace_supported_source_symlink_rejected"):
        create_sources(tmp_path, TEMPLATE_REF, 42)


@pytest.mark.parametrize("mutation", ["missing", "drift", "extra"])
def test_generation_rejects_partial_or_modified_target(tmp_path: Path, mutation: str) -> None:
    create_sources(tmp_path, TEMPLATE_REF, 42)
    target = tmp_path / "demo-data" / "warehouse-network-large-1.0.0"
    if mutation == "missing":
        (target / "city-lanes.csv").unlink()
    elif mutation == "drift":
        (target / "city-lanes.csv").write_text("modified", encoding="utf-8")
    else:
        (target / "extra.csv").write_text("unexpected", encoding="utf-8")

    with pytest.raises(ValueError, match="demo_target_conflict"):
        create_sources(tmp_path, TEMPLATE_REF, 42)


def test_generation_rejects_symlink_target(tmp_path: Path) -> None:
    outside = tmp_path / "outside"
    outside.mkdir()
    (tmp_path / "demo-data").symlink_to(outside, target_is_directory=True)

    with pytest.raises(ValueError, match="demo_target_symlink_rejected"):
        create_sources(tmp_path, TEMPLATE_REF, 42)


def test_concurrent_generation_converges_without_overwrite(tmp_path: Path) -> None:
    with ThreadPoolExecutor(max_workers=2) as pool:
        results = list(pool.map(lambda _: create_sources(tmp_path, TEMPLATE_REF, 42), range(2)))

    assert sorted(result["status"] for result in results) == ["created", "reused"]
    manifest = json.loads(
        (tmp_path / "demo-data" / "warehouse-network-large-1.0.0" / "demo-manifest.json").read_text(
            encoding="utf-8"
        )
    )
    assert manifest["dataClassification"] == "synthetic_demo"


def test_demo_server_exposes_only_the_generation_tool() -> None:
    tools = asyncio.run(mcp.list_tools())
    assert [tool.name for tool in tools] == ["create_demo_workspace_sources"]
    options = mcp._mcp_server.create_initialization_options(
        experimental_capabilities={SANDBOX_STATE_META_CAPABILITY: {}}
    )
    assert SANDBOX_STATE_META_CAPABILITY in options.capabilities.experimental
