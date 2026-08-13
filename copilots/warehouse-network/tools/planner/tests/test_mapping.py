from __future__ import annotations

import pytest
from pydantic import ValidationError
from supply_chain_planner.data.mapping import (
    FieldObservation,
    SourceRole,
    suggest_role_mappings,
)
from supply_chain_planner.shared.models import ConfirmedSourceDecision


def test_confirmed_mapping_rejects_unknown_duplicate_and_missing_targets() -> None:
    base = {
        "relative_path": "demand.csv",
        "role": "demand",
        "mappings": [
            {
                "source_field": field,
                "target_field": field,
                "transform": "trim",
            }
            for field in ("city_id", "city_name", "demand_quantity")
        ],
    }
    for mappings, message in (
        (
            [*base["mappings"], {"source_field": "x", "target_field": "demand_id", "transform": "trim"}],
            "not allowed",
        ),
        (base["mappings"][:-1], "missing required fields"),
        ([*base["mappings"], base["mappings"][0]], "must be unique"),
    ):
        with pytest.raises(ValidationError, match=message):
            ConfirmedSourceDecision.model_validate({**base, "mappings": mappings})

def test_pure_warehouse_fields_require_existing_or_candidate_confirmation() -> None:
    proposals = suggest_role_mappings(
        [
            FieldObservation(name=field)
            for field in (
                "warehouse_id",
                "warehouse_name",
                "warehouse_type",
                "city_id",
                "city_name",
            )
        ]
    )

    assert [item.role for item in proposals] == [
        SourceRole.EXISTING_WAREHOUSE,
        SourceRole.CANDIDATE_WAREHOUSE,
    ]
    assert all(item.ambiguous for item in proposals)
    assert SourceRole.CURRENT_ASSIGNMENT not in {item.role for item in proposals}


def test_pure_mapping_marks_multiple_aliases_ambiguous() -> None:
    proposals = suggest_role_mappings(
        [
            FieldObservation(name=field)
            for field in ("city_id", "city_code", "city_name", "demand_quantity")
        ]
    )
    demand = next(item for item in proposals if item.role == SourceRole.DEMAND)
    city_mapping = next(
        item for item in demand.field_mappings if item.target_field == "city_id"
    )

    assert demand.ambiguous is True
    assert city_mapping.source_fields == ("city_id", "city_code")
