"""Pure network report bundles and legacy Case publication."""

from __future__ import annotations

import hashlib
from decimal import Decimal
from pathlib import Path
from typing import TYPE_CHECKING, Literal
from uuid import UUID

from pydantic import Field

from .delivery_models import (
    DeliveryModel,
    ordered_assignment,
    ordered_comparison,
    ordered_cost,
    ordered_metrics,
    validate_delivery_inputs,
)
from .network_models import DemandCityRecord, NormalizedInputBatch, WarehouseRecord
from .optimization_models import (
    AssignmentComparison,
    AssignmentResult,
    BaselineResult,
    CostSummary,
    PMedianSolution,
    ServiceMetric,
)

if TYPE_CHECKING:
    from .case_repository import CaseRepository
    from .case_types import CaseOperationResult
    from .resource_store import PublishedResource, ResourceStore

ReportSource = Literal["baseline", "scenario", "facility_location"]


class NetworkReportScope(DeliveryModel):
    demand_city_count: int = Field(ge=0)
    warehouse_count: int = Field(ge=0)
    existing_warehouse_count: int = Field(ge=0)
    candidate_warehouse_count: int = Field(ge=0)
    total_demand: Decimal = Field(ge=0)


class NetworkReportEntities(DeliveryModel):
    demand_cities: list[DemandCityRecord]
    warehouses: list[WarehouseRecord]


class NetworkBaselineReport(DeliveryModel):
    label: Literal["actual_current", "optimized_existing_footprint"]
    active_warehouse_ids: list[str]
    assignment: AssignmentResult
    service: list[ServiceMetric]
    cost: CostSummary | None
    notice_code: str | None


class NetworkFacilityReport(DeliveryModel):
    status: Literal["optimal", "feasible"]
    active_warehouse_ids: list[str]
    opened_candidate_ids: list[str]
    closed_existing_ids: list[str]
    assignment: AssignmentResult
    objective_value: float | None
    cost: CostSummary | None
    best_bound: float | None
    service: list[ServiceMetric]
    optimality: Literal["proven", "feasible_only", "not_available"]
    message: str | None


class NetworkPlanningReportBundle(DeliveryModel):
    schema_version: Literal["network_planning_report_bundle.v1"] = (
        "network_planning_report_bundle.v1"
    )
    kind: Literal["network_planning_report"] = "network_planning_report"
    title: str = "Warehouse network planning report"
    country_code: str = Field(pattern=r"^[A-Z]{2}$")
    scope: NetworkReportScope
    entities: NetworkReportEntities
    baseline: NetworkBaselineReport
    facility: NetworkFacilityReport
    comparison: AssignmentComparison
    notices: list[str]


def build_network_planning_report_bundle(
    normalized: NormalizedInputBatch,
    baseline: BaselineResult,
    facility: PMedianSolution,
    comparison: AssignmentComparison,
    *,
    country_code: str,
) -> NetworkPlanningReportBundle:
    """Build a complete JSON report without persistence or solver work."""

    validated = validate_delivery_inputs(normalized, baseline, facility, comparison)
    if facility.assignment is None:  # narrowed by validation; retained for typing.
        raise ValueError("delivery_facility_assignment_required")
    notices = sorted(
        {
            notice
            for notice in (baseline.notice_code, facility.message)
            if notice is not None
        }
    )
    existing_count = sum(
        1 for warehouse in validated.warehouse_by_id.values() if warehouse.is_existing
    )
    return NetworkPlanningReportBundle(
        country_code=country_code,
        scope=NetworkReportScope(
            demand_city_count=len(validated.demand_by_id),
            warehouse_count=len(validated.warehouse_by_id),
            existing_warehouse_count=existing_count,
            candidate_warehouse_count=len(validated.warehouse_by_id) - existing_count,
            total_demand=baseline.assignment.total_demand,
        ),
        entities=NetworkReportEntities(
            demand_cities=[
                validated.demand_by_id[city_id]
                for city_id in sorted(validated.demand_by_id)
            ],
            warehouses=[
                validated.warehouse_by_id[warehouse_id]
                for warehouse_id in sorted(validated.warehouse_by_id)
            ],
        ),
        baseline=NetworkBaselineReport(
            label=baseline.label,
            active_warehouse_ids=sorted(validated.baseline_active_ids),
            assignment=ordered_assignment(baseline.assignment),
            service=ordered_metrics(baseline.service),
            cost=ordered_cost(baseline.cost),
            notice_code=baseline.notice_code,
        ),
        facility=NetworkFacilityReport(
            status=facility.status,
            active_warehouse_ids=sorted(validated.facility_active_ids),
            opened_candidate_ids=sorted(facility.opened_candidate_ids),
            closed_existing_ids=sorted(facility.closed_existing_ids),
            assignment=ordered_assignment(facility.assignment),
            objective_value=facility.objective_value,
            cost=ordered_cost(facility.cost),
            best_bound=facility.best_bound,
            service=ordered_metrics(facility.service),
            optimality=facility.optimality,
            message=facility.message,
        ),
        comparison=ordered_comparison(comparison),
        notices=notices,
    )


