"""Small deterministic assignment and finite-candidate facility solvers."""

from __future__ import annotations

import math
from collections import defaultdict
from collections.abc import Iterable
from typing import Any

from .case_models import CurrentAssignment, DemandCity, Warehouse
from .matrix_models import CostMatrix, RouteMatrix
from .optimization_models import (
    AssignmentObjective,
    AssignmentResult,
    AssignmentRow,
    CostSummary,
    ServiceMetric,
)

try:
    from ortools.sat.python import cp_model
except ImportError:  # pragma: no cover - exercised by the explicit unavailable path
    cp_model = None


class SolverUnavailable(RuntimeError):
    """The declared deterministic solver dependency is not installed."""


def _route_index(matrix: RouteMatrix):
    return {(row.origin_id, row.destination_id): row for row in matrix.rows}


def _cost_index(matrix: CostMatrix):
    return {
        (row.origin_id, row.destination_id, row.layer): row.cost_per_demand_unit
        for row in matrix.rows
    }


def solve_assignment(
    demand: Iterable[DemandCity],
    warehouses: Iterable[Warehouse],
    route_matrix: RouteMatrix,
    cost_matrix: CostMatrix | None,
    objective: AssignmentObjective,
    active_warehouse_ids: set[str] | None = None,
) -> AssignmentResult:
    demand_rows = sorted(demand, key=lambda row: row.city_id)
    warehouse_rows = sorted(
        (
            warehouse
            for warehouse in warehouses
            if active_warehouse_ids is None or warehouse.warehouse_id in active_warehouse_ids
        ),
        key=lambda row: row.warehouse_id,
    )
    if not warehouse_rows:
        raise ValueError("assignment_requires_active_warehouse")
    routes = _route_index(route_matrix)
    costs = _cost_index(cost_matrix) if cost_matrix is not None else {}
    rows: list[AssignmentRow] = []
    unassigned = 0
    for city in demand_rows:
        choices = []
        for warehouse in warehouse_rows:
            if (
                warehouse.upstream_center_id
                and active_warehouse_ids is not None
                and warehouse.upstream_center_id not in active_warehouse_ids
            ):
                continue
            route = routes.get((warehouse.warehouse_id, city.city_id))
            if route is None or route.status != "ready":
                continue
            cost = costs.get((warehouse.warehouse_id, city.city_id, "last_mile"))
            if objective == "min_cost" and cost is None:
                continue
            choices.append(
                (
                    cost if objective == "min_cost" else route.duration_hours,
                    route.duration_hours,
                    warehouse.warehouse_id,
                    route.distance_km,
                    cost,
                )
            )
        if not choices:
            rows.append(
                AssignmentRow(
                    demand_city_id=city.city_id,
                    warehouse_id=None,
                    demand_quantity=city.demand_quantity,
                    reason="no_ready_route_or_cost",
                )
            )
            unassigned += city.demand_quantity
            continue
        _, duration, warehouse_id, distance, cost = min(choices)
        rows.append(
            AssignmentRow(
                demand_city_id=city.city_id,
                warehouse_id=warehouse_id,
                upstream_center_id=next(
                    warehouse.upstream_center_id
                    for warehouse in warehouse_rows
                    if warehouse.warehouse_id == warehouse_id
                ),
                demand_quantity=city.demand_quantity,
                distance_km=distance,
                duration_hours=duration,
                cost=cost,
            )
        )
    total = sum(city.demand_quantity for city in demand_rows)
    return AssignmentResult(
        objective=objective,
        rows=rows,
        total_demand=total,
        unassigned_demand=unassigned,
    )


