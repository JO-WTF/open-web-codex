"""Dependency-driven readiness evaluation for network analyses."""

from __future__ import annotations

from dataclasses import dataclass

from .case_types import AnalysisKind, CaseStatusSummary, FacetName, FacetState


@dataclass(frozen=True)
class AnalysisDefinition:
    required_facets: frozenset[FacetName]
    optional_facets: frozenset[FacetName]
    output_facet: FacetName


ANALYSIS_DEFINITIONS = {
    AnalysisKind.SERVICE_BASELINE: AnalysisDefinition(
        frozenset({FacetName.NORMALIZED_INPUT, FacetName.ROUTE_MATRIX}),
        frozenset(),
        FacetName.BASELINE,
    ),
    AnalysisKind.ACTUAL_SERVICE: AnalysisDefinition(
        frozenset({FacetName.NORMALIZED_INPUT, FacetName.ROUTE_MATRIX}),
        frozenset({FacetName.CURRENT_ASSIGNMENT}),
        FacetName.BASELINE,
    ),
    AnalysisKind.COST_BASELINE: AnalysisDefinition(
        frozenset({FacetName.NORMALIZED_INPUT, FacetName.COST_MATRIX}),
        frozenset(),
        FacetName.BASELINE,
    ),
    AnalysisKind.ACTUAL_COST: AnalysisDefinition(
        frozenset({FacetName.NORMALIZED_INPUT, FacetName.COST_MATRIX}),
        frozenset({FacetName.CURRENT_ASSIGNMENT}),
        FacetName.BASELINE,
    ),
    AnalysisKind.SCENARIO: AnalysisDefinition(
        frozenset({FacetName.NORMALIZED_INPUT, FacetName.ROUTE_MATRIX}),
        frozenset({FacetName.COST_MATRIX}),
        FacetName.SCENARIO,
    ),
    AnalysisKind.P_MEDIAN: AnalysisDefinition(
        frozenset({FacetName.NORMALIZED_INPUT, FacetName.COST_MATRIX}),
        frozenset(),
        FacetName.FACILITY_LOCATION,
    ),
    AnalysisKind.SERVICE_CONSTRAINED: AnalysisDefinition(
        frozenset(
            {FacetName.NORMALIZED_INPUT, FacetName.ROUTE_MATRIX, FacetName.COST_MATRIX}
        ),
        frozenset(),
        FacetName.FACILITY_LOCATION,
    ),
    AnalysisKind.REPORT: AnalysisDefinition(
        frozenset({FacetName.BASELINE}), frozenset(), FacetName.REPORT
    ),
    AnalysisKind.MAP: AnalysisDefinition(
        frozenset({FacetName.BASELINE, FacetName.SCENARIO}), frozenset(), FacetName.MAP
    ),
}


class ReadinessEvaluator:
    def evaluate(
        self, status: CaseStatusSummary, requested: list[AnalysisKind]
    ) -> CaseStatusSummary:
        by_name = {facet.name: facet for facet in status.facets}
        actions: list[str] = []
        for analysis in requested:
            definition = ANALYSIS_DEFINITIONS[analysis]
            missing = [
                facet
                for facet in definition.required_facets
                if by_name[facet].state != FacetState.READY
            ]
            if missing:
                actions.extend(f"prepare_{facet.value}" for facet in sorted(missing))
                continue
            if analysis == AnalysisKind.ACTUAL_SERVICE and (
                by_name[FacetName.CURRENT_ASSIGNMENT].state != FacetState.READY
            ):
                actions.append("evaluate_optimized_service_baseline")
            elif analysis == AnalysisKind.ACTUAL_COST and (
                by_name[FacetName.CURRENT_ASSIGNMENT].state != FacetState.READY
            ):
                actions.append("evaluate_optimized_cost_baseline")
            else:
                actions.append(f"evaluate_{analysis.value}")
        return status.model_copy(update={"available_actions": list(dict.fromkeys(actions))})
