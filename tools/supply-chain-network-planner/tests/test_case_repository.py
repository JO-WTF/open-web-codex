from __future__ import annotations

import hashlib
from pathlib import Path

import pytest

from supply_chain_planner.case_repository import CaseRepository, CaseRepositoryError
from supply_chain_planner.case_types import (
    ComponentWrite,
    FacetName,
    FacetState,
    SourceSnapshot,
)


def test_case_is_bound_to_workspace(tmp_path: Path) -> None:
    repository = CaseRepository(tmp_path / "state" / "cases.sqlite3")
    first = tmp_path / "first"
    second = tmp_path / "second"
    first.mkdir()
    second.mkdir()
    case = repository.create_case(first, "ID", "Analyze the network")

    assert repository.get_case(case.case_id, first).revision == 1
    with pytest.raises(CaseRepositoryError, match="does not belong"):
        repository.get_case(case.case_id, second)


def test_completed_operation_is_idempotent(tmp_path: Path) -> None:
    repository = CaseRepository(tmp_path / "cases.sqlite3")
    workspace = tmp_path / "workspace"
    workspace.mkdir()
    case = repository.create_case(workspace, "ID", "Analyze the network")
    lease = repository.begin_operation(case.case_id, workspace, "define_requirements", {"x": 1})
    digest = hashlib.sha256(b"requirements").hexdigest()
    result = repository.complete_operation(
        lease,
        workspace,
        [
            ComponentWrite(
                component_kind="requirements.v1",
                content_sha256=digest,
                row_count=1,
                facet=FacetName.REQUIREMENTS,
            )
        ],
    )

    repeated = repository.begin_operation(
        case.case_id, workspace, "define_requirements", {"x": 1}
    )
    reused = repository.complete_operation(repeated, workspace, [])

    assert result.components[0].component_id == reused.components[0].component_id
    assert reused.operation.reused is True
    status = repository.get_status(case.case_id, workspace)
    requirements = next(item for item in status.facets if item.name == FacetName.REQUIREMENTS)
    assert requirements.state == FacetState.READY


def test_replacing_component_invalidates_bound_dependents(tmp_path: Path) -> None:
    repository = CaseRepository(tmp_path / "cases.sqlite3")
    workspace = tmp_path / "workspace"
    workspace.mkdir()
    case = repository.create_case(workspace, "ID", "Analyze the network")
    requirements = repository.complete_operation(
        repository.begin_operation(case.case_id, workspace, "requirements-one", {"v": 1}),
        workspace,
        [
            ComponentWrite(
                component_kind="requirements.v1",
                content_sha256=hashlib.sha256(b"requirements-one").hexdigest(),
                row_count=1,
                facet=FacetName.REQUIREMENTS,
            )
        ],
    ).components[0]
    repository.complete_operation(
        repository.begin_operation(case.case_id, workspace, "report-one", {"v": 1}),
        workspace,
        [
            ComponentWrite(
                component_kind="report.v1",
                content_sha256=hashlib.sha256(b"report-one").hexdigest(),
                row_count=1,
                facet=FacetName.REPORT,
                depends_on=(requirements.component_id,),
            )
        ],
    )

    repository.complete_operation(
        repository.begin_operation(case.case_id, workspace, "requirements-two", {"v": 2}),
        workspace,
        [
            ComponentWrite(
                component_kind="requirements.v1",
                content_sha256=hashlib.sha256(b"requirements-two").hexdigest(),
                row_count=1,
                facet=FacetName.REQUIREMENTS,
            )
        ],
    )

    status = repository.get_status(case.case_id, workspace)
    report = next(item for item in status.facets if item.name == FacetName.REPORT)
    assert report.state == FacetState.STALE


def test_archiving_rejects_running_operation(tmp_path: Path) -> None:
    repository = CaseRepository(tmp_path / "cases.sqlite3")
    workspace = tmp_path / "workspace"
    workspace.mkdir()
    case = repository.create_case(workspace, "ID", "Analyze the network")
    repository.begin_operation(case.case_id, workspace, "refresh_sources", {"revision": 1})

    with pytest.raises(CaseRepositoryError, match="running operations"):
        repository.archive_case(case.case_id, workspace)


def test_unchanged_source_inventory_is_reused(tmp_path: Path) -> None:
    repository = CaseRepository(tmp_path / "cases.sqlite3")
    workspace = tmp_path / "workspace"
    workspace.mkdir()
    case = repository.create_case(workspace, "ID", "Analyze the network")
    source = SourceSnapshot(
        source_ref="source-one",
        display_name="demand.csv",
        media_type="text/csv",
        content_sha256=hashlib.sha256(b"same").hexdigest(),
        byte_size=4,
    )

    first, first_changed = repository.refresh_sources(case.case_id, workspace, [source])
    second, second_changed = repository.refresh_sources(case.case_id, workspace, [source])

    assert first_changed is True
    assert second_changed is False
    assert first[0].source_id == second[0].source_id
