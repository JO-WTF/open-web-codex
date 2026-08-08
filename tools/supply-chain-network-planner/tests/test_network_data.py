from __future__ import annotations

from pathlib import Path

from supply_chain_planner.case_repository import CaseRepository
from supply_chain_planner.network_data import SourceInventoryService


def test_refresh_and_inspect_workspace_source(tmp_path: Path) -> None:
    workspace = tmp_path / "workspace"
    workspace.mkdir()
    (workspace / "demand.csv").write_text(
        "city_id,city_name,demand_quantity\nID-1,Jakarta,10\n", encoding="utf-8"
    )
    repository = CaseRepository(tmp_path / "cases.sqlite3")
    case = repository.create_case(workspace, "ID", "Analyze the network")
    service = SourceInventoryService(repository)

    sources, changed = service.refresh(case.case_id, workspace)
    inspections = service.inspect(case.case_id, workspace, [sources[0].source_id])

    assert changed is True
    assert sources[0].display_name == "demand.csv"
    assert [field.field_name for field in inspections[0].fields] == [
        "city_id",
        "city_name",
        "demand_quantity",
    ]
