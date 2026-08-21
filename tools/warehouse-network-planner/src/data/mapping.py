"""Deterministic source-role and field mapping for network input."""

from __future__ import annotations

import re
from collections import defaultdict
from collections.abc import Sequence
from dataclasses import dataclass, replace
from decimal import Decimal, InvalidOperation
from enum import StrEnum
from typing import Literal

from pydantic import BaseModel, ConfigDict


class MappingContract(BaseModel):
    model_config = ConfigDict(extra="forbid")


class SourceRole(StrEnum):
    DEMAND = "demand"
    EXISTING_WAREHOUSE = "existing_warehouse"
    CANDIDATE_WAREHOUSE = "candidate_warehouse"
    CURRENT_ASSIGNMENT = "current_assignment"
    ROUTE_QUOTE = "route_quote"
    ADMINISTRATIVE_CATALOG = "administrative_catalog"


class TransformKind(StrEnum):
    IDENTITY = "identity"
    TRIM = "trim"
    PARSE_INTEGER = "parse_integer"
    PARSE_DECIMAL = "parse_decimal"
    NORMALIZE_IDENTIFIER = "normalize_identifier"
    NORMALIZE_WAREHOUSE_TYPE = "normalize_warehouse_type"
    PARSE_BOOLEAN = "parse_boolean"
    DIVIDE_CONSTANT = "divide_constant"
    MULTIPLY_CONSTANT = "multiply_constant"
    ADMINISTRATIVE_LOOKUP = "administrative_lookup"
    COORDINATE_LOOKUP = "coordinate_lookup"


class TransformSpec(MappingContract):
    kind: TransformKind
    factor: Decimal | None = None


TARGET_ALIASES: dict[SourceRole, dict[str, tuple[str, ...]]] = {
    SourceRole.DEMAND: {
        "city_id": ("city_id", "demand_city_id", "city_code"),
        "city_name": ("city_name", "demand_city_name", "city"),
        "province_id": ("province_id", "province_code", "region_id"),
        "province_name": ("province_name", "province", "region"),
        "demand_quantity": ("demand_quantity", "demand_units", "quantity", "demand"),
        "longitude": ("longitude", "lon", "lng"),
        "latitude": ("latitude", "lat"),
    },
    SourceRole.EXISTING_WAREHOUSE: {
        "warehouse_id": ("warehouse_id", "facility_id", "site_id"),
        "warehouse_name": ("warehouse_name", "facility_name", "site_name"),
        "warehouse_type": ("warehouse_type", "facility_type", "site_type", "type"),
        "city_id": ("city_id", "city_code"),
        "city_name": ("city_name", "city"),
        "province_id": ("province_id", "province_code", "region_id"),
        "province_name": ("province_name", "province", "region"),
        "longitude": ("longitude", "lon", "lng"),
        "latitude": ("latitude", "lat"),
        "upstream_center_id": ("upstream_center_id", "center_id"),
        "is_existing": ("is_existing", "existing"),
        "is_fixed": ("is_fixed", "fixed"),
    },
    SourceRole.CANDIDATE_WAREHOUSE: {
        "warehouse_id": ("warehouse_id", "facility_id", "site_id"),
        "warehouse_name": ("warehouse_name", "facility_name", "site_name"),
        "warehouse_type": ("warehouse_type", "facility_type", "site_type", "type"),
        "city_id": ("city_id", "city_code"),
        "city_name": ("city_name", "city"),
        "province_id": ("province_id", "province_code", "region_id"),
        "province_name": ("province_name", "province", "region"),
        "longitude": ("longitude", "lon", "lng"),
        "latitude": ("latitude", "lat"),
        "upstream_center_id": ("upstream_center_id", "center_id"),
        "is_existing": ("is_existing", "existing"),
        "is_fixed": ("is_fixed", "fixed"),
    },
    SourceRole.CURRENT_ASSIGNMENT: {
        "demand_city_id": ("demand_city_id", "customer_city_id", "city_id"),
        "serving_warehouse_id": (
            "serving_warehouse_id",
            "assigned_warehouse_id",
            "warehouse_id",
        ),
        "upstream_center_id": ("upstream_center_id", "center_id"),
    },
    SourceRole.ROUTE_QUOTE: {
        "origin_id": ("origin_id", "origin_city_id", "ori_city_id"),
        "destination_id": ("destination_id", "destination_city_id", "dest_city_id"),
        "destination_name": ("destination_name", "destination_city_name", "dest_city_name"),
        "layer": ("layer", "network_layer"),
        "distance_km": ("distance_km", "route_distance_km"),
        "duration_hours": ("duration_hours", "travel_time_hours"),
        "price_per_vehicle": ("price_per_vehicle", "vehicle_price", "price", "cost"),
        "currency": ("currency", "currency_code"),
        "vehicle_capacity": ("vehicle_capacity", "capacity"),
        "method": ("method", "route_method", "source_method"),
    },
}

