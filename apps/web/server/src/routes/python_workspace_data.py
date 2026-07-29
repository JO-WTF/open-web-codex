"""Verified access to one exact Workspace Dataset Release."""

from __future__ import annotations

import hashlib
import json
import os
import re
from dataclasses import dataclass
from pathlib import Path
from typing import BinaryIO
from uuid import UUID

_SLUG = re.compile(r"^[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?$")
_VERSION = re.compile(r"^[A-Za-z0-9](?:[A-Za-z0-9._-]{0,62}[A-Za-z0-9])?$")
_SHA256 = re.compile(r"^[0-9a-f]{64}$")
_MAX_MANIFEST_BYTES = 256 * 1024
_MAX_FILES = 32
_MAX_FILE_BYTES = 32 * 1024 * 1024


@dataclass(frozen=True)
class DatasetFile:
    logical_name: str
    role: str
    media_type: str
    byte_size: int
    content_sha256: str
    path: Path

    def open_binary(self) -> BinaryIO:
        return self.path.open("rb")


@dataclass(frozen=True)
class DatasetRelease:
    workspace_id: str
    release_id: str
    dataset_id: str
    version: str
    content_sha256: str
    files: dict[str, DatasetFile]

    def require_file(self, logical_name: str) -> DatasetFile:
        try:
            return self.files[logical_name]
        except KeyError as error:
            raise ValueError(f"Dataset file is not declared: {logical_name}") from error


def load_dataset_release(
    *,
    workspace_id: str,
    release_id: str,
    dataset_id: str,
    version: str,
    content_sha256: str,
) -> DatasetRelease:
    """Load and verify an exact platform-authorized Dataset Release."""

    workspace_id = str(UUID(workspace_id))
    release_id = str(UUID(release_id))
    if not _SLUG.fullmatch(dataset_id):
        raise ValueError("Dataset ID is invalid")
    if not _VERSION.fullmatch(version):
        raise ValueError("Dataset version is invalid")
    if not _SHA256.fullmatch(content_sha256):
        raise ValueError("Dataset content SHA-256 is invalid")

    workspace_root, authorized_workspace_id = _authorized_workspace()
    if workspace_id != authorized_workspace_id:
        raise ValueError("Workspace identity does not match the authorized capability package")
    release_root = workspace_root / "datasets" / dataset_id / version
    _reject_symlink_chain(workspace_root, release_root)

    manifest_path = release_root / "release.json"
    _reject_symlink(manifest_path)
    try:
        manifest_bytes = manifest_path.read_bytes()
    except OSError as error:
        raise ValueError("Dataset Release manifest is unavailable") from error
    if not manifest_bytes or len(manifest_bytes) > _MAX_MANIFEST_BYTES:
        raise ValueError("Dataset Release manifest size is invalid")
    manifest = json.loads(manifest_bytes)
    if (
        manifest.get("schemaVersion") != "workspace.dataset-release.v1"
        or manifest.get("releaseId") != release_id
        or manifest.get("datasetId") != dataset_id
        or manifest.get("version") != version
        or manifest.get("contentSha256") != content_sha256
    ):
        raise ValueError("Dataset Release identity does not match the authorized binding")

    declared_files = manifest.get("files")
    if not isinstance(declared_files, list) or not 1 <= len(declared_files) <= _MAX_FILES:
        raise ValueError("Dataset Release file declaration is invalid")

    files: dict[str, DatasetFile] = {}
    release_digest = hashlib.sha256()
    for value in (
        "workspace.dataset-release.v1",
        dataset_id,
        version,
        _required_text(manifest, "displayName"),
        _required_text(manifest, "description"),
    ):
        _update_digest(release_digest, value)

    for descriptor in sorted(declared_files, key=lambda item: item.get("logicalName", "")):
        logical_name = _required_text(descriptor, "logicalName")
        role = _required_text(descriptor, "role")
        media_type = _required_text(descriptor, "mediaType")
        relative_path = _required_text(descriptor, "relativePath")
        file_sha256 = _required_text(descriptor, "contentSha256")
        byte_size = descriptor.get("byteSize")
        if (
            logical_name in files
            or relative_path != f"files/{logical_name}"
            or not _safe_logical_name(logical_name)
            or not isinstance(byte_size, int)
            or not 0 < byte_size <= _MAX_FILE_BYTES
            or not _SHA256.fullmatch(file_sha256)
        ):
            raise ValueError("Dataset Release file declaration is invalid")
        path = release_root / "files" / logical_name
        _reject_symlink_chain(release_root, path)
        try:
            stat = path.stat()
            is_file = path.is_file()
        except OSError as error:
            raise ValueError(f"Dataset file is unavailable: {logical_name}") from error
        if not is_file or stat.st_size != byte_size:
            raise ValueError(f"Dataset file size does not match: {logical_name}")
        try:
            actual_sha256 = _file_sha256(path)
        except OSError as error:
            raise ValueError(f"Dataset file is unavailable: {logical_name}") from error
        if actual_sha256 != file_sha256:
            raise ValueError(f"Dataset file SHA-256 does not match: {logical_name}")
        for value in (logical_name, role, media_type, str(byte_size), file_sha256):
            _update_digest(release_digest, value)
        files[logical_name] = DatasetFile(
            logical_name=logical_name,
            role=role,
            media_type=media_type,
            byte_size=byte_size,
            content_sha256=file_sha256,
            path=path,
        )

    if release_digest.hexdigest() != content_sha256:
        raise ValueError("Dataset Release manifest content does not match its SHA-256")
    return DatasetRelease(
        workspace_id=workspace_id,
        release_id=release_id,
        dataset_id=dataset_id,
        version=version,
        content_sha256=content_sha256,
        files=files,
    )


