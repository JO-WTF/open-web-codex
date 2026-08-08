"""Case-owned Workspace source discovery and bounded structural inspection."""

from __future__ import annotations

import json
from pathlib import Path
from uuid import UUID

from pydantic import BaseModel, ConfigDict, Field

from .case_repository import CaseRepository, CaseRepositoryError
from .case_types import SourceSnapshot, SourceSummary
from .workspace_intake import discover, inspect


class DataServiceContract(BaseModel):
    model_config = ConfigDict(extra="forbid")


class FieldInspection(DataServiceContract):
    field_name: str = Field(min_length=1, max_length=256)
    inferred_type: str = Field(min_length=1, max_length=64)
    sample_values: list[str] = Field(default_factory=list, max_length=3)


class SourceInspection(DataServiceContract):
    source: SourceSummary
    structure_kind: str
    record_count: int | None = Field(default=None, ge=0)
    fields: list[FieldInspection] = Field(default_factory=list, max_length=256)


class SourceInventoryService:
    def __init__(self, repository: CaseRepository):
        self.repository = repository

    def refresh(
        self, case_id: UUID, workspace_root: Path
    ) -> tuple[list[SourceSummary], bool]:
        discovered = discover(workspace_root)
        snapshots = [
            SourceSnapshot(
                source_ref=item["source_ref"],
                display_name=item["display_name"],
                media_type=item["media_type"],
                content_sha256=item["content_sha256"],
                byte_size=item["byte_size"],
                metadata_json=json.dumps(
                    {"extension": item["extension"]}, sort_keys=True, separators=(",", ":")
                ),
            )
            for item in discovered
        ]
        return self.repository.refresh_sources(case_id, workspace_root, snapshots)

    def inspect(
        self, case_id: UUID, workspace_root: Path, source_ids: list[UUID]
    ) -> list[SourceInspection]:
        sources = {source.source_id: source for source in self.repository.list_sources(case_id, workspace_root)}
        if not source_ids or len(source_ids) > 100:
            raise CaseRepositoryError(
                "source_selection_invalid", "Select between one and one hundred active sources."
            )
        unknown = [source_id for source_id in source_ids if source_id not in sources]
        if unknown:
            raise CaseRepositoryError(
                "source_not_active", "One or more selected sources are not active in this Case."
            )
        return [self._inspect_source(workspace_root, sources[source_id]) for source_id in source_ids]

    @staticmethod
    def _inspect_source(workspace_root: Path, source: SourceSummary) -> SourceInspection:
        payload = inspect(workspace_root, source.source_ref)
        structure = payload.get("structure") if isinstance(payload, dict) else None
        if not isinstance(structure, dict):
            raise CaseRepositoryError("source_structure_invalid", "Source structure is unavailable.")
        columns = structure.get("columns")
        preview = structure.get("preview")
        rows = preview.get("rows") if isinstance(preview, dict) else []
        fields: list[FieldInspection] = []
        if isinstance(columns, list):
            for index, column in enumerate(columns[:256]):
                name = str(column.get("name") if isinstance(column, dict) else column).strip()
                if not name:
                    continue
                samples: list[str] = []
                if isinstance(rows, list):
                    for row in rows[:3]:
                        value = None
                        if isinstance(row, dict):
                            value = row.get(name)
                        elif isinstance(row, list) and index < len(row):
                            value = row[index]
                        if value not in (None, ""):
                            samples.append(str(value)[:64])
                inferred = "unknown"
                if isinstance(column, dict):
                    inferred = str(column.get("type") or column.get("inferred_type") or "unknown")
                fields.append(
                    FieldInspection(
                        field_name=name,
                        inferred_type=inferred[:64],
                        sample_values=samples[:3],
                    )
                )
        count = structure.get("record_count")
        return SourceInspection(
            source=source,
            structure_kind=str(structure.get("kind") or "unknown")[:64],
            record_count=count if isinstance(count, int) and count >= 0 else None,
            fields=fields,
        )
