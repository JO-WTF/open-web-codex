"""Shared pure validation and serialization for network delivery bundles."""

from __future__ import annotations

import math
from collections.abc import Callable, Iterable, Mapping
from dataclasses import dataclass
from decimal import Decimal
from typing import TypeVar

from pydantic import BaseModel, ConfigDict

from .network_models import DemandCityRecord, NormalizedInputBatch, WarehouseRecord
from .optimization_models import (
    AssignmentComparison,
    AssignmentResult,
    AssignmentRow,
    BaselineResult,
    CostSummary,
    PMedianSolution,
    ServiceMetric,
)

DeliveryValue = TypeVar("DeliveryValue")


class DeliveryModel(BaseModel):
    model_config = ConfigDict(extra="forbid")


@dataclass(frozen=True)
class ValidatedDeliveryInputs:
    demand_by_id: Mapping[str, DemandCityRecord]
    warehouse_by_id: Mapping[str, WarehouseRecord]
    baseline_rows_by_city: Mapping[str, AssignmentRow]
    facility_rows_by_city: Mapping[str, AssignmentRow]
    baseline_active_ids: frozenset[str]
    facility_active_ids: frozenset[str]


def validate_delivery_inputs(
    normalized: NormalizedInputBatch,
    baseline: BaselineResult,
    facility: PMedianSolution,
    comparison: AssignmentComparison,
) -> ValidatedDeliveryInputs:
    """Cross-check exact typed results without filling gaps or recomputing them."""

    if any(issue.severity == "error" for issue in normalized.issues):
        raise ValueError("delivery_normalized_input_has_errors")
    demand = _index(
        normalized.demand_cities,
        lambda item: item.city_id,
        "delivery_duplicate_demand_city_id",
    )
    warehouses = _index(
        normalized.warehouses,
        lambda item: item.warehouse_id,
        "delivery_duplicate_warehouse_id",
    )
    if not demand or not warehouses:
        raise ValueError("delivery_demand_and_warehouse_required")
    before_active = _active_ids("baseline", baseline.active_warehouse_ids, warehouses)
    after_active = _active_ids("facility", facility.active_warehouse_ids, warehouses)
    existing = {key for key, item in warehouses.items() if item.is_existing}
    if before_active != existing:
        raise ValueError("delivery_baseline_active_must_equal_existing")
    if facility.assignment is None:
        raise ValueError("delivery_facility_assignment_required")
    if facility.status not in {"optimal", "feasible"}:
        raise ValueError(f"delivery_facility_status_not_deliverable:{facility.status}")

    before_rows = _assignment_rows(
        "baseline", baseline.assignment, demand, warehouses, before_active
    )
    after_rows = _assignment_rows(
        "facility", facility.assignment, demand, warehouses, after_active
    )
    candidates = set(warehouses) - existing
    _exact_ids(
        "facility_opened_candidate",
        facility.opened_candidate_ids,
        after_active & candidates,
    )
    _exact_ids(
        "facility_closed_existing",
        facility.closed_existing_ids,
        existing - after_active,
    )
    _exact_ids(
        "comparison_selected_warehouse",
        comparison.selected_warehouse_ids,
        after_active - before_active,
    )
    _exact_ids(
        "comparison_removed_warehouse",
        comparison.removed_warehouse_ids,
        before_active - after_active,
    )
    _comparison(comparison, demand, before_rows, after_rows)
    _service(baseline, facility, comparison)
    _cost(baseline, facility, comparison)
    return ValidatedDeliveryInputs(
        demand_by_id=demand,
        warehouse_by_id=warehouses,
        baseline_rows_by_city=before_rows,
        facility_rows_by_city=after_rows,
        baseline_active_ids=frozenset(before_active),
        facility_active_ids=frozenset(after_active),
    )


def ordered_assignment(value: AssignmentResult) -> AssignmentResult:
    return value.model_copy(
        update={"rows": sorted(value.rows, key=lambda row: row.demand_city_id)}
    )


def ordered_cost(value: CostSummary | None) -> CostSummary | None:
    if value is None:
        return None
    return value.model_copy(
        update={
            "by_warehouse": dict(sorted(value.by_warehouse.items())),
            "linehaul_by_warehouse": dict(sorted(value.linehaul_by_warehouse.items())),
            "last_mile_by_warehouse": dict(sorted(value.last_mile_by_warehouse.items())),
            "missing_routes": sorted(value.missing_routes),
        }
    )


def ordered_metrics(values: list[ServiceMetric]) -> list[ServiceMetric]:
    return sorted(values, key=lambda item: item.target_hours)


