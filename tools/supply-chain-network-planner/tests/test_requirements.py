from __future__ import annotations

from pathlib import Path

from supply_chain_planner.case_repository import CaseRepository
from supply_chain_planner.case_types import AnalysisKind, FacetName, FacetState
from supply_chain_planner.requirements import RequirementRequest, RequirementService


def test_cost_analysis_requires_quote_or_cost_rule(tmp_path: Path) -> None:
    workspace = tmp_path / "workspace"
    workspace.mkdir()
    repository = CaseRepository(tmp_path / "cases.sqlite3")
    case = repository.create_case(workspace, "ID", "Analyze cost")

    profile, _ = RequirementService(repository).define(
        case.case_id,
        workspace,
        RequirementRequest(requested_analyses=[AnalysisKind.COST_BASELINE]),
    )

    assert "cost_rule_or_route_quote" in profile.required_entities
    status = repository.get_status(case.case_id, workspace)
    requirement = next(item for item in status.facets if item.name == FacetName.REQUIREMENTS)
    assert requirement.state == FacetState.READY
