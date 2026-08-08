"""Case-owned warehouse scenario and facility-location operations."""

from __future__ import annotations

from pathlib import Path
from uuid import UUID

from .case_repository import CaseRepository
from .case_types import CaseOperationResult
from .optimization_models import PMedianSolution, ScenarioResult, ScenarioSpec
from .solver import (
    SolverUnavailable,
    enumerate_p_median,
    service_metrics,
    solve_assignment,
    summarize_assignment_cost,
)


class NetworkScenarioService:
    def __init__(self, repository: CaseRepository):
        self.repository = repository

    def evaluate(
        self,
        case_id: UUID,
        workspace_root: Path,
        spec: ScenarioSpec,
    ) -> tuple[ScenarioResult, CaseOperationResult]:
        normalized, normalized_id = self.repository.load_normalized_input(case_id, workspace_root)
        routes, routes_id = self.repository.load_route_matrix(case_id, workspace_root)
        costs = None
        costs_id = None
        if spec.objective == "min_cost":
            costs, costs_id = self.repository.load_cost_matrix(case_id, workspace_root)

        warehouse_by_id = {warehouse.warehouse_id: warehouse for warehouse in normalized.warehouses}
        add_ids = set(spec.add_warehouse_ids)
        remove_ids = set(spec.remove_warehouse_ids)
        for relocation in spec.relocations:
            remove_ids.add(relocation.remove_warehouse_id)
            add_ids.add(relocation.add_warehouse_id)
        referenced = add_ids | remove_ids
        unknown = referenced - set(warehouse_by_id)
        if unknown:
            raise ValueError(f"scenario_unknown_warehouses:{','.join(sorted(unknown))}")
        invalid_adds = {
            warehouse_id for warehouse_id in add_ids if warehouse_by_id[warehouse_id].is_existing
        }
        if invalid_adds:
            raise ValueError("scenario_add_requires_candidates:" + ",".join(sorted(invalid_adds)))
        invalid_removals = {
            warehouse_id
            for warehouse_id in remove_ids
            if not warehouse_by_id[warehouse_id].is_existing
        }
        if invalid_removals:
            raise ValueError(
                "scenario_remove_requires_existing:" + ",".join(sorted(invalid_removals))
            )
        overlap = add_ids & remove_ids
        if overlap:
            raise ValueError(f"scenario_add_remove_conflict:{','.join(sorted(overlap))}")

        active = {
            warehouse.warehouse_id for warehouse in normalized.warehouses if warehouse.is_existing
        }
        active.difference_update(remove_ids)
        active.update(add_ids)
        assignment = solve_assignment(
            normalized.demand_cities,
            normalized.warehouses,
            routes,
            costs,
            spec.objective,
            active,
        )
        scenario = ScenarioResult(
            assignment=assignment,
            cost=summarize_assignment_cost(assignment, costs) if costs is not None else None,
            service=service_metrics(assignment, sorted(set(spec.service_targets))),
            warehouse_changes={
                "added": sorted(add_ids),
                "removed": sorted(remove_ids),
            },
        )
        lease = self.repository.begin_operation(
            case_id,
            workspace_root,
            "evaluate_facility_scenario",
            {
                "normalized_component_id": str(normalized_id),
                "route_component_id": str(routes_id),
                "cost_component_id": str(costs_id) if costs_id else None,
                "scenario": spec.model_dump(mode="json"),
            },
        )
        if lease.reused_component_id is not None:
            stored, _ = self.repository.load_scenario(case_id, workspace_root)
            return stored, self.repository.complete_operation(lease, workspace_root, [])
        dependencies = [normalized_id, routes_id]
        if costs_id is not None:
            dependencies.append(costs_id)
        result = self.repository.commit_scenario(
            lease, workspace_root, spec, scenario, dependencies
        )
        return scenario, result


class FacilityLocationService:
    def __init__(self, repository: CaseRepository):
        self.repository = repository

    def solve(
        self,
        case_id: UUID,
        workspace_root: Path,
        *,
        number_to_open: int,
        fixed_existing_ids: set[str],
        optional_existing_ids: set[str],
        time_limit_seconds: float,
        service_constraints: list[tuple[float, float]] | None = None,
    ) -> tuple[PMedianSolution, CaseOperationResult]:
        normalized, normalized_id = self.repository.load_normalized_input(case_id, workspace_root)
        routes, routes_id = self.repository.load_route_matrix(case_id, workspace_root)
        costs, costs_id = self.repository.load_cost_matrix(case_id, workspace_root)
        warehouses = {item.warehouse_id: item for item in normalized.warehouses}
        unknown = (fixed_existing_ids | optional_existing_ids) - set(warehouses)
        if unknown:
            raise ValueError(f"facility_unknown_warehouses:{','.join(sorted(unknown))}")
        invalid_existing = {
            warehouse_id
            for warehouse_id in fixed_existing_ids | optional_existing_ids
            if not warehouses[warehouse_id].is_existing
        }
        if invalid_existing:
            raise ValueError(
                "facility_existing_policy_requires_existing_warehouses:"
                + ",".join(sorted(invalid_existing))
            )
        overlap = fixed_existing_ids & optional_existing_ids
        if overlap:
            raise ValueError(f"facility_fixed_optional_conflict:{','.join(sorted(overlap))}")

        operation_input = {
            "normalized_component_id": str(normalized_id),
            "route_component_id": str(routes_id),
            "cost_component_id": str(costs_id),
            "number_to_open": number_to_open,
            "fixed_existing_ids": sorted(fixed_existing_ids),
            "optional_existing_ids": sorted(optional_existing_ids),
            "time_limit_seconds": time_limit_seconds,
            "service_constraints": service_constraints or [],
        }
        lease = self.repository.begin_operation(
            case_id, workspace_root, "solve_facility_location", operation_input
        )
        if lease.reused_component_id is not None:
            stored, _ = self.repository.load_facility_solution(case_id, workspace_root)
            return stored, self.repository.complete_operation(lease, workspace_root, [])
        try:
            best, branches, timed_out = enumerate_p_median(
                normalized.demand_cities,
                normalized.warehouses,
                routes,
                costs,
                number_to_open,
                fixed_existing_ids,
                optional_existing_ids,
                time_limit_seconds,
                service_constraints=service_constraints,
            )
        except SolverUnavailable as error:
            solution = PMedianSolution(
                status="unavailable",
                selected_warehouse_ids=[],
                optimality="not_available",
                message=str(error),
            )
        else:
            if best is None:
                solution = PMedianSolution(
                    status="timeout" if timed_out else "infeasible",
                    selected_warehouse_ids=[],
                    optimality="not_available",
                    message=f"branches={branches}",
                )
            else:
                objective_value, selected, assignment = best
                solution = PMedianSolution(
                    status="timeout" if timed_out else "optimal",
                    selected_warehouse_ids=sorted(selected),
                    assignment=assignment,
                    objective_value=objective_value,
                    service=service_metrics(
                        assignment,
                        sorted({target for target, _ in service_constraints or []}),
                    ),
                    optimality="feasible_only" if timed_out else "proven",
                    message=f"branches={branches}",
                )
        result = self.repository.commit_facility_solution(
            lease,
            workspace_root,
            solution,
            [normalized_id, routes_id, costs_id],
        )
        return solution, result
