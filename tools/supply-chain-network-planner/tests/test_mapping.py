from __future__ import annotations

from uuid import uuid4

from supply_chain_planner.case_types import SourceSummary
from supply_chain_planner.mapping import MappingEngine, SourceRole, TransformKind
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
