from __future__ import annotations

import csv
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
