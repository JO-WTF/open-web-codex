"""Content-addressed storage for bounded provider-owned MCP Resources."""

from __future__ import annotations

import hashlib
import os
import re
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any
from urllib.parse import urlparse

from pydantic import BaseModel

from .codec import MAX_RESOURCE_BYTES, canonical_json_bytes, decode_json_object
from .contracts import ResourceRef
from .errors import ProviderContractError

RESOURCE_ID_PATTERN = re.compile(r"^[a-z0-9_.-]{1,160}$")
PROVIDER_NAMESPACE_PATTERN = re.compile(r"^[a-z][a-z0-9_.-]{0,127}$")


def workspace_resource_root(
    profile_home: Path,
    startup_workspace: Path,
    provider_namespace: str,
) -> Path:
    """Return a private opaque namespace for one physical Workspace."""

    try:
        canonical_workspace = startup_workspace.resolve(strict=True)
        canonical_profile = profile_home.resolve()
    except (OSError, RuntimeError) as error:
        raise ProviderContractError("resource_scope_invalid") from error
    if not canonical_workspace.is_dir():
        raise ProviderContractError("resource_scope_invalid")
    if not PROVIDER_NAMESPACE_PATTERN.fullmatch(provider_namespace):
        raise ProviderContractError("provider_namespace_invalid")
    namespace = hashlib.sha256(os.fsencode(str(canonical_workspace))).hexdigest()
    return (
        canonical_profile
        / ".open-web-codex"
        / "mcp-state"
        / provider_namespace
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
    def __init__(self, root: Path, *, uri_prefix: str):
        parsed = urlparse(uri_prefix)
        if (
            not isinstance(uri_prefix, str)
            or len(uri_prefix) > 1024
            or not uri_prefix.endswith("/")
            or not parsed.scheme
        ):
            raise ProviderContractError("resource_uri_prefix_invalid")
        self.uri_prefix = uri_prefix
        try:
            root.mkdir(parents=True, exist_ok=True)
            self.root = root.resolve(strict=True)
        except (OSError, RuntimeError) as error:
            raise ProviderContractError("resource_store_invalid") from error
        if not self.root.is_dir():
            raise ProviderContractError("resource_store_invalid")

    def publish(
        self,
        schema: str,
        value: BaseModel | dict[str, Any],
        *,
        max_bytes: int = MAX_RESOURCE_BYTES,
    ) -> PublishedResource:
        safe_schema = _safe_schema(schema)
        encoded = canonical_json_bytes(value, max_bytes=max_bytes)
        digest = hashlib.sha256(encoded).hexdigest()[:24]
        resource_id = f"{safe_schema}-{digest}"
        path = self._path(resource_id)
        if not path.exists():
            descriptor, temporary_name = tempfile.mkstemp(
                prefix=f".{resource_id}.", suffix=".tmp", dir=self.root
            )
            temporary = Path(temporary_name)
            try:
                with os.fdopen(descriptor, "wb") as stream:
                    stream.write(encoded)
                    stream.flush()
                    os.fsync(stream.fileno())
                os.replace(temporary, path)
            except OSError as error:
                raise ProviderContractError("resource_publish_invalid") from error
            finally:
                temporary.unlink(missing_ok=True)
        return PublishedResource(
            resource_id=resource_id,
            uri=f"{self.uri_prefix}{resource_id}",
            schema=schema,
            size=len(encoded),
        )

    def read_bytes(
        self, resource_id: str, *, max_bytes: int = MAX_RESOURCE_BYTES
    ) -> bytes:
        path = self._path(resource_id)
        try:
            if path.stat().st_size > max_bytes:
                raise ProviderContractError("resource_payload_too_large")
            content = path.read_bytes()
        except ProviderContractError:
            raise
        except OSError as error:
            raise ProviderContractError("resource_read_invalid") from error
        if len(content) > max_bytes:
            raise ProviderContractError("resource_payload_too_large")
        return content

    def read(self, resource_id: str, *, max_bytes: int = MAX_RESOURCE_BYTES) -> str:
        try:
            return self.read_bytes(resource_id, max_bytes=max_bytes).decode("utf-8")
        except UnicodeError as error:
            raise ProviderContractError("resource_payload_invalid") from error

    def load(
        self,
        resource_ref: ResourceRef,
        *,
        max_bytes: int = MAX_RESOURCE_BYTES,
    ) -> dict[str, Any]:
        if not resource_ref.uri.startswith(self.uri_prefix):
            raise ProviderContractError("resource_uri_mismatch")
        resource_id = resource_ref.uri.removeprefix(self.uri_prefix)
        actual_schema = resource_id.rsplit("-", maxsplit=1)[0]
        if actual_schema != _safe_schema(resource_ref.resource_schema):
            raise ProviderContractError("resource_schema_mismatch")
        return decode_json_object(self.read_bytes(resource_id, max_bytes=max_bytes), max_bytes=max_bytes)

    def load_uri(self, uri: str, *, max_bytes: int = MAX_RESOURCE_BYTES) -> dict[str, Any]:
        if not isinstance(uri, str) or not uri.startswith(self.uri_prefix):
            raise ProviderContractError("resource_uri_mismatch")
        resource_id = uri.removeprefix(self.uri_prefix)
        return decode_json_object(self.read_bytes(resource_id, max_bytes=max_bytes), max_bytes=max_bytes)

    def _path(self, resource_id: str) -> Path:
        if not RESOURCE_ID_PATTERN.fullmatch(resource_id):
            raise ProviderContractError("resource_id_invalid")
        path = self.root / f"{resource_id}.json"
        if path.parent != self.root:
            raise ProviderContractError("resource_path_invalid")
        return path


def resource_ref(published: PublishedResource, server_name: str) -> ResourceRef:
    return ResourceRef(
        server=server_name,
        uri=published.uri,
        resource_schema=published.schema,
    )


def _safe_schema(schema: str) -> str:
    if not isinstance(schema, str) or not re.fullmatch(r"[a-z][a-z0-9_.-]{0,127}", schema):
        raise ProviderContractError("resource_schema_invalid")
    return schema.lower()