REQUIRED_FIELDS: dict[SourceRole, frozenset[str]] = {
    SourceRole.DEMAND: frozenset({"city_id", "city_name", "demand_quantity"}),
    SourceRole.EXISTING_WAREHOUSE: frozenset(
        {"warehouse_id", "warehouse_name", "warehouse_type", "city_id", "city_name"}
    ),
    SourceRole.CANDIDATE_WAREHOUSE: frozenset(
        {"warehouse_id", "warehouse_name", "warehouse_type", "city_id", "city_name"}
    ),
    SourceRole.CURRENT_ASSIGNMENT: frozenset(
        {"demand_city_id", "serving_warehouse_id"}
    ),
    SourceRole.ROUTE_QUOTE: frozenset(
        {"origin_id", "destination_id", "price_per_vehicle"}
    ),
}

# A role must have a domain-specific anchor before a partial assessment is
# useful.  Generic city columns occur in administrative, population and other
# supporting files and are not evidence of demand on their own.
ROLE_ANCHORS: dict[SourceRole, frozenset[str]] = {
    SourceRole.DEMAND: frozenset(
        {"demand_quantity", "demand_units", "quantity", "demand"}
    ),
    SourceRole.EXISTING_WAREHOUSE: frozenset(
        {
            "warehouse_id",
            "facility_id",
            "site_id",
            "warehouse_name",
            "facility_name",
            "site_name",
            "warehouse_type",
            "facility_type",
            "site_type",
            "is_existing",
            "existing",
            "is_fixed",
            "fixed",
        }
    ),
    SourceRole.CANDIDATE_WAREHOUSE: frozenset(
        {
            "warehouse_id",
            "facility_id",
            "site_id",
            "warehouse_name",
            "facility_name",
            "site_name",
            "warehouse_type",
            "facility_type",
            "site_type",
            "is_existing",
            "existing",
            "is_fixed",
            "fixed",
        }
    ),
    SourceRole.CURRENT_ASSIGNMENT: frozenset(
        {
            "demand_city_id",
            "customer_city_id",
            "serving_warehouse_id",
            "assigned_warehouse_id",
            "warehouse_id",
            "upstream_center_id",
            "center_id",
        }
    ),
    SourceRole.ROUTE_QUOTE: frozenset(
        {
            "origin_id",
            "origin_city_id",
            "ori_city_id",
            "destination_id",
            "destination_city_id",
            "dest_city_id",
            "price_per_vehicle",
            "vehicle_price",
            "price",
            "cost",
        }
    ),
}

def _normalized(value: str) -> str:
    return re.sub(r"[^a-z0-9]+", "_", value.strip().lower()).strip("_")


def _transform_for(field: str) -> TransformSpec:
    if field in {"demand_quantity"}:
        return TransformSpec(kind=TransformKind.PARSE_INTEGER)
    if field in {
        "longitude",
        "latitude",
        "price_per_vehicle",
        "vehicle_capacity",
        "distance_km",
        "duration_hours",
    }:
        return TransformSpec(kind=TransformKind.PARSE_DECIMAL)
    if field == "warehouse_type":
        return TransformSpec(kind=TransformKind.NORMALIZE_WAREHOUSE_TYPE)
    if field in {"is_existing", "is_fixed"}:
        return TransformSpec(kind=TransformKind.PARSE_BOOLEAN)
    if field.endswith("_id"):
        return TransformSpec(kind=TransformKind.NORMALIZE_IDENTIFIER)
    return TransformSpec(kind=TransformKind.TRIM)


@dataclass(frozen=True)
class FieldObservation:
    """One structurally inspected field supplied by the owning IO layer."""

    name: str
    sample_values: tuple[str, ...] = ()


