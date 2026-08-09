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
        "id": "indonesia-network-tutorial",
        "version": "1.0.0",
        "seed": 42,
    }
    assert created["contentSummary"]["sourceFileCount"] == 8
    assert created["contentSummary"]["totalBytes"] > 0
    assert len(created["contentSummary"]["manifestSha256"]) == 64
    assert len(created["sources"]) == 9
    assert all(source["relative_path"].startswith("demo-data/") for source in created["sources"])
    assert all(set(source) == {"relative_path", "format", "size"} for source in created["sources"])
    assert workspace_source_metadata(tmp_path)["dataClassification"] == "synthetic_demo"
    assert len(discover(tmp_path)) == 9


def test_tutorial_template_contains_validated_network_fixture(tmp_path: Path) -> None:
    created = create_sources(tmp_path, TEMPLATE_REF, 42)
    target = tmp_path / "demo-data" / "indonesia-network-tutorial-1.0.0"

    assert created["status"] == "created"
    assert created["contentSummary"]["cityDemandCount"] == LARGE_CITY_DEMAND_COUNT
    assert created["contentSummary"]["routeCount"] == 580
    demand = list(csv.DictReader((target / "demand-cities.csv").open(encoding="utf-8")))
    warehouses = list(csv.DictReader((target / "existing-warehouses.csv").open(encoding="utf-8")))
    quotes = list(csv.DictReader((target / "route-quotes.csv").open(encoding="utf-8")))
    assert len(demand) == 50
    assert len(warehouses) == 11
    assert len(quotes) == 580
    assert json.loads((target / "validation-report.json").read_text())["all_passed"] is True


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
    target = tmp_path / "demo-data" / "indonesia-network-tutorial-1.0.0"
    if mutation == "missing":
        (target / "route-quotes.csv").unlink()
    elif mutation == "drift":
        (target / "route-quotes.csv").write_text("modified", encoding="utf-8")
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
        (
            tmp_path / "demo-data" / "indonesia-network-tutorial-1.0.0" / "demo-manifest.json"
        ).read_text(encoding="utf-8")
    )
    assert manifest["dataClassification"] == "synthetic_demo"


def test_demo_server_exposes_only_the_generation_tool() -> None:
    tools = asyncio.run(mcp.list_tools())
    assert [tool.name for tool in tools] == ["create_demo_workspace_sources"]
    options = mcp._mcp_server.create_initialization_options(
        experimental_capabilities={SANDBOX_STATE_META_CAPABILITY: {}}
    )
    assert SANDBOX_STATE_META_CAPABILITY in options.capabilities.experimental