def solve_current_assignment(
    demand: Iterable[DemandCity],
    warehouses: Iterable[Warehouse],
    current_assignments: Iterable[CurrentAssignment],
    route_matrix: RouteMatrix,
    cost_matrix: CostMatrix | None,
    objective: AssignmentObjective,
) -> AssignmentResult:
    """Evaluate the recorded assignment without silently re-optimizing it."""
    demand_rows = sorted(demand, key=lambda row: row.city_id)
    warehouses_by_id = {warehouse.warehouse_id: warehouse for warehouse in warehouses}
    assignments_by_city = {
        assignment.demand_city_id: assignment for assignment in current_assignments
    }
    routes = _route_index(route_matrix)
    costs = _cost_index(cost_matrix) if cost_matrix is not None else {}
    rows: list[AssignmentRow] = []
    unassigned = 0
    for city in demand_rows:
        current = assignments_by_city.get(city.city_id)
        warehouse = warehouses_by_id.get(current.serving_warehouse_id) if current else None
        route = (
            routes.get((warehouse.warehouse_id, city.city_id)) if warehouse is not None else None
        )
        cost = (
            costs.get((warehouse.warehouse_id, city.city_id, "last_mile"))
            if warehouse is not None
            else None
        )
        if current is None:
            reason = "current_assignment_missing"
        elif warehouse is None:
            reason = "current_warehouse_missing"
        elif route is None or route.status != "ready":
            reason = "current_route_missing"
        elif objective == "min_cost" and cost is None:
            reason = "current_cost_missing"
        else:
            rows.append(
                AssignmentRow(
                    demand_city_id=city.city_id,
                    warehouse_id=warehouse.warehouse_id,
                    upstream_center_id=current.upstream_center_id,
                    demand_quantity=city.demand_quantity,
                    distance_km=route.distance_km,
                    duration_hours=route.duration_hours,
                    cost=cost,
                )
            )
            continue
        rows.append(
            AssignmentRow(
                demand_city_id=city.city_id,
                warehouse_id=warehouse.warehouse_id if warehouse is not None else None,
                upstream_center_id=current.upstream_center_id if current else None,
                demand_quantity=city.demand_quantity,
                reason=reason,
            )
        )
        unassigned += city.demand_quantity
    return AssignmentResult(
        objective=objective,
        rows=rows,
        total_demand=sum(city.demand_quantity for city in demand_rows),
        unassigned_demand=unassigned,
    )


def service_metrics(
    assignment: AssignmentResult,
    targets: Iterable[float],
) -> list[ServiceMetric]:
    total = assignment.total_demand
    return [
        ServiceMetric(
            target_hours=target,
            covered_demand=sum(
                row.demand_quantity
                for row in assignment.rows
                if row.duration_hours is not None and row.duration_hours <= target
            ),
            total_demand=total,
            coverage_rate=(
                sum(
                    row.demand_quantity
                    for row in assignment.rows
                    if row.duration_hours is not None and row.duration_hours <= target
                )
                / total
                if total
                else 0
            ),
        )
        for target in targets
    ]


def _assignment_objective_value(assignment: AssignmentResult) -> float:
    return sum(
        (row.cost or row.duration_hours or 0) * float(row.demand_quantity)
        for row in assignment.rows
        if row.warehouse_id is not None
    )