@dataclass(frozen=True)
class SuggestedFieldMapping:
    target_field: str
    source_fields: tuple[str, ...]
    transform: TransformSpec
    score: float
    reason_code: str


@dataclass(frozen=True)
class SuggestedRoleMapping:
    role: SourceRole
    confidence: float
    ambiguous: bool
    field_mappings: tuple[SuggestedFieldMapping, ...]


@dataclass(frozen=True)
class SuggestedRoleAssessment:
    """Bounded role evidence for one independently inspectable source unit."""

    role: SourceRole
    state: Literal["complete", "partial", "ambiguous"]
    confidence: float
    ambiguous: bool
    matched_required_fields: tuple[str, ...]
    missing_required_fields: tuple[str, ...]
    field_mappings: tuple[SuggestedFieldMapping, ...]


@dataclass(frozen=True)
class _RoleMappingAssessment:
    role: SourceRole
    confidence: float
    ambiguous: bool
    field_mappings: tuple[SuggestedFieldMapping, ...]
    matched_required_fields: frozenset[str]
    missing_required_fields: frozenset[str]


def suggest_role_mappings(fields: Sequence[FieldObservation]) -> list[SuggestedRoleMapping]:
    """Suggest domain roles from validated structural fields without source identity.

    This is the single mapping algorithm used by both the active Data surface
    and the legacy Case adapter.  It never selects a role or a field when more
    than one alias is present; callers must confirm an ambiguous suggestion.
    """

    proposals = [
        SuggestedRoleMapping(
            role=assessment.role,
            confidence=assessment.confidence,
            ambiguous=assessment.ambiguous,
            field_mappings=assessment.field_mappings,
        )
        for assessment in _assess_role_mappings(fields)
        if not assessment.missing_required_fields
    ]
    ambiguous_roles = _ambiguous_roles(proposals)
    return [
        replace(item, ambiguous=True) if item.role in ambiguous_roles else item
        for item in proposals
    ]


def assess_role_mappings(fields: Sequence[FieldObservation]) -> list[SuggestedRoleAssessment]:
    """Return complete/partial/ambiguous role evidence without blocking a turn.

    A partial role is evidence for a later, explicitly selected source, not a
    global request for user input during inspection.
    """

    assessments = _assess_role_mappings(fields)
    complete = [
        SuggestedRoleMapping(
            role=item.role,
            confidence=item.confidence,
            ambiguous=item.ambiguous,
            field_mappings=item.field_mappings,
        )
        for item in assessments
        if not item.missing_required_fields
    ]
    ambiguous_roles = _ambiguous_roles(complete)
    return [
        SuggestedRoleAssessment(
            role=item.role,
            state=(
                "partial"
                if item.missing_required_fields
                else "ambiguous"
                if item.role in ambiguous_roles or item.ambiguous
                else "complete"
            ),
            confidence=item.confidence,
            ambiguous=item.role in ambiguous_roles or item.ambiguous,
            matched_required_fields=tuple(sorted(item.matched_required_fields)),
            missing_required_fields=tuple(sorted(item.missing_required_fields)),
            field_mappings=item.field_mappings,
        )
        for item in assessments
    ]


def _assess_role_mappings(fields: Sequence[FieldObservation]) -> list[_RoleMappingAssessment]:
    columns: dict[str, list[FieldObservation]] = defaultdict(list)
    for field in fields:
        columns[_normalized(field.name)].append(field)
    existing_values = {
        value.strip().lower()
        for key in ("is_existing", "existing")
        for field in columns.get(key, [])
        for value in field.sample_values
    }
    assessments: list[_RoleMappingAssessment] = []
    for role, targets in TARGET_ALIASES.items():
        if role not in REQUIRED_FIELDS or not _role_is_applicable(
            role, set(columns), existing_values
        ):
            continue
        field_mappings: list[SuggestedFieldMapping] = []
        matched_targets: set[str] = set()
        ambiguous = False
        for target_field, aliases in targets.items():
            matches = list(
                dict.fromkeys(
                    field.name
                    for alias in aliases
                    for field in columns.get(alias, [])
                )
            )
            if not matches:
                continue
            ambiguous = ambiguous or len(matches) > 1
            source_field = matches[0]
            matched_targets.add(target_field)
            exact = _normalized(source_field) == target_field
            field_mappings.append(
                SuggestedFieldMapping(
                    target_field=target_field,
                    source_fields=tuple(matches),
                    transform=_transform_for(target_field),
                    score=1.0 if exact else 0.9,
                    reason_code="exact_name" if exact else "alias",
                )
            )
        required = REQUIRED_FIELDS[role]
        matched_required = frozenset(required & matched_targets)
        missing_required = frozenset(required - matched_targets)
        if not matched_required:
            continue
        mapping_confidence = sum(item.score for item in field_mappings) / max(
            1, len(field_mappings)
        )
        coverage = len(matched_required) / len(required)
        assessments.append(
            _RoleMappingAssessment(
                role=role,
                confidence=mapping_confidence * coverage,
                ambiguous=ambiguous,
                field_mappings=tuple(field_mappings),
                matched_required_fields=matched_required,
                missing_required_fields=missing_required,
            )
        )
    return assessments


