"""Native Turn Workspace scope validation shared by MCP providers."""

from __future__ import annotations

from pathlib import Path
from typing import Any
from urllib.parse import unquote, urlparse

SANDBOX_META = "codex/sandbox-state-meta"


def trusted_workspace_root(meta: Any) -> Path:
    extra = getattr(meta, "model_extra", None)
    state = extra.get(SANDBOX_META) if isinstance(extra, dict) else None
    sandbox_cwd = state.get("sandboxCwd") if isinstance(state, dict) else None
    if not isinstance(sandbox_cwd, str):
        raise ValueError("trusted Turn Workspace metadata is unavailable")
    parsed = urlparse(sandbox_cwd)
    if parsed.scheme != "file" or parsed.netloc not in ("", "localhost"):
        raise ValueError("trusted Turn Workspace is not a local file URI")
    root = Path(unquote(parsed.path)).resolve(strict=True)
    if not root.is_dir():
        raise ValueError("trusted Turn Workspace is not a directory")
    return root