def enumerate_p_median(
    demand: list[DemandCity],
    warehouses: list[Warehouse],
    route_matrix: RouteMatrix,
    cost_matrix: CostMatrix,
    number_to_open: int,
    fixed_existing_ids: set[str],
    optional_existing_ids: set[str],
    time_limit_seconds: float,
    service_constraints: list[tuple[float, float]] | None = None,
):
    if cp_model is None:
        raise SolverUnavailable("ortools is not installed")
    if number_to_open < 0 or time_limit_seconds <= 0:
        raise ValueError("invalid_solver_parameters")

    demand_rows = sorted(demand, key=lambda row: row.city_id)
    warehouse_rows = sorted(warehouses, key=lambda row: row.warehouse_id)
    fixed = {
        warehouse.warehouse_id
        for warehouse in warehouse_rows
        if warehouse.is_existing and warehouse.is_fixed
    }
    fixed |= fixed_existing_ids
    optional = set(optional_existing_ids) - fixed
    candidates = [
        warehouse
        for warehouse in warehouse_rows
        if not warehouse.is_existing and warehouse.warehouse_id not in fixed
    ]
    if number_to_open > len(candidates):
        return None, 0, False

    routes = _route_index(route_matrix)
    costs = _cost_index(cost_matrix)
    model = cp_model.CpModel()
    open_variables: dict[str, Any] = {}
    for warehouse in warehouse_rows:
        warehouse_id = warehouse.warehouse_id
        variable = model.NewBoolVar(f"open_{warehouse_id}")
        open_variables[warehouse_id] = variable
        if warehouse_id in fixed or (warehouse.is_existing and warehouse_id not in optional):
            model.Add(variable == 1)
        elif warehouse_id not in optional and warehouse.is_existing:
            model.Add(variable == 1)
    for warehouse in warehouse_rows:
        if warehouse.upstream_center_id:
            upstream = open_variables.get(warehouse.upstream_center_id)
            if upstream is None:
                raise ValueError(f"warehouse_upstream_center_missing:{warehouse.warehouse_id}")
            model.Add(open_variables[warehouse.warehouse_id] <= upstream)
    model.Add(
        sum(open_variables[warehouse.warehouse_id] for warehouse in candidates) == number_to_open
    )

    assignment_variables: dict[tuple[str, str], Any] = {}
    objective_terms = []
    for city in demand_rows:
        choices = []
        for warehouse in warehouse_rows:
            route = routes.get((warehouse.warehouse_id, city.city_id))
            cost = costs.get((warehouse.warehouse_id, city.city_id, "last_mile"))
            if route is None or route.status != "ready" or cost is None:
                continue
            variable = model.NewBoolVar(f"assign_{city.city_id}_{warehouse.warehouse_id}")
            assignment_variables[(city.city_id, warehouse.warehouse_id)] = variable
            model.Add(variable <= open_variables[warehouse.warehouse_id])
            objective_terms.append(
                int(round(cost * 1000 * float(city.demand_quantity))) * variable
            )
            choices.append((warehouse, route))
        if not choices:
            return None, 0, False
        model.Add(
            sum(
                assignment_variables[(city.city_id, warehouse.warehouse_id)]
                for warehouse, _ in choices
            )
            == 1
        )

    for target_hours, minimum_coverage in service_constraints or []:
        if not 0 < minimum_coverage <= 1 or target_hours <= 0:
            raise ValueError("invalid_service_constraint")
        eligible = []
        for city in demand_rows:
            for warehouse in warehouse_rows:
                variable = assignment_variables.get((city.city_id, warehouse.warehouse_id))
                route = routes.get((warehouse.warehouse_id, city.city_id))
                if (
                    variable is not None
                    and route is not None
                    and route.duration_hours <= target_hours
                ):
                    eligible.append(int(city.demand_quantity) * variable)
        required = math.ceil(
            float(sum(city.demand_quantity for city in demand_rows)) * minimum_coverage
        )
        model.Add(sum(eligible) >= required)

    model.Minimize(sum(objective_terms))
    solver = cp_model.CpSolver()
    solver.parameters.max_time_in_seconds = time_limit_seconds
    solver.parameters.num_search_workers = 1
    solver.parameters.random_seed = 0
    status = solver.Solve(model)
    if status not in (cp_model.OPTIMAL, cp_model.FEASIBLE):
        return None, int(solver.NumBranches()), status == cp_model.UNKNOWN

    active = {
        warehouse_id
        for warehouse_id, variable in open_variables.items()
        if solver.Value(variable) == 1
    }
    rows: list[AssignmentRow] = []
    unassigned = 0
    for city in demand_rows:
        selected = next(
            (
                warehouse
                for warehouse in warehouse_rows
                if assignment_variables.get((city.city_id, warehouse.warehouse_id)) is not None
                and solver.Value(assignment_variables[(city.city_id, warehouse.warehouse_id)]) == 1
            ),
            None,
        )
        if selected is None:
            rows.append(
                AssignmentRow(
                    demand_city_id=city.city_id,
                    warehouse_id=None,
                    demand_quantity=city.demand_quantity,
                    reason="no_ready_route_or_cost",
                )
            )
            unassigned += city.demand_quantity
            continue
        route = routes[(selected.warehouse_id, city.city_id)]
        cost = costs[(selected.warehouse_id, city.city_id, "last_mile")]
        rows.append(
            AssignmentRow(
                demand_city_id=city.city_id,
                warehouse_id=selected.warehouse_id,
                upstream_center_id=selected.upstream_center_id,
                demand_quantity=city.demand_quantity,
                distance_km=route.distance_km,
                duration_hours=route.duration_hours,
                cost=cost,
            )
        )
    assignment = AssignmentResult(
        objective="min_cost",
        rows=rows,
        total_demand=sum(city.demand_quantity for city in demand_rows),
        unassigned_demand=unassigned,
    )
    value = _assignment_objective_value(assignment)
    return (value, active, assignment), int(solver.NumBranches()), status != cp_model.OPTIMAL