def ordered_comparison(value: AssignmentComparison) -> AssignmentComparison:
    return value.model_copy(
        update={
            "requested_service_targets": sorted(value.requested_service_targets),
            "service": sorted(value.service, key=lambda item: item.target_hours),
            "selected_warehouse_ids": sorted(value.selected_warehouse_ids),
            "removed_warehouse_ids": sorted(value.removed_warehouse_ids),
            "affected_city_ids": sorted(value.affected_city_ids),
            "reassigned_city_ids": sorted(value.reassigned_city_ids),
            "city_changes": sorted(
                value.city_changes, key=lambda item: item.demand_city_id
            ),
        }
    )


def _index(
    values: Iterable[DeliveryValue],
    key: Callable[[DeliveryValue], str],
    error_code: str,
) -> dict[str, DeliveryValue]:
    result: dict[str, DeliveryValue] = {}
    for value in values:
        value_id = key(value)
        if value_id in result:
            raise ValueError(f"{error_code}:{value_id}")
        result[value_id] = value
    return result


def _active_ids(
    source: str,
    values: list[str],
    warehouses: Mapping[str, WarehouseRecord],
) -> set[str]:
    if len(values) != len(set(values)):
        raise ValueError(f"delivery_{source}_active_ids_duplicate")
    unknown = sorted(set(values) - set(warehouses))
    if unknown:
        raise ValueError(f"delivery_{source}_active_warehouse_unknown:{','.join(unknown)}")
    return set(values)


def _assignment_rows(
    source: str,
    assignment: AssignmentResult,
    demand: Mapping[str, DemandCityRecord],
    warehouses: Mapping[str, WarehouseRecord],
    active: set[str],
) -> dict[str, AssignmentRow]:
    rows = _index(
        assignment.rows,
        lambda item: item.demand_city_id,
        f"delivery_{source}_assignment_city_duplicate",
    )
    if set(rows) != set(demand):
        raise ValueError(f"delivery_{source}_assignment_city_set_mismatch")
    expected_total = sum(
        (item.demand_quantity for item in demand.values()), start=Decimal(0)
    )
    if assignment.total_demand != expected_total:
        raise ValueError(f"delivery_{source}_total_demand_mismatch")
    unassigned = Decimal(0)
    for city_id, row in rows.items():
        if row.demand_quantity != demand[city_id].demand_quantity:
            raise ValueError(f"delivery_{source}_assignment_demand_mismatch:{city_id}")
        if row.warehouse_id is None:
            unassigned += row.demand_quantity
            if row.upstream_center_id is not None:
                raise ValueError(f"delivery_{source}_unassigned_upstream:{city_id}")
            continue
        warehouse = warehouses.get(row.warehouse_id)
        if warehouse is None:
            raise ValueError(
                f"delivery_{source}_assignment_warehouse_unknown:{row.warehouse_id}"
            )
        if warehouse.warehouse_id not in active:
            raise ValueError(
                f"delivery_{source}_assignment_warehouse_inactive:{row.warehouse_id}"
            )
        if row.upstream_center_id != warehouse.upstream_center_id:
            raise ValueError(f"delivery_{source}_assignment_upstream_mismatch:{city_id}")
    if assignment.unassigned_demand != unassigned:
        raise ValueError(f"delivery_{source}_unassigned_demand_mismatch")
    _validate_crossdocks(source, active, warehouses)
    return rows


def _validate_crossdocks(
    source: str,
    active: set[str],
    warehouses: Mapping[str, WarehouseRecord],
) -> None:
    for warehouse_id in sorted(active):
        warehouse = warehouses[warehouse_id]
        if warehouse.warehouse_type != "cross_docking":
            continue
        upstream = warehouses.get(warehouse.upstream_center_id or "")
        if upstream is None:
            raise ValueError(f"delivery_{source}_crossdock_upstream_unknown:{warehouse_id}")
        if upstream.warehouse_type != "center":
            raise ValueError(f"delivery_{source}_crossdock_upstream_not_center:{warehouse_id}")
        if upstream.warehouse_id not in active:
            raise ValueError(f"delivery_{source}_crossdock_upstream_inactive:{warehouse_id}")


def _exact_ids(field: str, actual: list[str], expected: set[str]) -> None:
    if len(actual) != len(set(actual)):
        raise ValueError(f"delivery_{field}_ids_duplicate")
    if set(actual) != expected:
        raise ValueError(f"delivery_{field}_ids_mismatch")


