from __future__ import annotations

from uuid import uuid4

import pytest
from pydantic import ValidationError

from supply_chain_planner.case_types import SourceSummary
from supply_chain_planner.mapping import (
    FieldObservation,
    MappingEngine,
    SourceRole,
    TransformKind,
    suggest_role_mappings,
)
from supply_chain_planner.models import ConfirmedSourceDecision
from supply_chain_planner.network_data import FieldInspection, SourceInspection


def _inspection(
    name: str, fields: list[str], samples: dict[str, list[str]] | None = None
) -> SourceInspection:
    return SourceInspection(
        source=SourceSummary(
            source_id=uuid4(),
            source_ref=f"source-{name}",
            display_name=name,
            media_type="text/csv",
            content_sha256="a" * 64,
            byte_size=10,
            source_revision=1,
        ),
        structure_kind="table",
        fields=[
            FieldInspection(
                field_name=field,
                inferred_type="unknown",
                sample_values=(samples or {}).get(field, []),
            )
            for field in fields
        ],
    )


def test_route_quote_cannot_be_current_assignment() -> None:
    proposal = MappingEngine().propose(
        [
            _inspection(
                "route-quotes.csv",
                ["origin_id", "destination_id", "price_per_vehicle", "currency"],
            )
        ]
    )

    assert [item.role for item in proposal.proposals] == [SourceRole.ROUTE_QUOTE]


def test_mapping_retains_numeric_transform() -> None:
    proposal = MappingEngine().propose(
        [_inspection("demand.csv", ["city_id", "city_name", "demand_quantity"])]
    )

    demand = next(item for item in proposal.proposals if item.role == SourceRole.DEMAND)
    quantity = next(
        item for item in demand.field_candidates if item.target_field == "demand_quantity"
    )
    assert quantity.transform.kind == TransformKind.PARSE_INTEGER


def test_route_mapping_retains_provided_distance_duration_and_method() -> None:
    proposal = MappingEngine().propose(
        [
            _inspection(
                "routes.csv",
                [
                    "origin_id",
                    "destination_id",
                    "layer",
                    "distance_km",
                    "duration_hours",
                    "price_per_vehicle",
                    "currency",
                    "vehicle_capacity",
                    "method",
                ],
            )
        ]
    )
    route = next(item for item in proposal.proposals if item.role == SourceRole.ROUTE_QUOTE)
    mappings = {item.target_field: item for item in route.field_candidates}

    assert mappings["distance_km"].transform.kind == TransformKind.PARSE_DECIMAL
    assert mappings["duration_hours"].transform.kind == TransformKind.PARSE_DECIMAL
    assert mappings["method"].transform.kind == TransformKind.TRIM


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


def test_existing_flag_disambiguates_warehouse_role() -> None:
    proposal = MappingEngine().propose(
        [
            _inspection(
                "warehouses.csv",
                [
                    "warehouse_id",
                    "warehouse_name",
                    "warehouse_type",
                    "city_id",
                    "city_name",
                    "is_existing",
                ],
                {"is_existing": ["true", "true"]},
            )
        ]
    )

    assert [item.role for item in proposal.proposals] == [SourceRole.EXISTING_WAREHOUSE]


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