def by_warehouse_cost(assignment: AssignmentResult) -> dict[str, float]:
    result: dict[str, float] = defaultdict(float)
    for row in assignment.rows:
        if row.warehouse_id is not None and row.cost is not None:
            result[row.warehouse_id] += row.cost * float(row.demand_quantity)
    return dict(sorted(result.items()))


def summarize_assignment_cost(
    assignment: AssignmentResult,
    cost_matrix: CostMatrix,
) -> CostSummary:
    costs = _cost_index(cost_matrix)
    last_mile_by_warehouse: dict[str, float] = defaultdict(float)
    linehaul_by_warehouse: dict[str, float] = defaultdict(float)
    missing_routes = list(cost_matrix.missing_routes)
    for row in assignment.rows:
        if row.warehouse_id is None:
            continue
        demand_quantity = float(row.demand_quantity)
        last_mile_rate = costs.get(
            (row.warehouse_id, row.demand_city_id, "last_mile")
        )
        if last_mile_rate is None:
            missing_routes.append(
                (row.warehouse_id, row.demand_city_id, "last_mile")
            )
        else:
            last_mile_by_warehouse[row.warehouse_id] += (
                last_mile_rate * demand_quantity
            )
        if row.upstream_center_id and row.upstream_center_id != row.warehouse_id:
            linehaul_key = (
                row.upstream_center_id,
                row.warehouse_id,
                "linehaul",
            )
            linehaul_rate = costs.get(linehaul_key)
            if linehaul_rate is None:
                missing_routes.append(linehaul_key)
            else:
                linehaul_by_warehouse[row.upstream_center_id] += (
                    linehaul_rate * demand_quantity
                )
    warehouse_ids = set(last_mile_by_warehouse) | set(linehaul_by_warehouse)
    by_warehouse = {
        warehouse_id: last_mile_by_warehouse[warehouse_id]
        + linehaul_by_warehouse[warehouse_id]
        for warehouse_id in sorted(warehouse_ids)
    }
    linehaul = sum(linehaul_by_warehouse.values())
    last_mile = sum(last_mile_by_warehouse.values())
    return CostSummary(
        currency=cost_matrix.currency,
        total=linehaul + last_mile,
        linehaul=linehaul,
        last_mile=last_mile,
        by_warehouse=by_warehouse,
        linehaul_by_warehouse=dict(sorted(linehaul_by_warehouse.items())),
        last_mile_by_warehouse=dict(sorted(last_mile_by_warehouse.items())),
        complete=not missing_routes and assignment.unassigned_demand == 0,
        missing_routes=sorted(set(missing_routes)),
    )
