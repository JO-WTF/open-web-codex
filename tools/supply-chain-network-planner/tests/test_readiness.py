from __future__ import annotations

from _network_fixtures import network_case

from supply_chain_planner.case_models import DataQualityReport, NormalizedNetworkInput


def test_missing_current_coverage_is_optional_at_normalization_time() -> None:
    case = network_case()
    normalized = NormalizedNetworkInput(
        country_code=case.country_code,
        demand=case.demand,
        existing_warehouses=[warehouse for warehouse in case.warehouses if warehouse.is_existing],
        quality=DataQualityReport(
            state="ready",
            ready_entities=["demand", "existing_warehouses"],
        ),
    )
    assert normalized.current_assignments == []
    assert normalized.quality.state == "ready"
