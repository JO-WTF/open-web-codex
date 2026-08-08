from __future__ import annotations

from uuid import uuid4

from supply_chain_planner.case_types import (
    AnalysisKind,
    CaseState,
    CaseStatusSummary,
    FacetName,
    FacetState,
    FacetSummary,
)
from supply_chain_planner.readiness import ReadinessEvaluator


def _status(**states: FacetState) -> CaseStatusSummary:
    return CaseStatusSummary(
        case_id=uuid4(),
        revision=1,
        state=CaseState.ACTIVE,
        facets=[
            FacetSummary(name=facet, state=states.get(facet.value, FacetState.MISSING))
            for facet in FacetName
        ],
    )


def test_missing_current_assignment_selects_optimized_baseline() -> None:
    status = _status(normalized_input=FacetState.READY, route_matrix=FacetState.READY)

    result = ReadinessEvaluator().evaluate(status, [AnalysisKind.ACTUAL_SERVICE])

    assert result.available_actions == ["evaluate_optimized_service_baseline"]


def test_required_facets_are_reported_without_global_failure() -> None:
    result = ReadinessEvaluator().evaluate(_status(), [AnalysisKind.SERVICE_BASELINE])

    assert set(result.available_actions) == {
        "prepare_normalized_input",
        "prepare_route_matrix",
    }
