"""Workspace-scoped credential persistence for the maps MCP server."""

from __future__ import annotations

import json
import os
import tempfile
from pathlib import Path
from typing import Final

SUPPORTED_PROVIDERS: Final = frozenset({"google", "mapbox"})


class WorkspaceCredentialStore:
    """Store API keys in an ignored, workspace-local file with owner-only permissions."""

    def __init__(self, workspace_root: Path) -> None:
        self.workspace_root = workspace_root.expanduser().resolve()
        self.memory_dir = self.workspace_root / ".codex"
        self.memory_path = self.memory_dir / "maps-tool-memory.json"

    def get_api_key(self, provider: str) -> str | None:
        provider = _validate_provider(provider)
        data = self._read()
        value = data.get("providers", {}).get(provider, {}).get("api_key")
        if not isinstance(value, str) or not value.strip():
            return None
        return value.strip()

    def set_api_key(self, provider: str, api_key: str) -> None:
        provider = _validate_provider(provider)
        api_key = api_key.strip()
        if not api_key:
            raise ValueError("API key must not be empty")

        data = self._read()
        providers = data.setdefault("providers", {})
        providers[provider] = {"api_key": api_key}

        self.memory_dir.mkdir(mode=0o700, parents=True, exist_ok=True)
        os.chmod(self.memory_dir, 0o700)
        fd, temporary_name = tempfile.mkstemp(
            prefix="maps-tool-memory.", suffix=".tmp", dir=self.memory_dir
        )
        try:
            with os.fdopen(fd, "w", encoding="utf-8") as handle:
                json.dump(data, handle, ensure_ascii=False, indent=2, sort_keys=True)
                handle.write("\n")
                handle.flush()
                os.fsync(handle.fileno())
            os.chmod(temporary_name, 0o600)
            os.replace(temporary_name, self.memory_path)
            os.chmod(self.memory_path, 0o600)
        finally:
            if os.path.exists(temporary_name):
                os.unlink(temporary_name)

    def _read(self) -> dict[str, object]:
        if not self.memory_path.exists():
            return {"version": 1, "providers": {}}
        if self.memory_path.is_symlink():
            raise RuntimeError(f"Refusing to read symlinked credential memory: {self.memory_path}")
        try:
            with self.memory_path.open(encoding="utf-8") as handle:
                data = json.load(handle)
        except (OSError, json.JSONDecodeError) as exc:
            raise RuntimeError(f"Unable to read maps credential memory: {exc}") from exc
        if not isinstance(data, dict) or data.get("version") != 1:
            raise RuntimeError("Unsupported maps credential memory format")
        if not isinstance(data.get("providers"), dict):
            raise RuntimeError("Invalid maps credential memory providers object")
        return data


def _validate_provider(provider: str) -> str:
    normalized = provider.strip().lower()
    if normalized not in SUPPORTED_PROVIDERS:
        raise ValueError(f"Unsupported maps provider: {provider}")
    return normalized
