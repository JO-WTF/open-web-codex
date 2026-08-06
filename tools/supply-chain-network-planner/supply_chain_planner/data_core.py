"""Read-only source aggregation for the supply-chain Data Agent."""

from __future__ import annotations

import hashlib
import json
from .models import (
    DeliveryBaseline,
    DemandDistributionRow,
    DemandPoint,
    NetworkInput,
    PlanningDataQuality,
    PlanningDataset,
    PlanningSource,
    PlanningSourceInspection,
    PlanningSourceSummary,
    RouteFact,
    TransportRate,
)


def _canonical_source(source: PlanningSource) -> bytes:
    return json.dumps(
        source.model_dump(mode="json"),
        ensure_ascii=False,
        sort_keys=True,
        separators=(",", ":"),
    ).encode("utf-8")


def source_digest(source: PlanningSource) -> str:
    return hashlib.sha256(_canonical_source(source)).hexdigest()


def inspect_source(source: PlanningSource) -> PlanningSourceInspection:
    summary, _, _, quality = _analyze(source)
    return PlanningSourceInspection(source_summary=summary, data_quality=quality)


def build_planning_dataset(source: PlanningSource) -> PlanningDataset:
    summary, distribution, baseline, quality = _analyze(source)
    city_by_id = {item.city_id: item for item in source.cities}
    current_coverage = {
        item.city_id: item.facility_id
        for item in source.warehouse_city_coverage
        if item.is_current
    }
    demand_points = [
        DemandPoint(
            demand_id=f"city-demand-{item.city_id}",
            city_id=item.city_id,
            location=city_by_id[item.city_id].location,
            demand_units=item.demand_units,
            region=city_by_id[item.city_id].region,
            current_facility_id=current_coverage.get(item.city_id),
        )
        for item in sorted(source.city_demands, key=lambda item: item.city_id)
    ]
    transport_rates = [
        TransportRate(
            rate_id=f"rate-{lane.origin_city_id}-{lane.destination_city_id}",
            origin_city_id=lane.origin_city_id,
            destination_city_id=lane.destination_city_id,
            base_cost_per_unit=lane.base_cost_per_unit,
            distance_cost_per_km_per_unit=lane.distance_cost_per_km_per_unit,
        )
        for lane in source.lanes
    ]
    network_input = NetworkInput(
        planning_period=source.planning_period,
        currency=source.currency,
        service_policy=source.service_policy,
        cities=sorted(source.cities, key=lambda item: item.city_id),
        demand_points=demand_points,
        facilities=sorted(source.facilities, key=lambda item: item.facility_id),
        transport_rates=sorted(transport_rates, key=lambda item: item.rate_id),
    )
    digest = source_digest(source)
    return PlanningDataset(
        dataset_id=f"planning_dataset_{digest[:24]}",
        source_digest=digest,
        source_summary=summary,
        network_input=network_input,
        route_provider=source.route_provider,
        route_method=source.route_method,
        route_entries=sorted(
            [
                RouteFact(
                    origin_city_id=lane.origin_city_id,
                    destination_city_id=lane.destination_city_id,
                    distance_meters=int(lane.distance_km * 1000),
                    travel_seconds=int(lane.travel_time_hours * 3600),
                )
                for lane in source.lanes
            ],
            key=lambda item: (
                item.origin_city_id,
                item.destination_city_id,
            ),
        ),
        demand_distribution=distribution,
        delivery_baseline=baseline,
        data_quality=quality,
        dataClassification=source.data_classification,
        demoTemplate=source.demo_template,
        assumptions=[
            "Demand is provided at city and planning-period grain.",
            "Delivery baseline uses the source service-policy threshold.",
            "Candidate facilities are reviewed source facts, not Data Agent recommendations.",
            "City lanes provide distance, travel time and transport cost together.",
            "Only de-identified fields declared by planning_source.v2 are accepted.",
        ],
    )


def _analyze(
    source: PlanningSource,
) -> tuple[
    PlanningSourceSummary,
    list[DemandDistributionRow],
    DeliveryBaseline,
    PlanningDataQuality,
]:
    city_by_id = {city.city_id: city for city in source.cities}
    distribution = [
        DemandDistributionRow(
            demand_id=f"city-demand-{item.city_id}",
            region=city_by_id[item.city_id].region,
            demand_units=item.demand_units,
            city_demand_row_count=1,
            promotion_units=0,
            promotion_share=0,
        )
        for item in sorted(source.city_demands, key=lambda item: item.city_id)
    ]
    dates = [item.demand_date for item in source.city_demands]
    total_units = sum(item.demand_units for item in source.city_demands)
    summary = PlanningSourceSummary(
        source_id=source.source_id,
        market=source.market,
        label=source.label,
        source_updated_at=source.source_updated_at,
        city_demand_row_count=len(source.city_demands),
        demand_node_count=len(source.city_demands),
        facility_count=len(source.facilities),
        existing_facility_count=sum(facility.is_existing for facility in source.facilities),
        candidate_facility_count=sum(not facility.is_existing for facility in source.facilities),
        lane_count=len(source.lanes),
        date_from=min(dates),
        date_to=max(dates),
        demand_units=total_units,
    )
    baseline = DeliveryBaseline(
        observed_demand_units=0,
        on_time_demand_units=0,
        unobserved_demand_units=total_units,
        on_time_ratio=None,
        threshold_seconds=source.service_policy.max_delivery_seconds,
    )
    errors: list[str] = []
    warnings: list[str] = []
    missing_assignments = sorted(
        item.city_id
        for item in source.city_demands
        if item.city_id not in {
            coverage.city_id
            for coverage in source.warehouse_city_coverage
            if coverage.is_current
        }
    )
    if missing_assignments:
        errors.append(
            "missing current facility assignments for demand nodes: "
            + ", ".join(missing_assignments)
        )
    warnings.append(f"{total_units} demand units have no observed delivery duration")
    quality = PlanningDataQuality(
        valid=not errors,
        errors=errors,
        warnings=warnings,
    )
    return summary, distribution, baseline, quality
