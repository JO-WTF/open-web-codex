"""Case-based baseline analysis without model-visible row exchange."""

from __future__ import annotations

from pathlib import Path
from typing import Literal
from uuid import UUID

from .case_repository import CaseRepository, CaseRepositoryError
from .case_types import CaseOperationResult
from .optimization_models import AssignmentObjective, BaselineResult
from .solver import (
    service_metrics,
    solve_assignment,
    solve_current_assignment,
    summarize_assignment_cost,
)

CoverageMode = Literal["actual_if_available", "optimized_existing_footprint"]


class NetworkAnalysisService:
    def __init__(self, repository: CaseRepository):
        self.repository = repository

    def evaluate_baseline(
        self,
        case_id: UUID,
        workspace_root: Path,
        objective: AssignmentObjective,
        service_targets: list[float],
        coverage_mode: CoverageMode,
        include_cost: bool,
    ) -> tuple[BaselineResult, CaseOperationResult]:
        if not service_targets or any(target <= 0 for target in service_targets):
            raise ValueError("service_targets_must_be_positive")
        normalized, normalized_component_id = self.repository.load_normalized_input(
            case_id, workspace_root
        )
        routes, route_component_id = self.repository.load_route_matrix(
            case_id, workspace_root
        )
        costs = None
        cost_component_id = None
        if objective == "min_cost" or include_cost:
            try:
                costs, cost_component_id = self.repository.load_cost_matrix(
                    case_id, workspace_root
                )
            except CaseRepositoryError as error:
                if objective == "min_cost" or error.code != "cost_matrix_not_ready":
                    raise
        has_current = bool(normalized.current_assignments)
        use_current = coverage_mode == "actual_if_available" and has_current
        if use_current:
            assignment = solve_current_assignment(
                normalized.demand_cities,
                normalized.warehouses,
                normalized.current_assignments,
                routes,
                costs,
                objective,
            )
            label = "actual_current"
            notice_code = None
        else:
            existing_ids = {
                warehouse.warehouse_id
                for warehouse in normalized.warehouses
                if warehouse.is_existing
            }
            assignment = solve_assignment(
                normalized.demand_cities,
                normalized.warehouses,
                routes,
                costs,
                objective,
                existing_ids,
            )
            label = "optimized_existing_footprint"
            notice_code = (
                "current_assignment_missing" if not has_current else None
            )
        baseline = BaselineResult(
            label=label,
            assignment=assignment,
            service=service_metrics(assignment, sorted(set(service_targets))),
            cost=(
                summarize_assignment_cost(assignment, costs)
                if costs is not None and include_cost
                else None
            ),
            notice_code=notice_code,
        )
        lease = self.repository.begin_operation(
            case_id,
            workspace_root,
            "evaluate_network_baseline",
            {
                "normalized_component_id": str(normalized_component_id),
                "route_component_id": str(route_component_id),
                "cost_component_id": (
                    str(cost_component_id) if cost_component_id is not None else None
                ),
                "objective": objective,
                "service_targets": sorted(set(service_targets)),
                "coverage_mode": coverage_mode,
                "include_cost": include_cost,
            },
        )
        if lease.reused_component_id is not None:
            stored, _ = self.repository.load_baseline(case_id, workspace_root)
            return stored, self.repository.complete_operation(lease, workspace_root, [])
        dependencies = [normalized_component_id, route_component_id]
        if cost_component_id is not None:
            dependencies.append(cost_component_id)
        result = self.repository.commit_baseline(
            lease, workspace_root, baseline, dependencies
        )
        return baseline, result
