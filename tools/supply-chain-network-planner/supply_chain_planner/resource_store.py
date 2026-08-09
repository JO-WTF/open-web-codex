"""Content-addressed storage for MCP planning resources."""

from __future__ import annotations

import hashlib
import json
import os
import re
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from pydantic import BaseModel

from .models import MCP_SERVER_NAME, ResourceRef

RESOURCE_ID_PATTERN = re.compile(r"^[a-z0-9_.-]{1,160}$")
RESOURCE_URI_PREFIX = "supply-chain://resources/"


def workspace_resource_root(profile_home: Path, startup_workspace: Path) -> Path:
    """Return a private, opaque Resource namespace for one physical Workspace."""
    canonical_workspace = startup_workspace.resolve(strict=True)
    namespace = hashlib.sha256(os.fsencode(str(canonical_workspace))).hexdigest()
    return (
        profile_home
        / ".open-web-codex"
        / "mcp-state"
        / "supply-chain"
        / "workspaces"
        / namespace
        / "resources"
    )


@dataclass(frozen=True)
class PublishedResource:
    resource_id: str
    uri: str
    schema: str
    size: int


class ResourceStore:
    def __init__(self, root: Path, *, uri_prefix: str = RESOURCE_URI_PREFIX):
        self.root = root.resolve()
        self.uri_prefix = uri_prefix
        self.root.mkdir(parents=True, exist_ok=True)

    def publish(self, schema: str, value: BaseModel | dict[str, Any]) -> PublishedResource:
        payload = value.model_dump(mode="json") if isinstance(value, BaseModel) else value
        encoded = json.dumps(
            payload,
            ensure_ascii=False,
            sort_keys=True,
            separators=(",", ":"),
        ).encode("utf-8")
        digest = hashlib.sha256(encoded).hexdigest()[:24]
        safe_schema = re.sub(r"[^a-z0-9_.-]", "-", schema.lower())
        resource_id = f"{safe_schema}-{digest}"
        path = self._path(resource_id)
        if not path.exists():
            descriptor, temporary_name = tempfile.mkstemp(
                prefix=f".{resource_id}.",
                suffix=".tmp",
                dir=self.root,
            )
            temporary = Path(temporary_name)
            try:
                with os.fdopen(descriptor, "wb") as stream:
                    stream.write(encoded)
                    stream.flush()
                    os.fsync(stream.fileno())
                os.replace(temporary, path)
            finally:
                temporary.unlink(missing_ok=True)
        return PublishedResource(
            resource_id=resource_id,
            uri=f"{self.uri_prefix}{resource_id}",
            schema=schema,
            size=len(encoded),
        )

    def read(self, resource_id: str) -> str:
        return self._path(resource_id).read_text(encoding="utf-8")

    def load(self, resource_ref: ResourceRef) -> dict[str, Any]:
        ref = resource_ref
        resource_id = ref.uri.removeprefix(self.uri_prefix)
        actual_schema = resource_id.rsplit("-", maxsplit=1)[0]
        expected_schema = re.sub(r"[^a-z0-9_.-]", "-", ref.resource_schema.lower())
        if actual_schema != expected_schema:
            raise ValueError("resource_ref schema does not match the published Resource")
        return self.load_uri(ref.uri)

    def load_uri(self, uri: str) -> dict[str, Any]:
        if not uri.startswith(self.uri_prefix):
            raise ValueError(f"resource URI must start with {self.uri_prefix!r}")
        resource_id = uri.removeprefix(self.uri_prefix)
        payload = json.loads(self.read(resource_id))
        if not isinstance(payload, dict):
            raise ValueError(f"resource {resource_id!r} does not contain a JSON object")
        return payload

    def _path(self, resource_id: str) -> Path:
        if not RESOURCE_ID_PATTERN.fullmatch(resource_id):
            raise ValueError("invalid supply-chain resource identifier")
        path = (self.root / f"{resource_id}.json").resolve()
        if path.parent != self.root:
            raise ValueError("resource path escapes the configured resource directory")
        return path


def resource_ref(published: PublishedResource) -> ResourceRef:
    return ResourceRef(
        server=MCP_SERVER_NAME,
        uri=published.uri,
        resource_schema=published.schema,
    )
