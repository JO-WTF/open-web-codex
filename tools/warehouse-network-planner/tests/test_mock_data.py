from __future__ import annotations

import csv
import hashlib
import json
from pathlib import Path


def test_indonesia_tutorial_fixture_is_validated_and_not_empty() -> None:
    root = Path(__file__).resolve().parents[1] / "examples" / "indonesia-network" / "base"
    demand = list(csv.DictReader((root / "demand-cities.csv").open(newline="")))
    warehouses = list(csv.DictReader((root / "existing-warehouses.csv").open(newline="")))
    quotes = list(csv.DictReader((root / "route-quotes.csv").open(newline="")))
    validation = json.loads((root / "validation-report.json").read_text())

    assert len(demand) == 50
    assert len(warehouses) == 11
    assert len(quotes) == 580
    assert validation["all_passed"] is True
    assert validation["demand"]["formula_pass"] is True
    assert validation["boundary_source"]["all_50_points_inside_adm2"] is True
    assert validation["quotes"]["spearman_distance_price"] >= 0.85


def test_examples_exclude_rebuildable_and_duplicate_data() -> None:
    examples = Path(__file__).resolve().parents[1] / "examples"
    removed_paths = (
        examples / "network-input.json",
        examples / "route-matrix-input.json",
        examples / "indonesia-network" / "candidate-extension",
    )
    assert all(not path.exists() for path in removed_paths)
    ignored_paths = set((examples.parent / ".gitignore").read_text().splitlines())
    assert {
        "examples/indonesia-network/source/",
        "examples/indonesia-tutorial/releases/",
        "examples/indonesia-tutorial/sources/",
    } <= ignored_paths

    base = examples / "indonesia-network" / "base"
    source_lock_path = base / "source-lock.json"
    source_lock = json.loads(source_lock_path.read_text())
    boundary_source = next(
        source for source in source_lock["sources"] if source["source_id"] == "geoboundaries-idn-adm2"
    )
    assert boundary_source["sha256"] == (
        "2b08dc56f4b488f1a6c84093c6a302628feb0fa49efefa47cd9b1fee3bd129e3"
    )
    assert boundary_source["url"].startswith(
        "https://media.githubusercontent.com/media/wmgeolab/"
        "geoBoundariesArchive_3_0_0/7c8dbc599e312d9204e450aecfa66c204b8cf9b8/"
    )

    manifest = json.loads((base / "dataset-manifest.json").read_text())
    source_lock_digest = hashlib.sha256(source_lock_path.read_bytes()).hexdigest()
    assert manifest["source_lock"]["sha256"] == source_lock_digest
    source_lock_entry = next(
        entry for entry in manifest["files"] if entry["path"] == "source-lock.json"
    )
    assert source_lock_entry["bytes"] == source_lock_path.stat().st_size
    assert source_lock_entry["sha256"] == source_lock_digest