def _comparison(
    value: AssignmentComparison,
    demand: Mapping[str, DemandCityRecord],
    before: Mapping[str, AssignmentRow],
    after: Mapping[str, AssignmentRow],
) -> None:
    changes = _index(
        value.city_changes,
        lambda item: item.demand_city_id,
        "delivery_comparison_city_duplicate",
    )
    if set(changes) != set(demand):
        raise ValueError("delivery_comparison_city_set_mismatch")
    affected: set[str] = set()
    reassigned: set[str] = set()
    for city_id, change in changes.items():
        before_row, after_row = before[city_id], after[city_id]
        if (
            change.before_warehouse_id != before_row.warehouse_id
            or change.after_warehouse_id != after_row.warehouse_id
        ):
            raise ValueError(f"delivery_comparison_assignment_mismatch:{city_id}")
        if change.affected != (before_row != after_row):
            raise ValueError(f"delivery_comparison_affected_mismatch:{city_id}")
        if change.reassigned != (before_row.warehouse_id != after_row.warehouse_id):
            raise ValueError(f"delivery_comparison_reassigned_mismatch:{city_id}")
        if change.affected:
            affected.add(city_id)
        if change.reassigned:
            reassigned.add(city_id)
    _exact_ids("comparison_affected_city", value.affected_city_ids, affected)
    _exact_ids("comparison_reassigned_city", value.reassigned_city_ids, reassigned)


def _service(
    baseline: BaselineResult,
    facility: PMedianSolution,
    comparison: AssignmentComparison,
) -> None:
    if not baseline.service:
        raise ValueError("delivery_baseline_service_required")
    if not facility.service:
        raise ValueError("delivery_facility_service_required")
    requested = set(comparison.requested_service_targets)
    compared = {item.target_hours: item for item in comparison.service}
    before = {item.target_hours: item for item in baseline.service}
    after = {item.target_hours: item for item in facility.service}
    if not requested or set(compared) != requested or set(before) != requested:
        raise ValueError("delivery_baseline_service_targets_mismatch")
    if set(after) != requested:
        raise ValueError("delivery_facility_service_targets_mismatch")
    for target in requested:
        item = compared[target]
        if not _close(item.before_coverage_rate, before[target].coverage_rate):
            raise ValueError("delivery_baseline_comparison_service_mismatch")
        if not _close(item.after_coverage_rate, after[target].coverage_rate):
            raise ValueError("delivery_facility_comparison_service_mismatch")
        if not _close(
            item.coverage_rate_delta,
            item.after_coverage_rate - item.before_coverage_rate,
        ):
            raise ValueError("delivery_comparison_service_delta_mismatch")


def _cost(
    baseline: BaselineResult,
    facility: PMedianSolution,
    comparison: AssignmentComparison,
) -> None:
    if baseline.assignment.objective != facility.assignment.objective:
        raise ValueError("delivery_assignment_objective_mismatch")
    if baseline.assignment.objective == "min_cost" and baseline.cost is None:
        raise ValueError("delivery_baseline_cost_required")
    if facility.assignment.objective == "min_cost" and facility.cost is None:
        raise ValueError("delivery_facility_cost_required")
    if baseline.cost is None or facility.cost is None:
        if comparison.before_cost is not None or comparison.after_cost is not None:
            raise ValueError("delivery_comparison_cost_without_summaries")
        return
    if baseline.cost.currency != facility.cost.currency:
        raise ValueError("delivery_cost_currency_mismatch")
    for source, cost in (("baseline", baseline.cost), ("facility", facility.cost)):
        if not cost.complete or cost.missing_routes:
            raise ValueError(f"delivery_{source}_cost_incomplete")
        if not _close(cost.total, cost.linehaul + cost.last_mile):
            raise ValueError(f"delivery_{source}_cost_components_mismatch")
    if comparison.before_cost is None or comparison.after_cost is None:
        raise ValueError("delivery_comparison_cost_required")
    if not _close(comparison.before_cost, baseline.cost.total):
        raise ValueError("delivery_comparison_before_cost_mismatch")
    if not _close(comparison.after_cost, facility.cost.total):
        raise ValueError("delivery_comparison_after_cost_mismatch")
    expected_delta = facility.cost.total - baseline.cost.total
    if comparison.cost_delta is None or not _close(comparison.cost_delta, expected_delta):
        raise ValueError("delivery_comparison_cost_delta_mismatch")
    if facility.objective_value is None or not _close(
        facility.objective_value, facility.cost.total
    ):
        raise ValueError("delivery_facility_objective_value_mismatch")


def _close(left: float, right: float) -> bool:
    return math.isclose(left, right, rel_tol=1e-9, abs_tol=1e-6)
