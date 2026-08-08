"""Typed business requirements for one network-planning request."""

from __future__ import annotations

import hashlib
import json
from decimal import Decimal
from pathlib import Path
from typing import Literal
from uuid import UUID

from pydantic import BaseModel, ConfigDict, Field

from .case_repository import CaseRepository
from .case_types import AnalysisKind, CaseOperationResult, ComponentWrite, FacetName


class RequirementContract(BaseModel):
    model_config = ConfigDict(extra="forbid")


class RequirementRequest(RequirementContract):
    requested_analyses: list[AnalysisKind] = Field(min_length=1, max_length=10)
    assignment_objective: Literal["min_time", "min_cost"] | None = None
    service_target_hours: list[Decimal] = Field(default_factory=list, max_length=10)
    driver_hours_per_day: Decimal | None = Field(default=None, gt=0, le=24)


class RequirementProfile(RequirementContract):
    requested_analyses: list[AnalysisKind]
    required_entities: list[str]
    optional_entities: list[str]
    assignment_objective: Literal["min_time", "min_cost"] | None
    service_target_hours: list[Decimal]
    driver_hours_per_day: Decimal | None


class RequirementService:
    def __init__(self, repository: CaseRepository):
        self.repository = repository

    def define(
        self,
        case_id: UUID,
        workspace_root: Path,
        request: RequirementRequest,
    ) -> tuple[RequirementProfile, CaseOperationResult]:
        required = {"demand", "existing_warehouse"}
        optional = {"candidate_warehouse", "current_assignment", "route_quote"}
        if any(
            analysis
            in {
                AnalysisKind.COST_BASELINE,
                AnalysisKind.ACTUAL_COST,
                AnalysisKind.P_MEDIAN,
                AnalysisKind.SERVICE_CONSTRAINED,
            }
            for analysis in request.requested_analyses
        ):
            required.add("cost_rule_or_route_quote")
        profile = RequirementProfile(
            requested_analyses=request.requested_analyses,
            required_entities=sorted(required),
            optional_entities=sorted(optional),
            assignment_objective=request.assignment_objective,
            service_target_hours=request.service_target_hours,
            driver_hours_per_day=request.driver_hours_per_day,
        )
        canonical = json.dumps(
            profile.model_dump(mode="json"), sort_keys=True, separators=(",", ":")
        )
        lease = self.repository.begin_operation(
            case_id,
            workspace_root,
            "define_network_requirements",
            request,
        )
        result = self.repository.complete_operation(
            lease,
            workspace_root,
            [
                ComponentWrite(
                    component_kind="requirements.v1",
                    content_sha256=hashlib.sha256(canonical.encode()).hexdigest(),
                    row_count=len(profile.required_entities) + len(profile.optional_entities),
                    metadata_json=canonical,
                    facet=FacetName.REQUIREMENTS,
                )
            ],
        )
        return profile, result