def _role_is_applicable(
    role: SourceRole,
    column_names: set[str],
    existing_values: set[str],
) -> bool:
    if role in ROLE_ANCHORS and not column_names & ROLE_ANCHORS[role]:
        return False
    if role == SourceRole.DEMAND and not column_names & {
        "city_id",
        "demand_city_id",
        "city_code",
        "city_name",
        "demand_city_name",
        "city",
    }:
        return False
    if role == SourceRole.ROUTE_QUOTE and not column_names & {
        "origin_id",
        "origin_city_id",
        "ori_city_id",
        "destination_id",
        "destination_city_id",
        "dest_city_id",
    }:
        return False
    if role == SourceRole.CURRENT_ASSIGNMENT and column_names & {
        "warehouse_type",
        "warehouse_name",
        "facility_type",
        "is_existing",
        "is_fixed",
    }:
        return False
    if role == SourceRole.EXISTING_WAREHOUSE and existing_values and existing_values <= {
        "false",
        "0",
        "no",
        "n",
    }:
        return False
    if role == SourceRole.CANDIDATE_WAREHOUSE and existing_values and existing_values <= {
        "true",
        "1",
        "yes",
        "y",
    }:
        return False
    return True


def _ambiguous_roles(proposals: Sequence[SuggestedRoleMapping]) -> set[SourceRole]:
    if len(proposals) <= 1:
        return set()
    top = max(item.confidence for item in proposals)
    contenders = [item for item in proposals if top - item.confidence < 0.05]
    return {item.role for item in contenders} if len(contenders) > 1 else set()


class TransformRegistry:
    def apply(self, spec: TransformSpec, value: object) -> object:
        if value is None:
            return None
        if spec.kind == TransformKind.IDENTITY:
            return value
        text = str(value).strip()
        if spec.kind == TransformKind.TRIM:
            return text
        if spec.kind == TransformKind.NORMALIZE_IDENTIFIER:
            return text
        if spec.kind == TransformKind.PARSE_INTEGER:
            number = self._decimal(text)
            if number != number.to_integral_value():
                raise ValueError("integer_transform_fractional")
            return int(number)
        if spec.kind == TransformKind.PARSE_DECIMAL:
            return self._decimal(text)
        if spec.kind == TransformKind.PARSE_BOOLEAN:
            normalized = text.lower()
            if normalized in {"true", "1", "yes", "y"}:
                return True
            if normalized in {"false", "0", "no", "n"}:
                return False
            raise ValueError("boolean_transform_invalid")
        if spec.kind == TransformKind.NORMALIZE_WAREHOUSE_TYPE:
            normalized = _normalized(text)
            if normalized in {"center", "central", "central_warehouse"}:
                return "center"
            if normalized in {"cross_docking", "cross_dock", "forward_warehouse"}:
                return "cross_docking"
            raise ValueError("warehouse_type_invalid")
        if spec.kind in {TransformKind.DIVIDE_CONSTANT, TransformKind.MULTIPLY_CONSTANT}:
            if spec.factor is None or spec.factor == 0:
                raise ValueError("transform_factor_invalid")
            number = self._decimal(text)
            return number / spec.factor if spec.kind == TransformKind.DIVIDE_CONSTANT else number * spec.factor
        raise ValueError(f"transform_requires_external_service:{spec.kind.value}")

    @staticmethod
    def _decimal(value: str) -> Decimal:
        try:
            return Decimal(value.replace(",", ""))
        except InvalidOperation as error:
            raise ValueError("decimal_transform_invalid") from error
