"""SQLite-backed source of truth for network-planning case state."""

from __future__ import annotations

import hashlib
import json
import os
import sqlite3
from collections.abc import Iterable, Sequence
from contextlib import contextmanager
from datetime import UTC, datetime, timedelta
from decimal import Decimal
from pathlib import Path
from typing import TYPE_CHECKING, TypeVar
from uuid import UUID, uuid4

from pydantic import BaseModel

if TYPE_CHECKING:
    from .matrix_models import CostMatrix, RouteMatrix
    from .network_models import NormalizedInputBatch
    from .optimization_models import BaselineResult

from .case_types import (
    CaseOperationResult,
    CaseState,
    CaseStatusSummary,
    CaseSummary,
    ComponentSummary,
    ComponentWrite,
    FacetName,
    FacetState,
    FacetSummary,
    OperationLease,
    OperationState,
    PurgeResult,
    SelectedMapping,
    SourceSnapshot,
    SourceSummary,
    StorageStatus,
    StoredMappingCandidate,
    ToolOperationSummary,
)

SCHEMA_VERSION = 1
ALL_FACETS = tuple(FacetName)
ModelT = TypeVar("ModelT", bound=BaseModel)


class CaseRepositoryError(RuntimeError):
    def __init__(self, code: str, message: str):
        super().__init__(message)
        self.code = code


def _now() -> str:
    return datetime.now(UTC).isoformat()


def _canonical_json(value: object) -> str:
    if isinstance(value, BaseModel):
        value = value.model_dump(mode="json")
    return json.dumps(value, ensure_ascii=True, sort_keys=True, separators=(",", ":"))


def _sha256(value: str | bytes) -> str:
    raw = value.encode("utf-8") if isinstance(value, str) else value
    return hashlib.sha256(raw).hexdigest()


def workspace_fingerprint(workspace_root: Path) -> str:
    return _sha256(str(workspace_root.resolve()))


