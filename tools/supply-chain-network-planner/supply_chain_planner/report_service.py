"""Explicit publication of bounded user-facing Case deliverables."""

from __future__ import annotations

import hashlib
from pathlib import Path
from typing import Literal
from uuid import UUID

from .case_repository import CaseRepository
from .case_types import CaseOperationResult, FacetName
from .resource_store import PublishedResource, ResourceStore

ReportSource = Literal["baseline", "scenario", "facility_location"]


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
