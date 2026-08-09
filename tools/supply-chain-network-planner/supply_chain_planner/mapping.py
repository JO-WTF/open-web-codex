"""Deterministic source-role and field mapping for network input."""

from __future__ import annotations

import hashlib
import json
import re
from collections import defaultdict
from collections.abc import Sequence
from dataclasses import dataclass, replace
from decimal import Decimal, InvalidOperation
from enum import StrEnum
from typing import TYPE_CHECKING

from pydantic import BaseModel, ConfigDict, Field

if TYPE_CHECKING:
    from .network_data import SourceInspection


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


class FieldMappingCandidate(MappingContract):
    candidate_id: str = Field(pattern=r"^[0-9a-f]{64}$")
    source_id: str
    source_revision: int = Field(ge=1)
    source_role: SourceRole
    target_entity: str
    target_field: str
    source_field: str
    transform: TransformSpec
    score: float = Field(ge=0, le=1)
    reason_code: str


class RoleMappingProposal(MappingContract):
    source_id: str
    source_name: str
    role: SourceRole
    confidence: float = Field(ge=0, le=1)
    complete: bool
    ambiguous: bool
    field_candidates: list[FieldMappingCandidate]


class MappingProposal(MappingContract):
    proposals: list[RoleMappingProposal]


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
        "layer": ("layer", "network_layer"),
        "price_per_vehicle": ("price_per_vehicle", "vehicle_price", "price", "cost"),
        "currency": ("currency", "currency_code"),
        "vehicle_capacity": ("vehicle_capacity", "capacity"),
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

TARGET_ENTITIES = {
    SourceRole.DEMAND: "demand_city",
    SourceRole.EXISTING_WAREHOUSE: "warehouse",
    SourceRole.CANDIDATE_WAREHOUSE: "warehouse",
    SourceRole.CURRENT_ASSIGNMENT: "current_assignment",
    SourceRole.ROUTE_QUOTE: "route_quote",
}


def _normalized(value: str) -> str:
    return re.sub(r"[^a-z0-9]+", "_", value.strip().lower()).strip("_")


def _candidate_id(payload: dict[str, object]) -> str:
    raw = json.dumps(payload, sort_keys=True, separators=(",", ":")).encode()
    return hashlib.sha256(raw).hexdigest()


def _transform_for(field: str) -> TransformSpec:
    if field in {"demand_quantity"}:
        return TransformSpec(kind=TransformKind.PARSE_INTEGER)
    if field in {"longitude", "latitude", "price_per_vehicle", "vehicle_capacity"}:
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


def suggest_role_mappings(fields: Sequence[FieldObservation]) -> list[SuggestedRoleMapping]:
    """Suggest domain roles from validated structural fields without source identity.

    This is the single mapping algorithm used by both the active Data surface
    and the legacy Case adapter.  It never selects a role or a field when more
    than one alias is present; callers must confirm an ambiguous suggestion.
    """

    columns: dict[str, list[FieldObservation]] = defaultdict(list)
    for field in fields:
        columns[_normalized(field.name)].append(field)
    existing_values = {
        value.strip().lower()
        for key in ("is_existing", "existing")
        for field in columns.get(key, [])
        for value in field.sample_values
    }
    proposals: list[SuggestedRoleMapping] = []
    for role, targets in TARGET_ALIASES.items():
        if role not in REQUIRED_FIELDS:
            continue
        if role == SourceRole.CURRENT_ASSIGNMENT and columns.keys() & {
            "warehouse_type",
            "warehouse_name",
            "facility_type",
            "is_existing",
            "is_fixed",
        }:
            continue
        if role == SourceRole.EXISTING_WAREHOUSE and existing_values and existing_values <= {
            "false",
            "0",
            "no",
            "n",
        }:
            continue
        if role == SourceRole.CANDIDATE_WAREHOUSE and existing_values and existing_values <= {
            "true",
            "1",
            "yes",
            "y",
        }:
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
        if not REQUIRED_FIELDS[role].issubset(matched_targets):
            continue
        confidence = sum(item.score for item in field_mappings) / max(1, len(field_mappings))
        proposals.append(
            SuggestedRoleMapping(
                role=role,
                confidence=confidence,
                ambiguous=ambiguous,
                field_mappings=tuple(field_mappings),
            )
        )
    ambiguous_roles = _ambiguous_roles(proposals)
    return [
        replace(item, ambiguous=True) if item.role in ambiguous_roles else item
        for item in proposals
    ]


def _ambiguous_roles(proposals: Sequence[SuggestedRoleMapping]) -> set[SourceRole]:
    if len(proposals) <= 1:
        return set()
    top = max(item.confidence for item in proposals)
    contenders = [item for item in proposals if top - item.confidence < 0.05]
    return {item.role for item in contenders} if len(contenders) > 1 else set()


class SourceRoleClassifier:
    def classify(self, inspection: SourceInspection) -> list[RoleMappingProposal]:
        proposals: list[RoleMappingProposal] = []
        suggestions = suggest_role_mappings(
            [
                FieldObservation(
                    name=field.field_name,
                    sample_values=tuple(field.sample_values),
                )
                for field in inspection.fields
            ]
        )
        for suggestion in suggestions:
            field_candidates: list[FieldMappingCandidate] = []
            for field_mapping in suggestion.field_mappings:
                source_field = field_mapping.source_fields[0]
                payload = {
                    "source_id": str(inspection.source.source_id),
                    "source_revision": inspection.source.source_revision,
                    "source_role": suggestion.role.value,
                    "target_entity": TARGET_ENTITIES[suggestion.role],
                    "target_field": field_mapping.target_field,
                    "source_field": source_field,
                    "transform": field_mapping.transform.model_dump(mode="json"),
                }
                field_candidates.append(
                    FieldMappingCandidate(
                        candidate_id=_candidate_id(payload),
                        source_id=str(inspection.source.source_id),
                        source_revision=inspection.source.source_revision,
                        source_role=suggestion.role,
                        target_entity=TARGET_ENTITIES[suggestion.role],
                        target_field=field_mapping.target_field,
                        source_field=source_field,
                        transform=field_mapping.transform,
                        score=field_mapping.score,
                        reason_code=field_mapping.reason_code,
                    )
                )
            proposals.append(
                RoleMappingProposal(
                    source_id=str(inspection.source.source_id),
                    source_name=inspection.source.display_name,
                    role=suggestion.role,
                    confidence=suggestion.confidence,
                    complete=True,
                    ambiguous=suggestion.ambiguous,
                    field_candidates=field_candidates,
                )
            )
        return proposals


class MappingEngine:
    def propose(self, inspections: list[SourceInspection]) -> MappingProposal:
        classifier = SourceRoleClassifier()
        proposals = [
            proposal
            for inspection in inspections
            for proposal in classifier.classify(inspection)
        ]
        return MappingProposal(
            proposals=sorted(
                proposals,
                key=lambda item: (item.source_name, -item.confidence, item.role.value),
            )
        )


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
