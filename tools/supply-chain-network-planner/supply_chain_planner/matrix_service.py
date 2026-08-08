"""Case-owned orchestration for route and transport-cost matrices."""

from __future__ import annotations

from pathlib import Path
from typing import Literal
from uuid import UUID

from .case_repository import CaseRepository, CaseRepositoryError
from .case_types import CaseOperationResult, FacetName, FacetState
from .matrix import (
    build_cost_matrix,
    build_haversine_route_matrix,
    plan_route_matrix,
    register_navigation_route_matrix,
)
from .matrix_models import (
    CostMatrix,
    RouteMatrix,
    RouteMatrixPlan,
    RouteMatrixRow,
    WarehouseScope,
)


class CaseMatrixService:
    def __init__(self, repository: CaseRepository):
        self.repository = repository

    def plan_routes(
        self,
        case_id: UUID,
        workspace_root: Path,
        method: Literal["haversine", "navigation", "provided"],
        detour_coefficient: float | None,
        average_speed_kph: float | None,
    ) -> RouteMatrixPlan:
        normalized, _ = self.repository.load_normalized_input(case_id, workspace_root)
        plan = plan_route_matrix(
            normalized.demand_cities,
            normalized.warehouses,
            method,
            detour_coefficient,
            average_speed_kph,
        )
        return plan.model_copy(update={"case_id": case_id})

    def build_haversine(
        self,
        case_id: UUID,
        workspace_root: Path,
        detour_coefficient: float,
        average_speed_kph: float,
    ) -> tuple[RouteMatrix, CaseOperationResult]:
        normalized, normalized_component_id = self.repository.load_normalized_input(
            case_id, workspace_root
        )
        lease = self.repository.begin_operation(
            case_id,
            workspace_root,
            "build_haversine_route_matrix",
            {
                "normalized_component_id": str(normalized_component_id),
                "detour_coefficient": detour_coefficient,
                "average_speed_kph": average_speed_kph,
            },
        )
        if lease.reused_component_id is not None:
            matrix, _ = self.repository.load_route_matrix(case_id, workspace_root)
            return matrix, self.repository.complete_operation(lease, workspace_root, [])
        matrix = build_haversine_route_matrix(
            normalized.demand_cities,
            normalized.warehouses,
            detour_coefficient,
            average_speed_kph,
        )
        result = self.repository.commit_route_matrix(
            lease, workspace_root, matrix, [normalized_component_id]
        )
        return matrix, result

    def register_navigation(
        self,
        case_id: UUID,
        workspace_root: Path,
        rows: list[RouteMatrixRow],
    ) -> tuple[RouteMatrix, CaseOperationResult | None]:
        normalized, normalized_component_id = self.repository.load_normalized_input(
            case_id, workspace_root
        )
        matrix = register_navigation_route_matrix(
            normalized.demand_cities,
            normalized.warehouses,
            rows,
        )
        if matrix.missing_routes:
            self.repository.set_facet_state(
                case_id,
                workspace_root,
                FacetName.ROUTE_MATRIX,
                FacetState.NEEDS_INPUT,
                ["navigation_matrix_incomplete"],
            )
            return matrix, None
        lease = self.repository.begin_operation(
            case_id,
            workspace_root,
            "register_navigation_route_matrix",
            {
                "normalized_component_id": str(normalized_component_id),
                "row_count": len(rows),
            },
        )
        if lease.reused_component_id is not None:
            stored, _ = self.repository.load_route_matrix(case_id, workspace_root)
            return stored, self.repository.complete_operation(lease, workspace_root, [])
        result = self.repository.commit_route_matrix(
            lease, workspace_root, matrix, [normalized_component_id]
        )
        return matrix, result

    def build_costs(
        self,
        case_id: UUID,
        workspace_root: Path,
        fallback_rule: dict[str, object] | None,
        warehouse_scope: WarehouseScope,
    ) -> tuple[CostMatrix, CaseOperationResult | None]:
        normalized, normalized_component_id = self.repository.load_normalized_input(
            case_id, workspace_root
        )
        route_matrix = None
        route_component_id = None
        try:
            route_matrix, route_component_id = self.repository.load_route_matrix(
                case_id, workspace_root
            )
        except CaseRepositoryError as error:
            if error.code != "route_matrix_not_ready":
                raise
        warehouses = (
            [item for item in normalized.warehouses if item.is_existing]
            if warehouse_scope == "existing_only"
            else normalized.warehouses
        )
        matrix = build_cost_matrix(
            normalized.demand_cities,
            warehouses,
            normalized.route_quotes,
            fallback_rule,
            route_matrix,
        ).model_copy(update={"warehouse_scope": warehouse_scope})
        if matrix.missing_routes:
            self.repository.set_facet_state(
                case_id,
                workspace_root,
                FacetName.COST_MATRIX,
                FacetState.NEEDS_INPUT,
                ["cost_rule_required"],
            )
            return matrix, None
        dependencies = [normalized_component_id]
        if route_component_id is not None:
            dependencies.append(route_component_id)
        lease = self.repository.begin_operation(
            case_id,
            workspace_root,
            "build_cost_matrix",
            {
                "normalized_component_id": str(normalized_component_id),
                "route_component_id": (
                    str(route_component_id) if route_component_id is not None else None
                ),
                "fallback_rule": fallback_rule,
                "warehouse_scope": warehouse_scope,
            },
        )
        if lease.reused_component_id is not None:
            stored, _ = self.repository.load_cost_matrix(case_id, workspace_root)
            return stored, self.repository.complete_operation(lease, workspace_root, [])
        result = self.repository.commit_cost_matrix(
            lease, workspace_root, matrix, dependencies
        )
        return matrix, result