class NetworkReportService:
    def __init__(self, repository: CaseRepository, resource_store: ResourceStore):
        self.repository = repository
        self.resource_store = resource_store

    def publish(
        self,
        case_id: UUID,
        workspace_root: Path,
        source: ReportSource,
    ) -> tuple[PublishedResource, dict[str, object], CaseOperationResult]:
        from .case_types import FacetName

        case = self.repository.get_case(case_id, workspace_root)
        if source == "baseline":
            value, source_component_id = self.repository.load_baseline(
                case_id, workspace_root
            )
            payload = {
                "schema_version": "network_planning_report.v1",
                "case_id": str(case_id),
                "country_code": case.country_code,
                "source": source,
                "result_label": value.label,
                "objective": value.assignment.objective,
                "total_demand": str(value.assignment.total_demand),
                "unassigned_demand": str(value.assignment.unassigned_demand),
                "service_metrics": [
                    metric.model_dump(mode="json") for metric in value.service
                ],
                "cost_summary": (
                    value.cost.model_dump(mode="json") if value.cost is not None else None
                ),
                "notices": [value.notice_code] if value.notice_code else [],
            }
        elif source == "scenario":
            value, source_component_id = self.repository.load_scenario(
                case_id, workspace_root
            )
            payload = {
                "schema_version": "network_planning_report.v1",
                "case_id": str(case_id),
                "country_code": case.country_code,
                "source": source,
                "result_label": "scenario",
                "objective": value.assignment.objective,
                "total_demand": str(value.assignment.total_demand),
                "unassigned_demand": str(value.assignment.unassigned_demand),
                "warehouse_changes": value.warehouse_changes,
                "service_metrics": [
                    metric.model_dump(mode="json") for metric in value.service
                ],
                "cost_summary": (
                    value.cost.model_dump(mode="json") if value.cost is not None else None
                ),
                "notices": [],
            }
        else:
            value, source_component_id = self.repository.load_facility_solution(
                case_id, workspace_root
            )
            payload = {
                "schema_version": "network_planning_report.v1",
                "case_id": str(case_id),
                "country_code": case.country_code,
                "source": source,
                "result_label": value.status,
                "objective": (
                    value.assignment.objective if value.assignment is not None else "min_cost"
                ),
                "total_demand": (
                    str(value.assignment.total_demand)
                    if value.assignment is not None
                    else None
                ),
                "unassigned_demand": (
                    str(value.assignment.unassigned_demand)
                    if value.assignment is not None
                    else None
                ),
                "selected_warehouse_ids": value.selected_warehouse_ids,
                "objective_value": value.objective_value,
                "best_bound": value.best_bound,
                "optimality": value.optimality,
                "service_metrics": [
                    metric.model_dump(mode="json") for metric in value.service
                ],
                "notices": [value.message] if value.message else [],
            }

        published = self.resource_store.publish("network_planning_report.v1", payload)
        raw = self.resource_store.read(published.resource_id).encode("utf-8")
        content_sha256 = hashlib.sha256(raw).hexdigest()
        lease = self.repository.begin_operation(
            case_id,
            workspace_root,
            "publish_network_planning_report",
            {
                "source": source,
                "source_component_id": str(source_component_id),
                "content_sha256": content_sha256,
            },
        )
        summary = {
            "source": source,
            "result_label": payload["result_label"],
            "service_metric_count": len(payload["service_metrics"]),
            "has_cost": payload.get("cost_summary") is not None,
        }
        result = self.repository.commit_deliverable(
            lease,
            workspace_root,
            component_kind="network_planning_report.v1",
            facet=FacetName.REPORT,
            artifact_schema="network_planning_report.v1",
            media_type="application/json",
            byte_size=len(raw),
            content_sha256=content_sha256,
            storage_name=published.resource_id,
            summary=summary,
            depends_on=[source_component_id],
        )
        return published, summary, result
