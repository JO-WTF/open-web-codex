from __future__ import annotations

from pathlib import Path

from supply_chain_planner.case_repository import CaseRepository
from supply_chain_planner.case_types import FacetName, FacetState
from supply_chain_planner.mapping import SourceRole
from supply_chain_planner.mapping_service import CaseMappingService
from supply_chain_planner.network_data import SourceInventoryService
from supply_chain_planner.normalization import NormalizationService


def test_quote_rows_never_create_current_assignments(tmp_path: Path) -> None:
    workspace = tmp_path / "workspace"
    workspace.mkdir()
    (workspace / "demand.csv").write_text(
        "city_id,city_name,demand_quantity,longitude,latitude\n"
        "ID-1,Jakarta,10,106.8,-6.2\n",
        encoding="utf-8",
    )
    (workspace / "warehouses.csv").write_text(
        "warehouse_id,warehouse_name,warehouse_type,city_id,city_name,is_existing,"
        "longitude,latitude\nWH-1,Center,center,ID-1,Jakarta,true,106.8,-6.2\n",
        encoding="utf-8",
    )
    (workspace / "quotes.csv").write_text(
        "origin_id,destination_id,layer,price_per_vehicle,currency,vehicle_capacity\n"
        "WH-1,ID-1,last_mile,10000,IDR,1\n",
        encoding="utf-8",
    )
    repository = CaseRepository(tmp_path / "cases.sqlite3")
    case = repository.create_case(workspace, "ID", "Analyze")
    SourceInventoryService(repository).refresh(case.case_id, workspace)
    proposal, _ = CaseMappingService(repository).propose(case.case_id, workspace)
    selected = [
        candidate.candidate_id
        for role in proposal.proposals
        if role.role
        in {SourceRole.DEMAND, SourceRole.EXISTING_WAREHOUSE, SourceRole.ROUTE_QUOTE}
        for candidate in role.field_candidates
    ]
    CaseMappingService(repository).apply(case.case_id, workspace, selected)

    batch, result = NormalizationService(repository).normalize(case.case_id, workspace)

    assert result is not None
    assert len(batch.route_quotes) == 1
    assert batch.current_assignments == []
    status = repository.get_status(case.case_id, workspace)
    assignment = next(item for item in status.facets if item.name == FacetName.CURRENT_ASSIGNMENT)
    assert assignment.state == FacetState.MISSING
