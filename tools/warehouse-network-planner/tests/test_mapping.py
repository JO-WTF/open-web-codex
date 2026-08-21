from __future__ import annotations

import pytest
from pydantic import ValidationError
from supply_chain_planner.data.mapping import (
    FieldObservation,
    SourceRole,
    suggest_role_mappings,
    suggest_role_requirements,
)
from supply_chain_planner.shared.models import (
    ConfirmedSourceDecision,
    ConfirmedSourceInputDecision,
)


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
        ([*base["mappings"], base["mappings"][0]], "must be unique"),
        (base["mappings"][:-1], "missing required fields"),
    ):
        with pytest.raises(ValidationError, match=message):
            ConfirmedSourceDecision.model_validate({**base, "mappings": mappings})

    incomplete = ConfirmedSourceInputDecision.model_validate(
        {**base, "mappings": base["mappings"][:-1]}
    )
    assert [mapping.target_field for mapping in incomplete.mappings] == ["city_id", "city_name"]


def test_partial_warehouse_mapping_reports_missing_warehouse_type() -> None:
    requirements = suggest_role_requirements(
        [
            FieldObservation(name=field, sample_values=("true",) if field == "is_existing" else ())
            for field in (
                "warehouse_id",
                "warehouse_name",
                "city_id",
                "city_name",
                "is_existing",
            )
        ]
    )

    assert [(item.role, item.missing_required_fields) for item in requirements] == [
        (SourceRole.EXISTING_WAREHOUSE, ("warehouse_type",))
    ]


def test_complete_demand_does_not_report_weak_warehouse_gaps() -> None:
    requirements = suggest_role_requirements(
        [
            FieldObservation(name=field)
            for field in ("city_id", "city_name", "demand_quantity")
        ]
    )

    assert requirements == []

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
