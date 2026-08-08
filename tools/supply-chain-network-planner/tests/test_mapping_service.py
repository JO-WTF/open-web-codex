from __future__ import annotations

from pathlib import Path

from supply_chain_planner.case_repository import CaseRepository
from supply_chain_planner.case_types import FacetName, FacetState
from supply_chain_planner.mapping import SourceRole
from supply_chain_planner.mapping_service import CaseMappingService
from supply_chain_planner.network_data import SourceInventoryService


def test_exact_workspace_mapping_is_persisted_and_applied(tmp_path: Path) -> None:
    workspace = tmp_path / "workspace"
    workspace.mkdir()
    (workspace / "demand.csv").write_text(
        "city_id,city_name,demand_quantity\nID-1,Jakarta,10\n", encoding="utf-8"
    )
    (workspace / "warehouses.csv").write_text(
        "warehouse_id,warehouse_name,warehouse_type,city_id,city_name,is_existing\n"
        "WH-1,Jakarta Center,center,ID-1,Jakarta,true\n",
        encoding="utf-8",
    )
    repository = CaseRepository(tmp_path / "cases.sqlite3")
    case = repository.create_case(workspace, "ID", "Analyze the network")
    SourceInventoryService(repository).refresh(case.case_id, workspace)

    proposal, _ = CaseMappingService(repository).propose(case.case_id, workspace)
    selected = [
        candidate.candidate_id
        for role in proposal.proposals
        if role.role in {SourceRole.DEMAND, SourceRole.EXISTING_WAREHOUSE}
        for candidate in role.field_candidates
    ]
    CaseMappingService(repository).apply(case.case_id, workspace, selected)

    status = repository.get_status(case.case_id, workspace)
    mapping = next(item for item in status.facets if item.name == FacetName.MAPPING)
    assert mapping.state == FacetState.READY
