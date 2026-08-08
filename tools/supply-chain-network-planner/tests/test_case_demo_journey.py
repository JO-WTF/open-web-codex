from __future__ import annotations

from pathlib import Path

from supply_chain_planner.analysis_service import NetworkAnalysisService
from supply_chain_planner.case_repository import CaseRepository
from supply_chain_planner.demo_server import TEMPLATE_REF, create_sources
from supply_chain_planner.mapping import SourceRole
from supply_chain_planner.mapping_service import CaseMappingService
from supply_chain_planner.matrix_service import CaseMatrixService
from supply_chain_planner.network_data import SourceInventoryService
from supply_chain_planner.normalization import NormalizationService


def test_demo_sources_run_from_case_creation_to_service_and_cost_baseline(
    tmp_path: Path,
) -> None:
    workspace = tmp_path / "workspace"
    workspace.mkdir()
    repository = CaseRepository(tmp_path / "cases.sqlite3")
    case = repository.create_case(workspace, "ID", "分析现有网络")
    create_sources(workspace, TEMPLATE_REF, 42)
    SourceInventoryService(repository).refresh(case.case_id, workspace)
    proposal, _ = CaseMappingService(repository).propose(case.case_id, workspace)
    selected = [
        candidate.candidate_id
        for source in proposal.proposals
        if source.complete
        and not source.ambiguous
        and source.role
        in {
            SourceRole.DEMAND,
            SourceRole.EXISTING_WAREHOUSE,
            SourceRole.CANDIDATE_WAREHOUSE,
            SourceRole.ROUTE_QUOTE,
        }
        for candidate in source.field_candidates
    ]
    CaseMappingService(repository).apply(case.case_id, workspace, selected)
    normalized, result = NormalizationService(repository).normalize(
        case.case_id, workspace
    )
    assert result is not None
    assert len(normalized.demand_cities) == 50
    assert len([item for item in normalized.warehouses if item.is_existing]) == 11
    assert len(normalized.route_quotes) == 580

    matrices = CaseMatrixService(repository)
    routes, _ = matrices.build_haversine(case.case_id, workspace, 1.2, 40)
    costs, cost_result = matrices.build_costs(
        case.case_id, workspace, None, "existing_only"
    )
    baseline, _ = NetworkAnalysisService(repository).evaluate_baseline(
        case.case_id,
        workspace,
        "min_time",
        [6, 12, 18],
        "actual_if_available",
        True,
    )

    assert len(routes.rows) == 1_150
    assert cost_result is not None
    assert len(costs.rows) == 580
    assert costs.missing_routes == []
    assert baseline.label == "optimized_existing_footprint"
    assert baseline.notice_code == "current_assignment_missing"
    assert baseline.cost is not None and baseline.cost.complete is True
    assert [metric.coverage_rate for metric in baseline.service] == sorted(
        metric.coverage_rate for metric in baseline.service
    )
