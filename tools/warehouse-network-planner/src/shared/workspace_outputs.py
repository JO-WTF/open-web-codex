"""Workspace-relative output locations owned by the warehouse-network Tool package."""

from __future__ import annotations

from enum import StrEnum
from pathlib import Path, PurePosixPath

from open_web_codex_provider import ProviderContractError, ensure_workspace_directory


class WorkspaceOutputKind(StrEnum):
    PREPARED_INPUT = "prepared"
    NAVIGATION_REQUEST = "requests"
    DELIVERY_MARKDOWN = "deliverables-markdown"


_OUTPUT_ROOT = PurePosixPath("outputs/warehouse-network")
_OUTPUT_CONTRACTS = {
    WorkspaceOutputKind.PREPARED_INPUT: (_OUTPUT_ROOT / "prepared", ".json"),
    WorkspaceOutputKind.NAVIGATION_REQUEST: (_OUTPUT_ROOT / "requests", ".json"),
    WorkspaceOutputKind.DELIVERY_MARKDOWN: (_OUTPUT_ROOT / "deliverables", ".md"),
}


def prepare_workspace_output_path(
    workspace_root: Path,
    value: str,
    kind: WorkspaceOutputKind,
) -> str:
    """Prepare one direct create-new file under the package-owned output directory."""

    expected_parent, expected_suffix = _OUTPUT_CONTRACTS[kind]
    candidate = PurePosixPath(value)
    if (
        candidate.is_absolute()
        or candidate.parent != expected_parent
        or candidate.suffix.lower() != expected_suffix
        or candidate.name in {"", ".", ".."}
    ):
        raise ProviderContractError("generated_output_path_invalid")
    ensure_workspace_directory(workspace_root, expected_parent.as_posix())
    return candidate.as_posix()
