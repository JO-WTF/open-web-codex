"""Deterministic Case comparison map publication."""

from __future__ import annotations

import hashlib
from pathlib import Path
from typing import Literal
from uuid import UUID

from .case_repository import CaseRepository
from .case_types import CaseOperationResult, FacetName
from .resource_store import PublishedResource, ResourceStore

CandidateSource = Literal["scenario", "facility_location"]


class NetworkMapService:
    def __init__(self, repository: CaseRepository, resource_store: ResourceStore):
        self.repository = repository
        self.resource_store = resource_store

    def publish_comparison(
        self,
        case_id: UUID,
        workspace_root: Path,
        candidate_source: CandidateSource,
    ) -> tuple[PublishedResource, dict[str, object], CaseOperationResult]:
        normalized, normalized_id = self.repository.load_normalized_input(
            case_id, workspace_root
        )
        baseline, baseline_id = self.repository.load_baseline(case_id, workspace_root)
        if candidate_source == "scenario":
            candidate, candidate_id = self.repository.load_scenario(
                case_id, workspace_root
            )
            candidate_assignment = candidate.assignment
            candidate_label = "scenario"
            candidate_service = candidate.service
        else:
            candidate, candidate_id = self.repository.load_facility_solution(
                case_id, workspace_root
            )
            if candidate.assignment is None:
                raise ValueError("facility_solution_has_no_assignment")
            candidate_assignment = candidate.assignment
            candidate_label = candidate.status
            candidate_service = candidate.service

        warehouses = {item.warehouse_id: item for item in normalized.warehouses}
        demand = {item.city_id: item for item in normalized.demand_cities}
        baseline_active = {
            row.warehouse_id
            for row in baseline.assignment.rows
            if row.warehouse_id is not None
        }
        candidate_active = {
            row.warehouse_id
            for row in candidate_assignment.rows
            if row.warehouse_id is not None
        }
        features: list[dict[str, object]] = []
        for warehouse in sorted(warehouses.values(), key=lambda item: item.warehouse_id):
            if warehouse.longitude is None or warehouse.latitude is None:
                continue
            features.append(
                {
                    "type": "Feature",
                    "geometry": {
                        "type": "Point",
                        "coordinates": [warehouse.longitude, warehouse.latitude],
                    },
                    "properties": {
                        "kind": "warehouse",
                        "warehouse_id": warehouse.warehouse_id,
                        "warehouse_name": warehouse.warehouse_name,
                        "warehouse_type": warehouse.warehouse_type,
                        "is_existing": warehouse.is_existing,
                        "baseline_active": warehouse.warehouse_id in baseline_active,
                        "candidate_active": warehouse.warehouse_id in candidate_active,
                    },
                }
            )
        for city in sorted(demand.values(), key=lambda item: item.city_id):
            if city.longitude is None or city.latitude is None:
                continue
            features.append(
                {
                    "type": "Feature",
                    "geometry": {
                        "type": "Point",
                        "coordinates": [city.longitude, city.latitude],
                    },
                    "properties": {
                        "kind": "demand",
                        "city_id": city.city_id,
                        "city_name": city.city_name,
                        "demand_quantity": str(city.demand_quantity),
                    },
                }
            )
        for scenario, result_label, assignment in (
            ("baseline", baseline.label, baseline.assignment),
            ("candidate", candidate_label, candidate_assignment),
        ):
            for row in assignment.rows:
                warehouse = warehouses.get(row.warehouse_id or "")
                city = demand.get(row.demand_city_id)
                if (
                    warehouse is None
                    or city is None
                    or warehouse.longitude is None
                    or warehouse.latitude is None
                    or city.longitude is None
                    or city.latitude is None
                ):
                    continue
                features.append(
                    {
                        "type": "Feature",
                        "geometry": {
                            "type": "LineString",
                            "coordinates": [
                                [warehouse.longitude, warehouse.latitude],
                                [city.longitude, city.latitude],
                            ],
                        },
                        "properties": {
                            "kind": "assignment",
                            "scenario": scenario,
                            "result_label": result_label,
                            "warehouse_id": warehouse.warehouse_id,
                            "demand_city_id": city.city_id,
                        },
                    }
                )
        metadata = {
            "case_id": str(case_id),
            "baseline_label": baseline.label,
            "candidate_source": candidate_source,
            "candidate_label": candidate_label,
            "baseline_service": [
                item.model_dump(mode="json") for item in baseline.service
            ],
            "candidate_service": [
                item.model_dump(mode="json") for item in candidate_service
            ],
        }
        geojson = {
            "type": "FeatureCollection",
            "schema_version": "network_comparison_map.v1",
            "metadata": metadata,
            "features": features,
        }
        published = self.resource_store.publish("network_comparison_map.v1", geojson)
        raw = self.resource_store.read(published.resource_id).encode("utf-8")
        content_sha256 = hashlib.sha256(raw).hexdigest()
        lease = self.repository.begin_operation(
            case_id,
            workspace_root,
            "publish_network_comparison_map",
            {
                "normalized_component_id": str(normalized_id),
                "baseline_component_id": str(baseline_id),
                "candidate_component_id": str(candidate_id),
                "candidate_source": candidate_source,
                "content_sha256": content_sha256,
            },
        )
        summary = {
            "candidate_source": candidate_source,
            "feature_count": len(features),
            "baseline_active_warehouse_count": len(baseline_active),
            "candidate_active_warehouse_count": len(candidate_active),
        }
        result = self.repository.commit_deliverable(
            lease,
            workspace_root,
            component_kind="network_comparison_map.v1",
            facet=FacetName.MAP,
            artifact_schema="network_comparison_map.v1",
            media_type="application/geo+json",
            byte_size=len(raw),
            content_sha256=content_sha256,
            storage_name=published.resource_id,
            summary=summary,
            depends_on=[normalized_id, baseline_id, candidate_id],
        )
        return published, summary, result
