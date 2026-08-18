"""Persistent GeoJSON resources addressed by MCP Resource URIs."""

from __future__ import annotations

import json
import os
import re
import tempfile
from dataclasses import dataclass
from hashlib import sha256
from pathlib import Path

_RESOURCE_ID = re.compile(r"^[A-Za-z0-9_.-]{1,128}$")


@dataclass(frozen=True)
class PublishedGeoJson:
    resource_id: str
    uri: str
    size: int


class GeoJsonResourceStore:
    def __init__(self, state_root: Path) -> None:
        self.root = state_root.resolve() / ".codex" / "maps-data"

    def publish(self, geojson: dict[str, object]) -> PublishedGeoJson:
        payload = json.dumps(
            geojson,
            ensure_ascii=False,
            separators=(",", ":"),
        ).encode("utf-8")
        resource_id = f"map-data-{sha256(payload).hexdigest()}"
        target = _publish_immutable(self.root, f"{resource_id}.geojson", payload)
        return PublishedGeoJson(
            resource_id=resource_id,
            uri=f"maps-data://geojson/{resource_id}",
            size=target.stat().st_size,
        )

    def read(self, resource_id: str) -> str:
        if not _RESOURCE_ID.fullmatch(resource_id):
            raise ValueError("GeoJSON Resource identifier is invalid")
        path = self.root / f"{resource_id}.geojson"
        return path.read_text(encoding="utf-8")


@dataclass(frozen=True)
class PublishedMapCardSpec:
    resource_id: str
    uri: str
    size: int


class MapCardSpecStore:
    """Provider-owned immutable presentation specs, separate from GeoJSON."""

    def __init__(self, state_root: Path) -> None:
        self.root = state_root.resolve() / ".codex" / "map-card-specs"

    def publish(self, value: dict[str, object]) -> PublishedMapCardSpec:
        payload = json.dumps(value, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
        resource_id = f"map-card-spec-{sha256(payload).hexdigest()}"
        target = _publish_immutable(self.root, f"{resource_id}.json", payload)
        return PublishedMapCardSpec(
            resource_id=resource_id,
            uri=f"maps-data://map-card-spec/{resource_id}",
            size=target.stat().st_size,
        )

    def read(self, resource_id: str) -> str:
        if not _RESOURCE_ID.fullmatch(resource_id):
            raise ValueError("Map card spec Resource identifier is invalid")
        path = self.root / f"{resource_id}.json"
        return path.read_text(encoding="utf-8")


def _publish_immutable(root: Path, file_name: str, payload: bytes) -> Path:
    """Create or reuse one content-addressed provider Resource without overwriting it."""

    root.mkdir(parents=True, exist_ok=True, mode=0o700)
    os.chmod(root, 0o700)
    target = root / file_name
    if target.exists():
        if target.read_bytes() != payload:
            raise ValueError("content_address_collision")
        return target
    with tempfile.NamedTemporaryFile(dir=root, delete=False) as temporary:
        temporary.write(payload)
        temporary.flush()
        os.fsync(temporary.fileno())
        temporary_path = Path(temporary.name)
    try:
        os.chmod(temporary_path, 0o600)
        try:
            os.link(temporary_path, target)
        except FileExistsError:
            if target.read_bytes() != payload:
                raise ValueError("content_address_collision")
    finally:
        temporary_path.unlink(missing_ok=True)
    return target