class CaseRepository:
    def __init__(self, database_path: Path):
        self.database_path = database_path.resolve()
        self.database_path.parent.mkdir(parents=True, exist_ok=True)
        self._initialize()

    @classmethod
    def from_profile(cls, profile_root: Path | None = None) -> CaseRepository:
        root = (profile_root or Path(os.environ.get("CODEX_HOME", Path.cwd() / ".codex"))).resolve()
        return cls(root / "mcp-state" / "supply-chain-network" / "cases.sqlite3")

    def _connect(self) -> sqlite3.Connection:
        connection = sqlite3.connect(self.database_path, timeout=5.0)
        connection.row_factory = sqlite3.Row
        connection.execute("PRAGMA foreign_keys = ON")
        connection.execute("PRAGMA busy_timeout = 5000")
        return connection

    def _initialize(self) -> None:
        with self._connect() as connection:
            version = int(connection.execute("PRAGMA user_version").fetchone()[0])
            if version not in (0, SCHEMA_VERSION):
                raise CaseRepositoryError(
                    "case_schema_incompatible",
                    "Network Case storage uses an unsupported schema; reset it explicitly.",
                )
            if version == 0:
                schema = Path(__file__).with_name("case_schema.sql").read_text(encoding="utf-8")
                connection.executescript(schema)
            connection.execute("PRAGMA journal_mode = WAL")

    @contextmanager
    def _transaction(self) -> Iterable[sqlite3.Connection]:
        connection = self._connect()
        try:
            connection.execute("BEGIN IMMEDIATE")
            yield connection
            connection.commit()
        except Exception:
            connection.rollback()
            raise
        finally:
            connection.close()

    def create_case(self, workspace_root: Path, country_code: str, intent: str) -> CaseSummary:
        country = country_code.strip().upper()
        if len(country) != 2 or not country.isascii() or not country.isalpha():
            raise CaseRepositoryError("country_code_invalid", "Country code must use two letters.")
        normalized_intent = " ".join(intent.split())
        if not normalized_intent or len(normalized_intent) > 4_000:
            raise CaseRepositoryError("case_intent_invalid", "Case intent is missing or too long.")
        case_id = uuid4()
        timestamp = _now()
        with self._transaction() as connection:
            connection.execute(
                "INSERT INTO network_cases (case_id, workspace_fingerprint, country_code, "
                "intent_sha256, state, revision, created_at, updated_at, last_accessed_at) "
                "VALUES (?, ?, ?, ?, 'active', 1, ?, ?, ?)",
                (
                    str(case_id),
                    workspace_fingerprint(workspace_root),
                    country,
                    _sha256(normalized_intent),
                    timestamp,
                    timestamp,
                    timestamp,
                ),
            )
            connection.executemany(
                "INSERT INTO case_bindings "
                "(case_id, facet, component_id, state, issue_summary_json, updated_at) "
                "VALUES (?, ?, NULL, 'missing', '[]', ?)",
                [(str(case_id), facet.value, timestamp) for facet in ALL_FACETS],
            )
        return CaseSummary(
            case_id=case_id, country_code=country, state=CaseState.ACTIVE, revision=1
        )

    def _case_row(
        self, connection: sqlite3.Connection, case_id: UUID, workspace_root: Path
    ) -> sqlite3.Row:
        row = connection.execute(
            "SELECT * FROM network_cases WHERE case_id = ?", (str(case_id),)
        ).fetchone()
        if row is None:
            raise CaseRepositoryError("case_not_found", "Network Case was not found.")
        if row["workspace_fingerprint"] != workspace_fingerprint(workspace_root):
            raise CaseRepositoryError(
                "case_workspace_mismatch", "Network Case does not belong to this Workspace."
            )
        if row["state"] == CaseState.ARCHIVED.value:
            raise CaseRepositoryError("case_archived", "Network Case is archived.")
        return row

    def get_case(self, case_id: UUID, workspace_root: Path) -> CaseSummary:
        with self._connect() as connection:
            row = self._case_row(connection, case_id, workspace_root)
            connection.execute(
                "UPDATE network_cases SET last_accessed_at = ? WHERE case_id = ?",
                (_now(), str(case_id)),
            )
            connection.commit()
        return CaseSummary(
            case_id=case_id,
            country_code=row["country_code"],
            state=CaseState(row["state"]),
            revision=row["revision"],
        )

    def get_status(self, case_id: UUID, workspace_root: Path) -> CaseStatusSummary:
        case = self.get_case(case_id, workspace_root)
        with self._connect() as connection:
            rows = connection.execute(
                "SELECT binding.facet, binding.state, binding.issue_summary_json, "
                "component.component_revision, component.row_count "
                "FROM case_bindings binding "
                "LEFT JOIN case_components component ON component.component_id = binding.component_id "
                "WHERE binding.case_id = ? ORDER BY binding.facet",
                (str(case_id),),
            ).fetchall()
        facets = []
        for row in rows:
            issues = json.loads(row["issue_summary_json"])
            facets.append(
                FacetSummary(
                    name=FacetName(row["facet"]),
                    state=FacetState(row["state"]),
                    component_revision=row["component_revision"],
                    row_count=row["row_count"],
                    issue_codes=[str(item) for item in issues[:20]],
                )
            )
        return CaseStatusSummary(
            case_id=case.case_id,
            revision=case.revision,
            state=case.state,
            facets=facets,
        )

    def begin_operation(
        self,
        case_id: UUID,
        workspace_root: Path,
        operation_kind: str,
        normalized_input: BaseModel | dict[str, object],
        *,
        retry: bool = False,
    ) -> OperationLease:
        kind = operation_kind.strip()
        if not kind or len(kind) > 128:
            raise CaseRepositoryError("operation_kind_invalid", "Operation kind is invalid.")
        input_sha256 = _sha256(_canonical_json(normalized_input))
        with self._transaction() as connection:
            self._case_row(connection, case_id, workspace_root)
            existing = connection.execute(
                "SELECT * FROM case_operations WHERE case_id = ? AND operation_kind = ? "
                "AND input_sha256 = ?",
                (str(case_id), kind, input_sha256),
            ).fetchone()
            if existing is not None:
                state = OperationState(existing["state"])
                if state == OperationState.COMPLETED:
                    return OperationLease(
                        operation_id=UUID(existing["operation_id"]),
                        case_id=case_id,
                        operation_kind=kind,
                        input_sha256=input_sha256,
                        reused_component_id=(
                            UUID(existing["result_component_id"])
                            if existing["result_component_id"]
                            else None
                        ),
                    )
                if state == OperationState.RUNNING:
                    raise CaseRepositoryError(
                        "operation_in_progress", "The same Case operation is already running."
                    )
                if not retry:
                    raise CaseRepositoryError(
                        "operation_retry_required",
                        "The previous operation failed; retry must be explicit.",
                    )
                connection.execute(
                    "DELETE FROM case_operations WHERE operation_id = ?",
                    (existing["operation_id"],),
                )
            operation_id = uuid4()
            connection.execute(
                "INSERT INTO case_operations "
                "(operation_id, case_id, operation_kind, input_sha256, state, started_at) "
                "VALUES (?, ?, ?, ?, 'running', ?)",
                (str(operation_id), str(case_id), kind, input_sha256, _now()),
            )
        return OperationLease(
            operation_id=operation_id,
            case_id=case_id,
            operation_kind=kind,
            input_sha256=input_sha256,
        )

    def refresh_sources(
        self,
        case_id: UUID,
        workspace_root: Path,
        sources: Sequence[SourceSnapshot],
    ) -> tuple[list[SourceSummary], bool]:
        timestamp = _now()
        changed = False
        summaries: list[SourceSummary] = []
        with self._transaction() as connection:
            self._case_row(connection, case_id, workspace_root)
            active_rows = connection.execute(
                "SELECT * FROM case_sources WHERE case_id = ? AND active = 1",
                (str(case_id),),
            ).fetchall()
            active_by_ref = {row["source_ref"]: row for row in active_rows}
            incoming_refs = {source.source_ref for source in sources}
            for row in active_rows:
                if row["source_ref"] not in incoming_refs:
                    connection.execute(
                        "UPDATE case_sources SET active = 0 WHERE source_id = ?",
                        (row["source_id"],),
                    )
                    changed = True
            for source in sources:
                existing = active_by_ref.get(source.source_ref)
                if existing is not None and existing["content_sha256"] == source.content_sha256:
                    summaries.append(self._source_summary(existing))
                    continue
                if existing is not None:
                    connection.execute(
                        "UPDATE case_sources SET active = 0 WHERE source_id = ?",
                        (existing["source_id"],),
                    )
                revision = connection.execute(
                    "SELECT COALESCE(MAX(source_revision), 0) + 1 FROM case_sources "
                    "WHERE case_id = ? AND source_ref = ?",
                    (str(case_id), source.source_ref),
                ).fetchone()[0]
                source_id = uuid4()
                connection.execute(
                    "INSERT INTO case_sources "
                    "(source_id, case_id, source_ref, display_name, media_type, content_sha256, "
                    "byte_size, row_count, source_revision, active, metadata_json, created_at) "
                    "VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?)",
                    (
                        str(source_id),
                        str(case_id),
                        source.source_ref,
                        source.display_name,
                        source.media_type,
                        source.content_sha256,
                        source.byte_size,
                        source.row_count,
                        revision,
                        source.metadata_json,
                        timestamp,
                    ),
                )
                row = connection.execute(
                    "SELECT * FROM case_sources WHERE source_id = ?", (str(source_id),)
                ).fetchone()
                summaries.append(self._source_summary(row))
                changed = True
            source_state = FacetState.READY if summaries else FacetState.NEEDS_INPUT
            connection.execute(
                "UPDATE case_bindings SET state = ?, component_id = NULL, "
                "issue_summary_json = ?, updated_at = ? WHERE case_id = ? AND facet = ?",
                (
                    source_state.value,
                    "[]" if summaries else '["workspace_sources_missing"]',
                    timestamp,
                    str(case_id),
                    FacetName.SOURCES.value,
                ),
            )
            if changed:
                connection.execute(
                    "UPDATE case_bindings SET state = CASE WHEN component_id IS NULL "
                    "THEN 'missing' ELSE 'stale' END, updated_at = ? "
                    "WHERE case_id = ? AND facet IN "
                    "('mapping', 'normalized_input', 'geography', 'current_assignment', "
                    "'route_matrix', 'cost_matrix', 'baseline', 'scenario', "
                    "'facility_location', 'report', 'map')",
                    (timestamp, str(case_id)),
                )
                connection.execute(
                    "UPDATE network_cases SET revision = revision + 1, updated_at = ? "
                    "WHERE case_id = ?",
                    (timestamp, str(case_id)),
                )
        summaries.sort(key=lambda item: (item.display_name, str(item.source_id)))
        return summaries, changed

    def list_sources(self, case_id: UUID, workspace_root: Path) -> list[SourceSummary]:
        with self._connect() as connection:
            self._case_row(connection, case_id, workspace_root)
            rows = connection.execute(
                "SELECT * FROM case_sources WHERE case_id = ? AND active = 1 "
                "ORDER BY display_name, source_id",
                (str(case_id),),
            ).fetchall()
        return [self._source_summary(row) for row in rows]

    def commit_mapping_proposal(
        self,
        lease: OperationLease,
        workspace_root: Path,
        candidates: Sequence[StoredMappingCandidate],
        *,
        needs_input: bool,
    ) -> CaseOperationResult:
        if not candidates:
            raise CaseRepositoryError(
                "mapping_candidates_empty", "No usable source mappings were found."
            )
        if lease.reused_component_id is not None:
            return self.complete_operation(lease, workspace_root, [])
        canonical = _canonical_json([item.model_dump(mode="json") for item in candidates])
        component_id = uuid4()
        timestamp = _now()
        with self._transaction() as connection:
            self._case_row(connection, lease.case_id, workspace_root)
            operation = connection.execute(
                "SELECT state FROM case_operations WHERE operation_id = ?",
                (str(lease.operation_id),),
            ).fetchone()
            if operation is None or operation["state"] != OperationState.RUNNING.value:
                raise CaseRepositoryError(
                    "operation_not_running", "Case operation is not in the running state."
                )
            revision = connection.execute(
                "SELECT COALESCE(MAX(component_revision), 0) + 1 FROM case_components "
                "WHERE case_id = ? AND component_kind = 'mapping_proposal.v1'",
                (str(lease.case_id),),
            ).fetchone()[0]
            connection.execute(
                "INSERT INTO case_components "
                "(component_id, case_id, component_kind, component_revision, content_sha256, "
                "row_count, metadata_json, created_at) VALUES (?, ?, 'mapping_proposal.v1', "
                "?, ?, ?, ?, ?)",
                (
                    str(component_id),
                    str(lease.case_id),
                    revision,
                    _sha256(canonical),
                    len(candidates),
                    _canonical_json({"needs_input": needs_input}),
                    timestamp,
                ),
            )
            connection.executemany(
                "INSERT INTO mapping_candidates "
                "(candidate_id, component_id, source_id, source_role, target_entity, "
                "target_field, source_field, transform_json, score, reason_code) "
                "VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                [
                    (
                        item.candidate_id,
                        str(component_id),
                        str(item.source_id),
                        item.source_role,
                        item.target_entity,
                        item.target_field,
                        item.source_field,
                        item.transform_json,
                        item.score,
                        item.reason_code,
                    )
                    for item in candidates
                ],
            )
            connection.execute(
                "UPDATE case_bindings SET component_id = ?, state = ?, "
                "issue_summary_json = ?, updated_at = ? WHERE case_id = ? AND facet = 'mapping'",
                (
                    str(component_id),
                    FacetState.NEEDS_INPUT.value if needs_input else FacetState.MISSING.value,
                    '["mapping_confirmation_required"]' if needs_input else "[]",
                    timestamp,
                    str(lease.case_id),
                ),
            )
            connection.execute(
                "UPDATE case_operations SET state = 'completed', result_component_id = ?, "
                "terminal_at = ? WHERE operation_id = ? AND state = 'running'",
                (str(component_id), timestamp, str(lease.operation_id)),
            )
            connection.execute(
                "UPDATE network_cases SET revision = revision + 1, updated_at = ? "
                "WHERE case_id = ?",
                (timestamp, str(lease.case_id)),
            )
        component = ComponentSummary(
            component_id=component_id,
            component_kind="mapping_proposal.v1",
            component_revision=revision,
            content_sha256=_sha256(canonical),
            row_count=len(candidates),
        )
        return CaseOperationResult(
            operation=ToolOperationSummary(
                operation_id=lease.operation_id,
                operation_kind=lease.operation_kind,
                state=OperationState.COMPLETED,
            ),
            components=[component],
        )

    def select_mapping_candidates(
        self,
        case_id: UUID,
        workspace_root: Path,
        candidate_ids: Sequence[str],
    ) -> ComponentSummary:
        if not candidate_ids or len(set(candidate_ids)) != len(candidate_ids):
            raise CaseRepositoryError(
                "mapping_selection_invalid", "Mapping selection must contain unique candidates."
            )
        timestamp = _now()
        with self._transaction() as connection:
            self._case_row(connection, case_id, workspace_root)
            rows = connection.execute(
                f"SELECT candidate.* FROM mapping_candidates candidate "
                f"JOIN case_components component ON component.component_id = candidate.component_id "
                f"WHERE component.case_id = ? AND candidate.candidate_id IN "
                f"({','.join('?' for _ in candidate_ids)})",
                (str(case_id), *candidate_ids),
            ).fetchall()
            if len(rows) != len(candidate_ids):
                raise CaseRepositoryError(
                    "mapping_candidate_not_found", "A selected mapping candidate is unavailable."
                )
            role_by_source: dict[str, str] = {}
            selected_roles: set[str] = set()
            target_keys: set[tuple[str, str, str]] = set()
            for row in rows:
                selected_roles.add(row["source_role"])
                previous = role_by_source.setdefault(row["source_id"], row["source_role"])
                if previous != row["source_role"]:
                    raise CaseRepositoryError(
                        "mapping_source_role_conflict",
                        "One source cannot have multiple business roles in one mapping revision.",
                    )
                key = (row["source_id"], row["target_entity"], row["target_field"])
                if key in target_keys:
                    raise CaseRepositoryError(
                        "mapping_target_conflict", "A target field has multiple selected sources."
                    )
                target_keys.add(key)
            required_roles = {"demand", "existing_warehouse"}
            if not required_roles.issubset(selected_roles):
                raise CaseRepositoryError(
                    "mapping_required_roles_missing",
                    "Demand and existing warehouse sources must both be selected.",
                )
            canonical = _canonical_json(sorted(candidate_ids))
            revision = connection.execute(
                "SELECT COALESCE(MAX(component_revision), 0) + 1 FROM case_components "
                "WHERE case_id = ? AND component_kind = 'mapping_selection.v1'",
                (str(case_id),),
            ).fetchone()[0]
            component_id = uuid4()
            digest = _sha256(canonical)
            connection.execute(
                "INSERT INTO case_components "
                "(component_id, case_id, component_kind, component_revision, content_sha256, "
                "row_count, metadata_json, created_at) VALUES (?, ?, 'mapping_selection.v1', "
                "?, ?, ?, '{}', ?)",
                (
                    str(component_id),
                    str(case_id),
                    revision,
                    digest,
                    len(candidate_ids),
                    timestamp,
                ),
            )
            connection.executemany(
                "INSERT INTO mapping_selections (component_id, candidate_id) VALUES (?, ?)",
                [(str(component_id), candidate_id) for candidate_id in candidate_ids],
            )
            connection.execute(
                "UPDATE case_bindings SET component_id = ?, state = 'ready', "
                "issue_summary_json = '[]', updated_at = ? WHERE case_id = ? AND facet = 'mapping'",
                (str(component_id), timestamp, str(case_id)),
            )
            connection.execute(
                "UPDATE case_bindings SET state = CASE WHEN component_id IS NULL THEN 'missing' "
                "ELSE 'stale' END, updated_at = ? WHERE case_id = ? AND facet IN "
                "('normalized_input', 'geography', 'current_assignment', 'route_matrix', "
                "'cost_matrix', 'baseline', 'scenario', 'facility_location', 'report', 'map')",
                (timestamp, str(case_id)),
            )
            connection.execute(
                "UPDATE network_cases SET revision = revision + 1, updated_at = ? "
                "WHERE case_id = ?",
                (timestamp, str(case_id)),
            )
        return ComponentSummary(
            component_id=component_id,
            component_kind="mapping_selection.v1",
            component_revision=revision,
            content_sha256=digest,
            row_count=len(candidate_ids),
        )

    def selected_mappings(self, case_id: UUID, workspace_root: Path) -> list[SelectedMapping]:
        with self._connect() as connection:
            self._case_row(connection, case_id, workspace_root)
            binding = connection.execute(
                "SELECT component_id, state FROM case_bindings "
                "WHERE case_id = ? AND facet = 'mapping'",
                (str(case_id),),
            ).fetchone()
            if binding is None or binding["state"] != FacetState.READY.value:
                raise CaseRepositoryError(
                    "mapping_not_ready", "A confirmed mapping is required before normalization."
                )
            rows = connection.execute(
                "SELECT candidate.*, source.source_ref FROM mapping_selections selection "
                "JOIN mapping_candidates candidate ON candidate.candidate_id = selection.candidate_id "
                "JOIN case_sources source ON source.source_id = candidate.source_id "
                "WHERE selection.component_id = ? AND source.active = 1 "
                "ORDER BY candidate.source_id, candidate.target_entity, candidate.target_field",
                (binding["component_id"],),
            ).fetchall()
        return [
            SelectedMapping(
                candidate_id=row["candidate_id"],
                source_id=UUID(row["source_id"]),
                source_ref=row["source_ref"],
                source_role=row["source_role"],
                target_entity=row["target_entity"],
                target_field=row["target_field"],
                source_field=row["source_field"],
                transform_json=row["transform_json"],
            )
            for row in rows
        ]

    def set_facet_state(
        self,
        case_id: UUID,
        workspace_root: Path,
        facet: FacetName,
        state: FacetState,
        issue_codes: Sequence[str],
    ) -> None:
        with self._transaction() as connection:
            self._case_row(connection, case_id, workspace_root)
            connection.execute(
                "UPDATE case_bindings SET state = ?, issue_summary_json = ?, updated_at = ? "
                "WHERE case_id = ? AND facet = ?",
                (
                    state.value,
                    _canonical_json(list(issue_codes)[:20]),
                    _now(),
                    str(case_id),
                    facet.value,
                ),
            )

    def commit_normalized_input(
        self,
        lease: OperationLease,
        workspace_root: Path,
        batch: NormalizedInputBatch,
    ) -> CaseOperationResult:
        from .network_models import NormalizedInputBatch

        if not isinstance(batch, NormalizedInputBatch):
            raise TypeError("batch must be NormalizedInputBatch")
        if lease.reused_component_id is not None:
            return self.complete_operation(lease, workspace_root, [])
        canonical = _canonical_json(batch)
        digest = _sha256(canonical)
        component_id = uuid4()
        timestamp = _now()
        row_count = (
            len(batch.demand_cities)
            + len(batch.warehouses)
            + len(batch.current_assignments)
            + len(batch.route_quotes)
        )
        with self._transaction() as connection:
            self._case_row(connection, lease.case_id, workspace_root)
            previous_binding = connection.execute(
                "SELECT component_id FROM case_bindings WHERE case_id = ? "
                "AND facet = 'normalized_input'",
                (str(lease.case_id),),
            ).fetchone()
            operation = connection.execute(
                "SELECT state FROM case_operations WHERE operation_id = ?",
                (str(lease.operation_id),),
            ).fetchone()
            if operation is None or operation["state"] != OperationState.RUNNING.value:
                raise CaseRepositoryError(
                    "operation_not_running", "Case operation is not in the running state."
                )
            revision = connection.execute(
                "SELECT COALESCE(MAX(component_revision), 0) + 1 FROM case_components "
                "WHERE case_id = ? AND component_kind = 'normalized_input.v1'",
                (str(lease.case_id),),
            ).fetchone()[0]
            connection.execute(
                "INSERT INTO case_components "
                "(component_id, case_id, component_kind, component_revision, content_sha256, "
                "row_count, metadata_json, created_at) VALUES (?, ?, 'normalized_input.v1', "
                "?, ?, ?, ?, ?)",
                (
                    str(component_id),
                    str(lease.case_id),
                    revision,
                    digest,
                    row_count,
                    _canonical_json(
                        {
                            "demand_count": len(batch.demand_cities),
                            "warehouse_count": len(batch.warehouses),
                            "current_assignment_count": len(batch.current_assignments),
                            "route_quote_count": len(batch.route_quotes),
                            "issue_codes": [issue.code for issue in batch.issues],
                        }
                    ),
                    timestamp,
                ),
            )
            connection.executemany(
                "INSERT INTO demand_cities "
                "(component_id, city_id, city_name, province_id, province_name, demand_quantity, "
                "longitude, latitude) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                [
                    (
                        str(component_id),
                        row.city_id,
                        row.city_name,
                        row.province_id,
                        row.province_name,
                        str(row.demand_quantity),
                        row.longitude,
                        row.latitude,
                    )
                    for row in batch.demand_cities
                ],
            )
            connection.executemany(
                "INSERT INTO warehouses "
                "(component_id, warehouse_id, warehouse_name, warehouse_type, city_id, city_name, "
                "longitude, latitude, upstream_center_id, is_existing, is_fixed) "
                "VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                [
                    (
                        str(component_id),
                        row.warehouse_id,
                        row.warehouse_name,
                        row.warehouse_type,
                        row.city_id,
                        row.city_name,
                        row.longitude,
                        row.latitude,
                        row.upstream_center_id,
                        int(row.is_existing),
                        int(row.is_fixed),
                    )
                    for row in batch.warehouses
                ],
            )
            connection.executemany(
                "INSERT INTO current_assignments "
                "(component_id, demand_city_id, serving_warehouse_id, upstream_center_id) "
                "VALUES (?, ?, ?, ?)",
                [
                    (
                        str(component_id),
                        row.demand_city_id,
                        row.serving_warehouse_id,
                        row.upstream_center_id,
                    )
                    for row in batch.current_assignments
                ],
            )
            connection.executemany(
                "INSERT INTO route_quotes "
                "(component_id, origin_id, destination_id, layer, price_per_vehicle, "
                "currency, vehicle_capacity) VALUES (?, ?, ?, ?, ?, ?, ?)",
                [
                    (
                        str(component_id),
                        row.origin_id,
                        row.destination_id,
                        row.layer,
                        str(row.price_per_vehicle),
                        row.currency,
                        str(row.vehicle_capacity),
                    )
                    for row in batch.route_quotes
                ],
            )
            if previous_binding is not None and previous_binding["component_id"] is not None:
                self._invalidate_bound_dependents(
                    connection,
                    lease.case_id,
                    UUID(previous_binding["component_id"]),
                )
            connection.execute(
                "UPDATE case_bindings SET component_id = ?, state = 'ready', "
                "issue_summary_json = ?, updated_at = ? "
                "WHERE case_id = ? AND facet = 'normalized_input'",
                (
                    str(component_id),
                    _canonical_json([issue.code for issue in batch.issues]),
                    timestamp,
                    str(lease.case_id),
                ),
            )
            assignment_state = (
                FacetState.READY.value if batch.current_assignments else FacetState.MISSING.value
            )
            connection.execute(
                "UPDATE case_bindings SET component_id = ?, state = ?, "
                "issue_summary_json = ?, updated_at = ? "
                "WHERE case_id = ? AND facet = 'current_assignment'",
                (
                    str(component_id) if batch.current_assignments else None,
                    assignment_state,
                    "[]" if batch.current_assignments else '["current_assignment_missing"]',
                    timestamp,
                    str(lease.case_id),
                ),
            )
            geography_ready = all(
                row.longitude is not None and row.latitude is not None
                for row in [*batch.demand_cities, *batch.warehouses]
            )
            connection.execute(
                "UPDATE case_bindings SET component_id = ?, state = ?, "
                "issue_summary_json = ?, updated_at = ? "
                "WHERE case_id = ? AND facet = 'geography'",
                (
                    str(component_id) if geography_ready else None,
                    FacetState.READY.value if geography_ready else FacetState.NEEDS_INPUT.value,
                    "[]" if geography_ready else '["coordinates_missing"]',
                    timestamp,
                    str(lease.case_id),
                ),
            )
            connection.execute(
                "UPDATE case_operations SET state = 'completed', result_component_id = ?, "
                "terminal_at = ? WHERE operation_id = ? AND state = 'running'",
                (str(component_id), timestamp, str(lease.operation_id)),
            )
            connection.execute(
                "UPDATE network_cases SET revision = revision + 1, updated_at = ? "
                "WHERE case_id = ?",
                (timestamp, str(lease.case_id)),
            )
        return CaseOperationResult(
            operation=ToolOperationSummary(
                operation_id=lease.operation_id,
                operation_kind=lease.operation_kind,
                state=OperationState.COMPLETED,
            ),
            components=[
                ComponentSummary(
                    component_id=component_id,
                    component_kind="normalized_input.v1",
                    component_revision=revision,
                    content_sha256=digest,
                    row_count=row_count,
                )
            ],
        )

    def load_normalized_input(
        self, case_id: UUID, workspace_root: Path
    ) -> tuple[NormalizedInputBatch, UUID]:
        from .network_models import (
            CurrentAssignmentRecord,
            DemandCityRecord,
            NormalizedInputBatch,
            RouteQuoteRecord,
            WarehouseRecord,
        )

        with self._connect() as connection:
            self._case_row(connection, case_id, workspace_root)
            component_id = self._ready_component_id(connection, case_id, FacetName.NORMALIZED_INPUT)
            demand_rows = connection.execute(
                "SELECT * FROM demand_cities WHERE component_id = ? ORDER BY city_id",
                (str(component_id),),
            ).fetchall()
            warehouse_rows = connection.execute(
                "SELECT * FROM warehouses WHERE component_id = ? ORDER BY warehouse_id",
                (str(component_id),),
            ).fetchall()
            assignment_rows = connection.execute(
                "SELECT * FROM current_assignments WHERE component_id = ? ORDER BY demand_city_id",
                (str(component_id),),
            ).fetchall()
            quote_rows = connection.execute(
                "SELECT * FROM route_quotes WHERE component_id = ? "
                "ORDER BY origin_id, destination_id, layer",
                (str(component_id),),
            ).fetchall()
        return (
            NormalizedInputBatch(
                demand_cities=[
                    DemandCityRecord(
                        city_id=row["city_id"],
                        city_name=row["city_name"],
                        province_id=row["province_id"],
                        province_name=row["province_name"],
                        demand_quantity=Decimal(row["demand_quantity"]),
                        longitude=row["longitude"],
                        latitude=row["latitude"],
                    )
                    for row in demand_rows
                ],
                warehouses=[
                    WarehouseRecord(
                        warehouse_id=row["warehouse_id"],
                        warehouse_name=row["warehouse_name"],
                        warehouse_type=row["warehouse_type"],
                        city_id=row["city_id"],
                        city_name=row["city_name"],
                        longitude=row["longitude"],
                        latitude=row["latitude"],
                        upstream_center_id=row["upstream_center_id"],
                        is_existing=bool(row["is_existing"]),
                        is_fixed=bool(row["is_fixed"]),
                    )
                    for row in warehouse_rows
                ],
                current_assignments=[
                    CurrentAssignmentRecord(
                        demand_city_id=row["demand_city_id"],
                        serving_warehouse_id=row["serving_warehouse_id"],
                        upstream_center_id=row["upstream_center_id"],
                    )
                    for row in assignment_rows
                ],
                route_quotes=[
                    RouteQuoteRecord(
                        origin_id=row["origin_id"],
                        destination_id=row["destination_id"],
                        layer=row["layer"],
                        price_per_vehicle=Decimal(row["price_per_vehicle"]),
                        currency=row["currency"],
                        vehicle_capacity=Decimal(row["vehicle_capacity"]),
                    )
                    for row in quote_rows
                ],
            ),
            component_id,
        )

    def commit_route_matrix(
        self,
        lease: OperationLease,
        workspace_root: Path,
        matrix: RouteMatrix,
        depends_on: Sequence[UUID],
    ) -> CaseOperationResult:
        from .matrix_models import RouteMatrix

        if not isinstance(matrix, RouteMatrix):
            raise TypeError("matrix must be RouteMatrix")
        return self._commit_matrix(
            lease,
            workspace_root,
            component_kind="route_matrix.v1",
            facet=FacetName.ROUTE_MATRIX,
            rows=matrix.rows,
            metadata={
                "method": matrix.method,
                "missing_route_count": len(matrix.missing_routes),
                "validation": matrix.validation,
            },
            depends_on=depends_on,
            insert_sql=(
                "INSERT INTO route_matrix_rows "
                "(component_id, origin_id, destination_id, distance_km, duration_hours, method) "
                "VALUES (?, ?, ?, ?, ?, ?)"
            ),
            row_values=lambda component_id, row: (
                component_id,
                row.origin_id,
                row.destination_id,
                row.distance_km,
                row.duration_hours,
                row.method,
            ),
        )

    def load_route_matrix(self, case_id: UUID, workspace_root: Path) -> tuple[RouteMatrix, UUID]:
        from .matrix_models import RouteMatrix, RouteMatrixRow

        with self._connect() as connection:
            self._case_row(connection, case_id, workspace_root)
            component_id = self._ready_component_id(connection, case_id, FacetName.ROUTE_MATRIX)
            component = connection.execute(
                "SELECT metadata_json FROM case_components WHERE component_id = ?",
                (str(component_id),),
            ).fetchone()
            rows = connection.execute(
                "SELECT * FROM route_matrix_rows WHERE component_id = ? "
                "ORDER BY origin_id, destination_id",
                (str(component_id),),
            ).fetchall()
        metadata = json.loads(component["metadata_json"])
        return (
            RouteMatrix(
                method=metadata["method"],
                rows=[
                    RouteMatrixRow(
                        origin_id=row["origin_id"],
                        destination_id=row["destination_id"],
                        distance_km=row["distance_km"],
                        duration_hours=row["duration_hours"],
                        method=row["method"],
                    )
                    for row in rows
                ],
                missing_routes=[],
                validation=metadata.get("validation", {}),
            ),
            component_id,
        )

    def commit_cost_matrix(
        self,
        lease: OperationLease,
        workspace_root: Path,
        matrix: CostMatrix,
        depends_on: Sequence[UUID],
    ) -> CaseOperationResult:
        from .matrix_models import CostMatrix

        if not isinstance(matrix, CostMatrix):
            raise TypeError("matrix must be CostMatrix")
        return self._commit_matrix(
            lease,
            workspace_root,
            component_kind="cost_matrix.v1",
            facet=FacetName.COST_MATRIX,
            rows=matrix.rows,
            metadata={
                "currency": matrix.currency,
                "warehouse_scope": matrix.warehouse_scope,
                "missing_route_count": len(matrix.missing_routes),
                "calculation_rule": matrix.calculation_rule,
            },
            depends_on=depends_on,
            insert_sql=(
                "INSERT INTO cost_matrix_rows "
                "(component_id, origin_id, destination_id, layer, "
                "cost_per_demand_unit, currency, source) VALUES (?, ?, ?, ?, ?, ?, ?)"
            ),
            row_values=lambda component_id, row: (
                component_id,
                row.origin_id,
                row.destination_id,
                row.layer,
                str(row.cost_per_demand_unit),
                row.currency,
                row.source,
            ),
        )

    def load_cost_matrix(self, case_id: UUID, workspace_root: Path) -> tuple[CostMatrix, UUID]:
        from .matrix_models import CostMatrix, CostMatrixRow

        with self._connect() as connection:
            self._case_row(connection, case_id, workspace_root)
            component_id = self._ready_component_id(connection, case_id, FacetName.COST_MATRIX)
            component = connection.execute(
                "SELECT metadata_json FROM case_components WHERE component_id = ?",
                (str(component_id),),
            ).fetchone()
            rows = connection.execute(
                "SELECT * FROM cost_matrix_rows WHERE component_id = ? "
                "ORDER BY origin_id, destination_id, layer",
                (str(component_id),),
            ).fetchall()
        metadata = json.loads(component["metadata_json"])
        return (
            CostMatrix(
                currency=metadata["currency"],
                warehouse_scope=metadata["warehouse_scope"],
                rows=[
                    CostMatrixRow(
                        origin_id=row["origin_id"],
                        destination_id=row["destination_id"],
                        layer=row["layer"],
                        cost_per_demand_unit=float(row["cost_per_demand_unit"]),
                        currency=row["currency"],
                        source=row["source"],
                    )
                    for row in rows
                ],
                missing_routes=[],
                calculation_rule=metadata.get("calculation_rule"),
            ),
            component_id,
        )

    def commit_baseline(
        self,
        lease: OperationLease,
        workspace_root: Path,
        baseline: BaselineResult,
        depends_on: Sequence[UUID],
    ) -> CaseOperationResult:
        from .optimization_models import BaselineResult

        if not isinstance(baseline, BaselineResult):
            raise TypeError("baseline must be BaselineResult")
        if lease.reused_component_id is not None:
            return self.complete_operation(lease, workspace_root, [])
        canonical = _canonical_json(baseline)
        digest = _sha256(canonical)
        component_id = uuid4()
        timestamp = _now()
        metadata = {
            "label": baseline.label,
            "objective": baseline.assignment.objective,
            "total_demand": str(baseline.assignment.total_demand),
            "unassigned_demand": str(baseline.assignment.unassigned_demand),
            "notice_code": baseline.notice_code,
            "cost": (baseline.cost.model_dump(mode="json") if baseline.cost is not None else None),
        }
        with self._transaction() as connection:
            self._case_row(connection, lease.case_id, workspace_root)
            operation = connection.execute(
                "SELECT state FROM case_operations WHERE operation_id = ?",
                (str(lease.operation_id),),
            ).fetchone()
            if operation is None or operation["state"] != OperationState.RUNNING.value:
                raise CaseRepositoryError(
                    "operation_not_running", "Case operation is not in the running state."
                )
            previous_binding = connection.execute(
                "SELECT component_id FROM case_bindings WHERE case_id = ? AND facet = 'baseline'",
                (str(lease.case_id),),
            ).fetchone()
            revision = connection.execute(
                "SELECT COALESCE(MAX(component_revision), 0) + 1 FROM case_components "
                "WHERE case_id = ? AND component_kind = 'network_baseline.v1'",
                (str(lease.case_id),),
            ).fetchone()[0]
            connection.execute(
                "INSERT INTO case_components "
                "(component_id, case_id, component_kind, component_revision, content_sha256, "
                "row_count, metadata_json, created_at) VALUES (?, ?, 'network_baseline.v1', "
                "?, ?, ?, ?, ?)",
                (
                    str(component_id),
                    str(lease.case_id),
                    revision,
                    digest,
                    len(baseline.assignment.rows),
                    _canonical_json(metadata),
                    timestamp,
                ),
            )
            connection.executemany(
                "INSERT INTO case_component_dependencies "
                "(component_id, depends_on_component_id) VALUES (?, ?)",
                [(str(component_id), str(item)) for item in depends_on],
            )
            connection.executemany(
                "INSERT INTO assignment_rows "
                "(component_id, demand_city_id, warehouse_id, upstream_center_id, "
                "demand_quantity, distance_km, duration_hours, cost, reason) "
                "VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                [
                    (
                        str(component_id),
                        row.demand_city_id,
                        row.warehouse_id,
                        row.upstream_center_id,
                        str(row.demand_quantity),
                        row.distance_km,
                        row.duration_hours,
                        str(row.cost) if row.cost is not None else None,
                        row.reason,
                    )
                    for row in baseline.assignment.rows
                ],
            )
            connection.executemany(
                "INSERT INTO service_metrics "
                "(component_id, scope_id, target_hours, covered_demand, total_demand, "
                "coverage_rate) VALUES (?, 'network', ?, ?, ?, ?)",
                [
                    (
                        str(component_id),
                        metric.target_hours,
                        str(metric.covered_demand),
                        str(metric.total_demand),
                        metric.coverage_rate,
                    )
                    for metric in baseline.service
                ],
            )
            if baseline.cost is not None:
                connection.executemany(
                    "INSERT INTO warehouse_cost_summaries "
                    "(component_id, warehouse_id, linehaul_cost, last_mile_cost, "
                    "total_cost, currency) VALUES (?, ?, ?, ?, ?, ?)",
                    [
                        (
                            str(component_id),
                            warehouse_id,
                            str(baseline.cost.linehaul_by_warehouse.get(warehouse_id, 0)),
                            str(baseline.cost.last_mile_by_warehouse.get(warehouse_id, 0)),
                            str(total),
                            baseline.cost.currency,
                        )
                        for warehouse_id, total in baseline.cost.by_warehouse.items()
                    ],
                )
            if previous_binding is not None and previous_binding["component_id"] is not None:
                self._invalidate_bound_dependents(
                    connection,
                    lease.case_id,
                    UUID(previous_binding["component_id"]),
                )
            connection.execute(
                "UPDATE case_bindings SET component_id = ?, state = 'ready', "
                "issue_summary_json = ?, updated_at = ? "
                "WHERE case_id = ? AND facet = 'baseline'",
                (
                    str(component_id),
                    (
                        _canonical_json([baseline.notice_code])
                        if baseline.notice_code is not None
                        else "[]"
                    ),
                    timestamp,
                    str(lease.case_id),
                ),
            )
            connection.execute(
                "UPDATE case_operations SET state = 'completed', result_component_id = ?, "
                "terminal_at = ? WHERE operation_id = ? AND state = 'running'",
                (str(component_id), timestamp, str(lease.operation_id)),
            )
            connection.execute(
                "UPDATE network_cases SET revision = revision + 1, updated_at = ? "
                "WHERE case_id = ?",
                (timestamp, str(lease.case_id)),
            )
        return CaseOperationResult(
            operation=ToolOperationSummary(
                operation_id=lease.operation_id,
                operation_kind=lease.operation_kind,
                state=OperationState.COMPLETED,
            ),
            components=[
                ComponentSummary(
                    component_id=component_id,
                    component_kind="network_baseline.v1",
                    component_revision=revision,
                    content_sha256=digest,
                    row_count=len(baseline.assignment.rows),
                )
            ],
        )

    def load_baseline(self, case_id: UUID, workspace_root: Path) -> tuple[BaselineResult, UUID]:
        from .optimization_models import (
            AssignmentResult,
            AssignmentRow,
            BaselineResult,
            CostSummary,
            ServiceMetric,
        )

        with self._connect() as connection:
            self._case_row(connection, case_id, workspace_root)
            component_id = self._ready_component_id(connection, case_id, FacetName.BASELINE)
            component = connection.execute(
                "SELECT metadata_json FROM case_components WHERE component_id = ?",
                (str(component_id),),
            ).fetchone()
            assignment_rows = connection.execute(
                "SELECT * FROM assignment_rows WHERE component_id = ? ORDER BY demand_city_id",
                (str(component_id),),
            ).fetchall()
            metric_rows = connection.execute(
                "SELECT * FROM service_metrics WHERE component_id = ? ORDER BY target_hours",
                (str(component_id),),
            ).fetchall()
        metadata = json.loads(component["metadata_json"])
        assignment = AssignmentResult(
            objective=metadata["objective"],
            rows=[
                AssignmentRow(
                    demand_city_id=row["demand_city_id"],
                    warehouse_id=row["warehouse_id"],
                    upstream_center_id=row["upstream_center_id"],
                    demand_quantity=Decimal(row["demand_quantity"]),
                    distance_km=row["distance_km"],
                    duration_hours=row["duration_hours"],
                    cost=float(row["cost"]) if row["cost"] is not None else None,
                    reason=row["reason"],
                )
                for row in assignment_rows
            ],
            total_demand=Decimal(metadata["total_demand"]),
            unassigned_demand=Decimal(metadata["unassigned_demand"]),
        )
        return (
            BaselineResult(
                label=metadata["label"],
                assignment=assignment,
                service=[
                    ServiceMetric(
                        target_hours=row["target_hours"],
                        covered_demand=Decimal(row["covered_demand"]),
                        total_demand=Decimal(row["total_demand"]),
                        coverage_rate=row["coverage_rate"],
                    )
                    for row in metric_rows
                ],
                cost=(
                    CostSummary.model_validate(metadata["cost"])
                    if metadata.get("cost") is not None
                    else None
                ),
                notice_code=metadata.get("notice_code"),
            ),
            component_id,
        )

    def commit_scenario(
        self,
        lease: OperationLease,
        workspace_root: Path,
        scenario_spec: object,
        scenario: object,
        depends_on: Sequence[UUID],
    ) -> CaseOperationResult:
        from .optimization_models import ScenarioResult, ScenarioSpec

        if not isinstance(scenario_spec, ScenarioSpec) or not isinstance(scenario, ScenarioResult):
            raise TypeError("scenario_spec and scenario must use current contracts")
        return self._commit_assignment_component(
            lease,
            workspace_root,
            component_kind="network_scenario.v1",
            facet=FacetName.SCENARIO,
            assignment=scenario.assignment,
            service=scenario.service,
            cost=scenario.cost,
            metadata={
                "scenario_spec": scenario_spec.model_dump(mode="json"),
                "warehouse_changes": scenario.warehouse_changes,
            },
            depends_on=depends_on,
            extension_sql=(
                "INSERT INTO network_scenarios "
                "(component_id, scenario_spec_json, result_summary_json) VALUES (?, ?, ?)"
            ),
            extension_values=lambda component_id: (
                component_id,
                _canonical_json(scenario_spec),
                _canonical_json(
                    {
                        "objective": scenario.assignment.objective,
                        "total_demand": str(scenario.assignment.total_demand),
                        "unassigned_demand": str(scenario.assignment.unassigned_demand),
                        "warehouse_changes": scenario.warehouse_changes,
                    }
                ),
            ),
        )

    def load_scenario(self, case_id: UUID, workspace_root: Path) -> tuple[object, UUID]:
        from .optimization_models import ScenarioResult

        component_id, metadata, assignment, service, cost = self._load_assignment_component(
            case_id, workspace_root, FacetName.SCENARIO
        )
        return (
            ScenarioResult(
                assignment=assignment,
                service=service,
                cost=cost,
                warehouse_changes=metadata.get("warehouse_changes", {}),
            ),
            component_id,
        )

    def commit_facility_solution(
        self,
        lease: OperationLease,
        workspace_root: Path,
        solution: object,
        depends_on: Sequence[UUID],
    ) -> CaseOperationResult:
        from .optimization_models import PMedianSolution

        if not isinstance(solution, PMedianSolution):
            raise TypeError("solution must be PMedianSolution")
        assignment = solution.assignment
        if assignment is None:
            return self._commit_facility_without_assignment(
                lease, workspace_root, solution, depends_on
            )
        return self._commit_assignment_component(
            lease,
            workspace_root,
            component_kind="facility_location_solution.v2",
            facet=FacetName.FACILITY_LOCATION,
            assignment=assignment,
            service=solution.service,
            cost=None,
            metadata={
                "status": solution.status,
                "selected_warehouse_ids": solution.selected_warehouse_ids,
                "objective_value": solution.objective_value,
                "best_bound": solution.best_bound,
                "optimality": solution.optimality,
                "message": solution.message,
            },
            depends_on=depends_on,
            extension_sql=(
                "INSERT INTO facility_location_solutions "
                "(component_id, solver_state, optimal, feasible, objective_value, "
                "best_bound, selected_warehouses_json) VALUES (?, ?, ?, ?, ?, ?, ?)"
            ),
            extension_values=lambda component_id: (
                component_id,
                solution.status,
                int(solution.status == "optimal"),
                int(solution.status in {"optimal", "feasible", "timeout"}),
                solution.objective_value,
                solution.best_bound,
                _canonical_json(solution.selected_warehouse_ids),
            ),
        )

    def load_facility_solution(self, case_id: UUID, workspace_root: Path) -> tuple[object, UUID]:
        from .optimization_models import PMedianSolution

        with self._connect() as connection:
            self._case_row(connection, case_id, workspace_root)
            component_id = self._ready_component_id(
                connection, case_id, FacetName.FACILITY_LOCATION
            )
            row = connection.execute(
                "SELECT row_count, metadata_json FROM case_components WHERE component_id = ?",
                (str(component_id),),
            ).fetchone()
        if row["row_count"] == 0:
            metadata = json.loads(row["metadata_json"])
            return PMedianSolution.model_validate(metadata), component_id
        component_id, metadata, assignment, service, _ = self._load_assignment_component(
            case_id, workspace_root, FacetName.FACILITY_LOCATION
        )
        return (
            PMedianSolution(
                status=metadata["status"],
                selected_warehouse_ids=metadata.get("selected_warehouse_ids", []),
                assignment=assignment if assignment.rows else None,
                objective_value=metadata.get("objective_value"),
                best_bound=metadata.get("best_bound"),
                service=service,
                optimality=metadata["optimality"],
                message=metadata.get("message"),
            ),
            component_id,
        )

    def commit_deliverable(
        self,
        lease: OperationLease,
        workspace_root: Path,
        *,
        component_kind: str,
        facet: FacetName,
        artifact_schema: str,
        media_type: str,
        byte_size: int,
        content_sha256: str,
        storage_name: str,
        summary: dict[str, object],
        depends_on: Sequence[UUID],
    ) -> CaseOperationResult:
        if lease.reused_component_id is not None:
            return self.complete_operation(lease, workspace_root, [])
        if byte_size < 0 or len(content_sha256) != 64:
            raise CaseRepositoryError(
                "deliverable_metadata_invalid", "Deliverable metadata is invalid."
            )
        component_id = uuid4()
        timestamp = _now()
        digest = _sha256(
            _canonical_json(
                {
                    "artifact_schema": artifact_schema,
                    "media_type": media_type,
                    "byte_size": byte_size,
                    "content_sha256": content_sha256,
                    "storage_name": storage_name,
                    "summary": summary,
                }
            )
        )
        with self._transaction() as connection:
            self._case_row(connection, lease.case_id, workspace_root)
            revision = self._next_component_revision(
                connection, lease.case_id, component_kind
            )
            self._insert_component(
                connection,
                component_id,
                lease.case_id,
                component_kind,
                revision,
                digest,
                1,
                summary,
                timestamp,
                depends_on,
            )
            connection.execute(
                "INSERT INTO case_deliverables "
                "(deliverable_id, case_id, component_id, artifact_schema, media_type, "
                "byte_size, content_sha256, storage_name, created_at) "
                "VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                (
                    str(uuid4()),
                    str(lease.case_id),
                    str(component_id),
                    artifact_schema,
                    media_type,
                    byte_size,
                    content_sha256,
                    storage_name,
                    timestamp,
                ),
            )
            self._finish_component_binding(
                connection, lease, facet, component_id, timestamp
            )
        return self._operation_result(
            lease, component_id, component_kind, revision, digest, 1
        )

    def _commit_facility_without_assignment(
        self,
        lease: OperationLease,
        workspace_root: Path,
        solution: object,
        depends_on: Sequence[UUID],
    ) -> CaseOperationResult:
        if lease.reused_component_id is not None:
            return self.complete_operation(lease, workspace_root, [])
        canonical = _canonical_json(solution)
        digest = _sha256(canonical)
        component_id = uuid4()
        timestamp = _now()
        metadata = solution.model_dump(mode="json")
        with self._transaction() as connection:
            self._case_row(connection, lease.case_id, workspace_root)
            revision = self._next_component_revision(
                connection, lease.case_id, "facility_location_solution.v2"
            )
            self._insert_component(
                connection,
                component_id,
                lease.case_id,
                "facility_location_solution.v2",
                revision,
                digest,
                0,
                metadata,
                timestamp,
                depends_on,
            )
            connection.execute(
                "INSERT INTO facility_location_solutions "
                "(component_id, solver_state, optimal, feasible, objective_value, "
                "best_bound, selected_warehouses_json) VALUES (?, ?, 0, 0, ?, ?, ?)",
                (
                    str(component_id),
                    solution.status,
                    solution.objective_value,
                    solution.best_bound,
                    _canonical_json(solution.selected_warehouse_ids),
                ),
            )
            self._finish_component_binding(
                connection,
                lease,
                FacetName.FACILITY_LOCATION,
                component_id,
                timestamp,
            )
        return self._operation_result(
            lease,
            component_id,
            "facility_location_solution.v2",
            revision,
            digest,
            0,
        )

    def _commit_assignment_component(
        self,
        lease: OperationLease,
        workspace_root: Path,
        *,
        component_kind: str,
        facet: FacetName,
        assignment: object,
        service: Sequence[object],
        cost: object | None,
        metadata: dict[str, object],
        depends_on: Sequence[UUID],
        extension_sql: str,
        extension_values,
    ) -> CaseOperationResult:
        if lease.reused_component_id is not None:
            return self.complete_operation(lease, workspace_root, [])
        metadata = {
            **metadata,
            "objective": assignment.objective,
            "total_demand": str(assignment.total_demand),
            "unassigned_demand": str(assignment.unassigned_demand),
            "cost": cost.model_dump(mode="json") if cost is not None else None,
        }
        canonical = _canonical_json(
            {
                "metadata": metadata,
                "assignment": assignment.model_dump(mode="json"),
                "service": [item.model_dump(mode="json") for item in service],
            }
        )
        digest = _sha256(canonical)
        component_id = uuid4()
        timestamp = _now()
        with self._transaction() as connection:
            self._case_row(connection, lease.case_id, workspace_root)
            revision = self._next_component_revision(connection, lease.case_id, component_kind)
            self._insert_component(
                connection,
                component_id,
                lease.case_id,
                component_kind,
                revision,
                digest,
                len(assignment.rows),
                metadata,
                timestamp,
                depends_on,
            )
            connection.executemany(
                "INSERT INTO assignment_rows "
                "(component_id, demand_city_id, warehouse_id, upstream_center_id, "
                "demand_quantity, distance_km, duration_hours, cost, reason) "
                "VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                [
                    (
                        str(component_id),
                        row.demand_city_id,
                        row.warehouse_id,
                        row.upstream_center_id,
                        str(row.demand_quantity),
                        row.distance_km,
                        row.duration_hours,
                        str(row.cost) if row.cost is not None else None,
                        row.reason,
                    )
                    for row in assignment.rows
                ],
            )
            connection.executemany(
                "INSERT INTO service_metrics "
                "(component_id, scope_id, target_hours, covered_demand, total_demand, "
                "coverage_rate) VALUES (?, 'network', ?, ?, ?, ?)",
                [
                    (
                        str(component_id),
                        item.target_hours,
                        str(item.covered_demand),
                        str(item.total_demand),
                        item.coverage_rate,
                    )
                    for item in service
                ],
            )
            if cost is not None:
                self._insert_cost_summaries(connection, component_id, cost)
            connection.execute(extension_sql, extension_values(str(component_id)))
            self._finish_component_binding(connection, lease, facet, component_id, timestamp)
        return self._operation_result(
            lease,
            component_id,
            component_kind,
            revision,
            digest,
            len(assignment.rows),
        )

    def _load_assignment_component(
        self,
        case_id: UUID,
        workspace_root: Path,
        facet: FacetName,
    ) -> tuple[UUID, dict[str, object], object, list[object], object | None]:
        from .optimization_models import (
            AssignmentResult,
            AssignmentRow,
            CostSummary,
            ServiceMetric,
        )

        with self._connect() as connection:
            self._case_row(connection, case_id, workspace_root)
            component_id = self._ready_component_id(connection, case_id, facet)
            component = connection.execute(
                "SELECT metadata_json FROM case_components WHERE component_id = ?",
                (str(component_id),),
            ).fetchone()
            assignment_rows = connection.execute(
                "SELECT * FROM assignment_rows WHERE component_id = ? ORDER BY demand_city_id",
                (str(component_id),),
            ).fetchall()
            metric_rows = connection.execute(
                "SELECT * FROM service_metrics WHERE component_id = ? ORDER BY target_hours",
                (str(component_id),),
            ).fetchall()
        metadata = json.loads(component["metadata_json"])
        assignment = AssignmentResult(
            objective=metadata.get("objective", "min_cost"),
            rows=[
                AssignmentRow(
                    demand_city_id=row["demand_city_id"],
                    warehouse_id=row["warehouse_id"],
                    upstream_center_id=row["upstream_center_id"],
                    demand_quantity=Decimal(row["demand_quantity"]),
                    distance_km=row["distance_km"],
                    duration_hours=row["duration_hours"],
                    cost=float(row["cost"]) if row["cost"] is not None else None,
                    reason=row["reason"],
                )
                for row in assignment_rows
            ],
            total_demand=Decimal(metadata.get("total_demand", "0")),
            unassigned_demand=Decimal(metadata.get("unassigned_demand", "0")),
        )
        service = [
            ServiceMetric(
                target_hours=row["target_hours"],
                covered_demand=Decimal(row["covered_demand"]),
                total_demand=Decimal(row["total_demand"]),
                coverage_rate=row["coverage_rate"],
            )
            for row in metric_rows
        ]
        cost = CostSummary.model_validate(metadata["cost"]) if metadata.get("cost") else None
        return component_id, metadata, assignment, service, cost

    def _next_component_revision(
        self, connection: sqlite3.Connection, case_id: UUID, component_kind: str
    ) -> int:
        return int(
            connection.execute(
                "SELECT COALESCE(MAX(component_revision), 0) + 1 FROM case_components "
                "WHERE case_id = ? AND component_kind = ?",
                (str(case_id), component_kind),
            ).fetchone()[0]
        )

    def _insert_component(
        self,
        connection: sqlite3.Connection,
        component_id: UUID,
        case_id: UUID,
        component_kind: str,
        revision: int,
        digest: str,
        row_count: int,
        metadata: dict[str, object],
        timestamp: str,
        depends_on: Sequence[UUID],
    ) -> None:
        connection.execute(
            "INSERT INTO case_components "
            "(component_id, case_id, component_kind, component_revision, content_sha256, "
            "row_count, metadata_json, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            (
                str(component_id),
                str(case_id),
                component_kind,
                revision,
                digest,
                row_count,
                _canonical_json(metadata),
                timestamp,
            ),
        )
        connection.executemany(
            "INSERT INTO case_component_dependencies "
            "(component_id, depends_on_component_id) VALUES (?, ?)",
            [(str(component_id), str(item)) for item in depends_on],
        )

    def _insert_cost_summaries(
        self, connection: sqlite3.Connection, component_id: UUID, cost: object
    ) -> None:
        connection.executemany(
            "INSERT INTO warehouse_cost_summaries "
            "(component_id, warehouse_id, linehaul_cost, last_mile_cost, total_cost, currency) "
            "VALUES (?, ?, ?, ?, ?, ?)",
            [
                (
                    str(component_id),
                    warehouse_id,
                    str(cost.linehaul_by_warehouse.get(warehouse_id, 0)),
                    str(cost.last_mile_by_warehouse.get(warehouse_id, 0)),
                    str(total),
                    cost.currency,
                )
                for warehouse_id, total in cost.by_warehouse.items()
            ],
        )

    def _finish_component_binding(
        self,
        connection: sqlite3.Connection,
        lease: OperationLease,
        facet: FacetName,
        component_id: UUID,
        timestamp: str,
    ) -> None:
        operation = connection.execute(
            "SELECT state FROM case_operations WHERE operation_id = ?",
            (str(lease.operation_id),),
        ).fetchone()
        if operation is None or operation["state"] != OperationState.RUNNING.value:
            raise CaseRepositoryError(
                "operation_not_running", "Case operation is not in the running state."
            )
        previous = connection.execute(
            "SELECT component_id FROM case_bindings WHERE case_id = ? AND facet = ?",
            (str(lease.case_id), facet.value),
        ).fetchone()
        if previous is not None and previous["component_id"] is not None:
            self._invalidate_bound_dependents(
                connection, lease.case_id, UUID(previous["component_id"])
            )
        connection.execute(
            "UPDATE case_bindings SET component_id = ?, state = 'ready', "
            "issue_summary_json = '[]', updated_at = ? WHERE case_id = ? AND facet = ?",
            (str(component_id), timestamp, str(lease.case_id), facet.value),
        )
        connection.execute(
            "UPDATE case_operations SET state = 'completed', result_component_id = ?, "
            "terminal_at = ? WHERE operation_id = ? AND state = 'running'",
            (str(component_id), timestamp, str(lease.operation_id)),
        )
        connection.execute(
            "UPDATE network_cases SET revision = revision + 1, updated_at = ? WHERE case_id = ?",
            (timestamp, str(lease.case_id)),
        )

    def _operation_result(
        self,
        lease: OperationLease,
        component_id: UUID,
        component_kind: str,
        revision: int,
        digest: str,
        row_count: int,
    ) -> CaseOperationResult:
        return CaseOperationResult(
            operation=ToolOperationSummary(
                operation_id=lease.operation_id,
                operation_kind=lease.operation_kind,
                state=OperationState.COMPLETED,
            ),
            components=[
                ComponentSummary(
                    component_id=component_id,
                    component_kind=component_kind,
                    component_revision=revision,
                    content_sha256=digest,
                    row_count=row_count,
                )
            ],
        )

    def _commit_matrix(
        self,
        lease: OperationLease,
        workspace_root: Path,
        *,
        component_kind: str,
        facet: FacetName,
        rows: Sequence[BaseModel],
        metadata: dict[str, object],
        depends_on: Sequence[UUID],
        insert_sql: str,
        row_values,
    ) -> CaseOperationResult:
        if lease.reused_component_id is not None:
            return self.complete_operation(lease, workspace_root, [])
        canonical = _canonical_json(
            {"rows": [row.model_dump(mode="json") for row in rows], "metadata": metadata}
        )
        digest = _sha256(canonical)
        component_id = uuid4()
        timestamp = _now()
        with self._transaction() as connection:
            self._case_row(connection, lease.case_id, workspace_root)
            operation = connection.execute(
                "SELECT state FROM case_operations WHERE operation_id = ?",
                (str(lease.operation_id),),
            ).fetchone()
            if operation is None or operation["state"] != OperationState.RUNNING.value:
                raise CaseRepositoryError(
                    "operation_not_running", "Case operation is not in the running state."
                )
            previous_binding = connection.execute(
                "SELECT component_id FROM case_bindings WHERE case_id = ? AND facet = ?",
                (str(lease.case_id), facet.value),
            ).fetchone()
            revision = connection.execute(
                "SELECT COALESCE(MAX(component_revision), 0) + 1 FROM case_components "
                "WHERE case_id = ? AND component_kind = ?",
                (str(lease.case_id), component_kind),
            ).fetchone()[0]
            connection.execute(
                "INSERT INTO case_components "
                "(component_id, case_id, component_kind, component_revision, content_sha256, "
                "row_count, metadata_json, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                (
                    str(component_id),
                    str(lease.case_id),
                    component_kind,
                    revision,
                    digest,
                    len(rows),
                    _canonical_json(metadata),
                    timestamp,
                ),
            )
            connection.executemany(
                "INSERT INTO case_component_dependencies "
                "(component_id, depends_on_component_id) VALUES (?, ?)",
                [(str(component_id), str(item)) for item in depends_on],
            )
            connection.executemany(insert_sql, [row_values(str(component_id), row) for row in rows])
            if previous_binding is not None and previous_binding["component_id"] is not None:
                self._invalidate_bound_dependents(
                    connection,
                    lease.case_id,
                    UUID(previous_binding["component_id"]),
                )
            connection.execute(
                "UPDATE case_bindings SET component_id = ?, state = 'ready', "
                "issue_summary_json = '[]', updated_at = ? WHERE case_id = ? AND facet = ?",
                (str(component_id), timestamp, str(lease.case_id), facet.value),
            )
            connection.execute(
                "UPDATE case_operations SET state = 'completed', result_component_id = ?, "
                "terminal_at = ? WHERE operation_id = ? AND state = 'running'",
                (str(component_id), timestamp, str(lease.operation_id)),
            )
            connection.execute(
                "UPDATE network_cases SET revision = revision + 1, updated_at = ? "
                "WHERE case_id = ?",
                (timestamp, str(lease.case_id)),
            )
        return CaseOperationResult(
            operation=ToolOperationSummary(
                operation_id=lease.operation_id,
                operation_kind=lease.operation_kind,
                state=OperationState.COMPLETED,
            ),
            components=[
                ComponentSummary(
                    component_id=component_id,
                    component_kind=component_kind,
                    component_revision=revision,
                    content_sha256=digest,
                    row_count=len(rows),
                )
            ],
        )

    @staticmethod
    def _ready_component_id(
        connection: sqlite3.Connection, case_id: UUID, facet: FacetName
    ) -> UUID:
        binding = connection.execute(
            "SELECT component_id, state FROM case_bindings WHERE case_id = ? AND facet = ?",
            (str(case_id), facet.value),
        ).fetchone()
        if (
            binding is None
            or binding["state"] != FacetState.READY.value
            or binding["component_id"] is None
        ):
            raise CaseRepositoryError(
                f"{facet.value}_not_ready", f"Case facet {facet.value} is not ready."
            )
        return UUID(binding["component_id"])

    @staticmethod
    def _source_summary(row: sqlite3.Row) -> SourceSummary:
        return SourceSummary(
            source_id=UUID(row["source_id"]),
            source_ref=row["source_ref"],
            display_name=row["display_name"],
            media_type=row["media_type"],
            content_sha256=row["content_sha256"],
            byte_size=row["byte_size"],
            row_count=row["row_count"],
            source_revision=row["source_revision"],
        )

    def complete_operation(
        self,
        lease: OperationLease,
        workspace_root: Path,
        components: Sequence[ComponentWrite],
    ) -> CaseOperationResult:
        if lease.reused_component_id is not None:
            with self._connect() as connection:
                row = connection.execute(
                    "SELECT * FROM case_components WHERE component_id = ?",
                    (str(lease.reused_component_id),),
                ).fetchone()
            summaries = [self._component_summary(row)] if row is not None else []
            return CaseOperationResult(
                operation=ToolOperationSummary(
                    operation_id=lease.operation_id,
                    operation_kind=lease.operation_kind,
                    state=OperationState.COMPLETED,
                    reused=True,
                ),
                components=summaries,
            )
        timestamp = _now()
        summaries: list[ComponentSummary] = []
        with self._transaction() as connection:
            self._case_row(connection, lease.case_id, workspace_root)
            operation = connection.execute(
                "SELECT state FROM case_operations WHERE operation_id = ?",
                (str(lease.operation_id),),
            ).fetchone()
            if operation is None or operation["state"] != OperationState.RUNNING.value:
                raise CaseRepositoryError(
                    "operation_not_running", "Case operation is not in the running state."
                )
            for component in components:
                previous_binding = connection.execute(
                    "SELECT component_id FROM case_bindings WHERE case_id = ? AND facet = ?",
                    (str(lease.case_id), component.facet.value),
                ).fetchone()
                existing = connection.execute(
                    "SELECT * FROM case_components WHERE case_id = ? AND component_kind = ? "
                    "AND content_sha256 = ?",
                    (str(lease.case_id), component.component_kind, component.content_sha256),
                ).fetchone()
                if existing is None:
                    revision = connection.execute(
                        "SELECT COALESCE(MAX(component_revision), 0) + 1 FROM case_components "
                        "WHERE case_id = ? AND component_kind = ?",
                        (str(lease.case_id), component.component_kind),
                    ).fetchone()[0]
                    component_id = uuid4()
                    connection.execute(
                        "INSERT INTO case_components "
                        "(component_id, case_id, component_kind, component_revision, "
                        "content_sha256, row_count, metadata_json, created_at) "
                        "VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                        (
                            str(component_id),
                            str(lease.case_id),
                            component.component_kind,
                            revision,
                            component.content_sha256,
                            component.row_count,
                            component.metadata_json,
                            timestamp,
                        ),
                    )
                    for dependency in component.depends_on:
                        connection.execute(
                            "INSERT INTO case_component_dependencies "
                            "(component_id, depends_on_component_id) VALUES (?, ?)",
                            (str(component_id), str(dependency)),
                        )
                    existing = connection.execute(
                        "SELECT * FROM case_components WHERE component_id = ?",
                        (str(component_id),),
                    ).fetchone()
                component_id = existing["component_id"]
                if (
                    previous_binding is not None
                    and previous_binding["component_id"] is not None
                    and previous_binding["component_id"] != component_id
                ):
                    self._invalidate_bound_dependents(
                        connection,
                        lease.case_id,
                        UUID(previous_binding["component_id"]),
                    )
                connection.execute(
                    "UPDATE case_bindings SET component_id = ?, state = 'ready', "
                    "issue_summary_json = '[]', updated_at = ? WHERE case_id = ? AND facet = ?",
                    (component_id, timestamp, str(lease.case_id), component.facet.value),
                )
                summaries.append(self._component_summary(existing))
            result_component_id = str(summaries[-1].component_id) if summaries else None
            updated = connection.execute(
                "UPDATE case_operations SET state = 'completed', result_component_id = ?, "
                "terminal_at = ? WHERE operation_id = ? AND state = 'running'",
                (result_component_id, timestamp, str(lease.operation_id)),
            ).rowcount
            if updated != 1:
                raise CaseRepositoryError(
                    "operation_terminal_conflict",
                    "Case operation already reached a terminal state.",
                )
            connection.execute(
                "UPDATE network_cases SET revision = revision + 1, updated_at = ?, "
                "last_accessed_at = ? WHERE case_id = ?",
                (timestamp, timestamp, str(lease.case_id)),
            )
        return CaseOperationResult(
            operation=ToolOperationSummary(
                operation_id=lease.operation_id,
                operation_kind=lease.operation_kind,
                state=OperationState.COMPLETED,
            ),
            components=summaries,
        )

    @staticmethod
    def _component_summary(row: sqlite3.Row) -> ComponentSummary:
        return ComponentSummary(
            component_id=UUID(row["component_id"]),
            component_kind=row["component_kind"],
            component_revision=row["component_revision"],
            content_sha256=row["content_sha256"],
            row_count=row["row_count"],
        )

    def fail_operation(
        self,
        lease: OperationLease,
        workspace_root: Path,
        code: str,
        safe_message: str,
        state: OperationState = OperationState.FAILED,
    ) -> ToolOperationSummary:
        if state not in {
            OperationState.FAILED,
            OperationState.INTERRUPTED,
            OperationState.TIMEOUT,
        }:
            raise ValueError("fail_operation requires a failure terminal state")
        with self._transaction() as connection:
            self._case_row(connection, lease.case_id, workspace_root)
            updated = connection.execute(
                "UPDATE case_operations SET state = ?, error_code = ?, safe_error_message = ?, "
                "terminal_at = ? WHERE operation_id = ? AND state = 'running'",
                (state.value, code[:128], safe_message[:1_000], _now(), str(lease.operation_id)),
            ).rowcount
            if updated != 1:
                raise CaseRepositoryError(
                    "operation_terminal_conflict",
                    "Case operation already reached a terminal state.",
                )
        return ToolOperationSummary(
            operation_id=lease.operation_id,
            operation_kind=lease.operation_kind,
            state=state,
        )

    def _invalidate_bound_dependents(
        self, connection: sqlite3.Connection, case_id: UUID, changed_component_id: UUID
    ) -> None:
        rows = connection.execute(
            "WITH RECURSIVE dependents(component_id) AS ("
            "  SELECT component_id FROM case_component_dependencies "
            "  WHERE depends_on_component_id = ? "
            "  UNION "
            "  SELECT edge.component_id FROM case_component_dependencies edge "
            "  JOIN dependents parent ON parent.component_id = edge.depends_on_component_id"
            ") SELECT component_id FROM dependents",
            (str(changed_component_id),),
        ).fetchall()
        dependent_ids = [row["component_id"] for row in rows]
        if not dependent_ids:
            return
        placeholders = ",".join("?" for _ in dependent_ids)
        connection.execute(
            f"UPDATE case_bindings SET state = 'stale', updated_at = ? "
            f"WHERE case_id = ? AND component_id IN ({placeholders})",
            (_now(), str(case_id), *dependent_ids),
        )

    def archive_case(self, case_id: UUID, workspace_root: Path) -> None:
        with self._transaction() as connection:
            self._case_row(connection, case_id, workspace_root)
            running = connection.execute(
                "SELECT COUNT(*) FROM case_operations WHERE case_id = ? AND state = 'running'",
                (str(case_id),),
            ).fetchone()[0]
            if running:
                raise CaseRepositoryError(
                    "case_has_running_operations",
                    "A Case with running operations cannot be archived.",
                )
            timestamp = _now()
            connection.execute(
                "UPDATE network_cases SET state = 'archived', archived_at = ?, updated_at = ? "
                "WHERE case_id = ?",
                (timestamp, timestamp, str(case_id)),
            )

    def storage_status(self, high_water_bytes: int) -> StorageStatus:
        database_bytes = self.database_path.stat().st_size if self.database_path.exists() else 0
        wal_path = Path(f"{self.database_path}-wal")
        wal_bytes = wal_path.stat().st_size if wal_path.exists() else 0
        with self._connect() as connection:
            archived = connection.execute(
                "SELECT COUNT(*) FROM network_cases WHERE state = 'archived'"
            ).fetchone()[0]
        return StorageStatus(
            database_bytes=database_bytes,
            wal_bytes=wal_bytes,
            archived_case_count=archived,
            over_high_water=database_bytes + wal_bytes > high_water_bytes,
        )

    def purge_archived(self, retention_days: int) -> PurgeResult:
        cutoff = (datetime.now(UTC) - timedelta(days=retention_days)).isoformat()
        before = self.database_path.stat().st_size if self.database_path.exists() else 0
        with self._transaction() as connection:
            rows = connection.execute(
                "SELECT case_id FROM network_cases WHERE state = 'archived' "
                "AND archived_at < ? AND NOT EXISTS ("
                "  SELECT 1 FROM case_operations operation "
                "  WHERE operation.case_id = network_cases.case_id AND operation.state = 'running'"
                ")",
                (cutoff,),
            ).fetchall()
            connection.executemany(
                "DELETE FROM network_cases WHERE case_id = ?",
                [(row["case_id"],) for row in rows],
            )
        after = self.database_path.stat().st_size if self.database_path.exists() else 0
        return PurgeResult(
            purged_cases=len(rows),
            reclaimed_candidate_bytes=max(0, before - after),
        )