def _authorized_workspace() -> tuple[Path, str]:
    workspace_root_override = os.environ.get(
        "OPEN_WEB_CODEX_AUTHORIZED_WORKSPACE_ROOT"
    )
    workspace_id_override = os.environ.get(
        "OPEN_WEB_CODEX_AUTHORIZED_WORKSPACE_ID"
    )
    if bool(workspace_root_override) != bool(workspace_id_override):
        raise RuntimeError("Authorized Workspace test identity is incomplete")
    if workspace_root_override and workspace_id_override:
        workspace_root = Path(workspace_root_override).resolve(strict=True)
        if not workspace_root.is_dir():
            raise RuntimeError("Authorized Workspace data root is unavailable")
        return workspace_root, str(UUID(workspace_id_override))

    package_root = Path(__file__).resolve().parent
    if len(package_root.parents) < 3 or package_root.parents[1].name != "tools":
        raise RuntimeError("Capability package is not in a versioned Workspace tools root")
    marker_path = package_root / ".open-web-release.json"
    _reject_symlink(marker_path)
    try:
        marker_bytes = marker_path.read_bytes()
    except OSError as error:
        raise RuntimeError("Capability package identity is unavailable") from error
    if not marker_bytes or len(marker_bytes) > _MAX_MANIFEST_BYTES:
        raise RuntimeError("Capability package identity is invalid")
    try:
        marker = json.loads(marker_bytes)
        workspace_id = str(UUID(_required_text(marker, "workspaceId")))
    except (TypeError, ValueError, json.JSONDecodeError) as error:
        raise RuntimeError("Capability package identity is invalid") from error
    if marker.get("schemaVersion") != "workspace.capability-package-release.v1":
        raise RuntimeError("Capability package identity is invalid")
    return package_root.parents[2], workspace_id


def _required_text(value: object, key: str) -> str:
    if not isinstance(value, dict):
        raise ValueError("Dataset Release descriptor is invalid")
    item = value.get(key)
    if not isinstance(item, str) or not item:
        raise ValueError(f"Dataset Release field is invalid: {key}")
    return item


def _safe_logical_name(value: str) -> bool:
    if (
        len(value.encode("utf-8")) > 256
        or "\x00" in value
        or "\\" in value
        or value.startswith("/")
        or value.startswith("./")
        or "//" in value
    ):
        return False
    return all(part not in ("", ".", "..") for part in value.split("/"))


def _reject_symlink(path: Path) -> None:
    if path.is_symlink():
        raise ValueError("Dataset Release contains a symbolic link")


def _reject_symlink_chain(root: Path, target: Path) -> None:
    root = root.resolve()
    try:
        relative = target.relative_to(root)
    except ValueError as error:
        raise ValueError("Dataset Release path escaped its Workspace") from error
    current = root
    for component in relative.parts:
        current = current / component
        if current.is_symlink():
            raise ValueError("Dataset Release contains a symbolic link")


def _file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        while chunk := source.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def _update_digest(digest: "hashlib._Hash", value: str) -> None:
    encoded = value.encode("utf-8")
    digest.update(len(encoded).to_bytes(8, "big"))
    digest.update(encoded)
