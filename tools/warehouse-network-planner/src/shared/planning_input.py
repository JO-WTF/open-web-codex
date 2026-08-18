"""Workspace-backed planning-input contract shared by Data and Network tools."""

from __future__ import annotations

from pathlib import Path

from supply_chain_planner.data.workspace_intake import read_json_document_with_sha256
from supply_chain_planner.network.models import PlanningInputIdentity
from supply_chain_planner.shared.models import PreparedNetworkResource


def load_prepared_network_input(
    workspace_root: Path,
    prepared_input_relative_path: str,
) -> tuple[PreparedNetworkResource, PlanningInputIdentity]:
    """Load one exact Tool-created planning input from the authorized Workspace."""
    payload, content_sha256 = read_json_document_with_sha256(
        workspace_root,
        prepared_input_relative_path,
    )
    try:
        prepared = PreparedNetworkResource.model_validate(payload)
    except ValueError as error:
        raise ValueError("prepared_network_input_invalid") from error
    return prepared, PlanningInputIdentity(content_sha256=content_sha256)


def require_matching_input(
    expected: PlanningInputIdentity,
    actual: PlanningInputIdentity,
) -> None:
    """Reject a valid-looking matrix or result derived from another input."""
    if expected != actual:
        raise ValueError("planning_input_identity_mismatch")
