"""Case-owned mapping proposal and selection workflow."""

from __future__ import annotations

import json
from pathlib import Path
from uuid import UUID

from .case_repository import CaseRepository
from .case_types import CaseOperationResult, ComponentSummary, StoredMappingCandidate
from .mapping import MappingEngine, MappingProposal
from .network_data import SourceInventoryService


class CaseMappingService:
    def __init__(self, repository: CaseRepository):
        self.repository = repository

    def propose(
        self, case_id: UUID, workspace_root: Path
    ) -> tuple[MappingProposal, CaseOperationResult]:
        sources = self.repository.list_sources(case_id, workspace_root)
        inspections = SourceInventoryService(self.repository).inspect(
            case_id, workspace_root, [source.source_id for source in sources]
        )
        proposal = MappingEngine().propose(inspections)
        lease = self.repository.begin_operation(
            case_id,
            workspace_root,
            "propose_case_mapping",
            {"sources": [source.model_dump(mode="json") for source in sources]},
        )
        candidates = [
            StoredMappingCandidate(
                candidate_id=candidate.candidate_id,
                source_id=UUID(candidate.source_id),
                source_role=candidate.source_role.value,
                target_entity=candidate.target_entity,
                target_field=candidate.target_field,
                source_field=candidate.source_field,
                transform_json=json.dumps(
                    candidate.transform.model_dump(mode="json"),
                    sort_keys=True,
                    separators=(",", ":"),
                ),
                score=candidate.score,
                reason_code=candidate.reason_code,
            )
            for role in proposal.proposals
            for candidate in role.field_candidates
        ]
        result = self.repository.commit_mapping_proposal(
            lease,
            workspace_root,
            candidates,
            needs_input=any(item.ambiguous for item in proposal.proposals),
        )
        return proposal, result

    def apply(
        self,
        case_id: UUID,
        workspace_root: Path,
        candidate_ids: list[str],
    ) -> ComponentSummary:
        return self.repository.select_mapping_candidates(
            case_id, workspace_root, candidate_ids
        )
