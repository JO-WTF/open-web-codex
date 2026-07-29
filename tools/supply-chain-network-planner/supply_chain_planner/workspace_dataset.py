"""Verified access to one exact platform Workspace Dataset Release."""

from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass
from pathlib import Path
from typing import BinaryIO
from uuid import UUID

from .indonesia_models import DatasetReleaseBinding

MAX_MANIFEST_BYTES = 256 * 1024
MAX_FILES = 32
MAX_FILE_BYTES = 32 * 1024 * 1024


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
class WorkspaceDatasetRelease:
    binding: DatasetReleaseBinding
    display_name: str
    description: str
    files: dict[str, DatasetFile]

    def require_file(self, logical_name: str) -> DatasetFile:
        try:
            return self.files[logical_name]
        except KeyError as error:
            raise ValueError(f"Dataset file is not declared: {logical_name}") from error


def load_workspace_dataset_release(
    workspace_root: Path,
    binding: DatasetReleaseBinding,
) -> WorkspaceDatasetRelease:
    """Load and hash-check the exact Release under the trusted Turn Workspace."""

    workspace_id = str(UUID(binding.workspace_id))
    release_id = str(UUID(binding.release_id))
    workspace_root = workspace_root.resolve(strict=True)
    if not workspace_root.is_dir() or workspace_root.name != workspace_id:
        raise ValueError("The Dataset Release workspace does not match the current authorized Turn")

    release_root = workspace_root / "datasets" / binding.dataset_id / binding.version
    _reject_symlink_chain(workspace_root, release_root)
    manifest_path = release_root / "release.json"
    _reject_symlink(manifest_path)
    manifest_bytes = manifest_path.read_bytes()
    if not manifest_bytes or len(manifest_bytes) > MAX_MANIFEST_BYTES:
        raise ValueError("Dataset Release manifest size is invalid")
    manifest = json.loads(manifest_bytes)
    if (
        manifest.get("schemaVersion") != "workspace.dataset-release.v1"
        or manifest.get("releaseId") != release_id
        or manifest.get("datasetId") != binding.dataset_id
        or manifest.get("version") != binding.version
        or manifest.get("contentSha256") != binding.content_sha256
    ):
        raise ValueError("Dataset Release identity does not match the authorized binding")

    display_name = _required_text(manifest, "displayName")
    description = _required_text(manifest, "description")
    declared_files = manifest.get("files")
    if not isinstance(declared_files, list) or not 1 <= len(declared_files) <= MAX_FILES:
        raise ValueError("Dataset Release file declaration is invalid")

    files: dict[str, DatasetFile] = {}
    release_digest = hashlib.sha256()
    for value in (
        "workspace.dataset-release.v1",
        binding.dataset_id,
        binding.version,
        display_name,
        description,
    ):
        _update_digest(release_digest, value)

    for descriptor in sorted(
        declared_files,
        key=lambda item: item.get("logicalName", "") if isinstance(item, dict) else "",
    ):
        logical_name = _required_text(descriptor, "logicalName")
        role = _required_text(descriptor, "role")
        media_type = _required_text(descriptor, "mediaType")
        relative_path = _required_text(descriptor, "relativePath")
        file_sha256 = _required_text(descriptor, "contentSha256")
        byte_size = descriptor.get("byteSize") if isinstance(descriptor, dict) else None
        if (
            logical_name in files
            or relative_path != f"files/{logical_name}"
            or not _safe_logical_name(logical_name)
            or not isinstance(byte_size, int)
            or not 0 < byte_size <= MAX_FILE_BYTES
            or len(file_sha256) != 64
            or any(character not in "0123456789abcdef" for character in file_sha256)
        ):
            raise ValueError("Dataset Release file declaration is invalid")
        path = release_root / "files" / logical_name
        _reject_symlink_chain(release_root, path)
        stat = path.stat()
        if not path.is_file() or stat.st_size != byte_size:
            raise ValueError(f"Dataset file size does not match: {logical_name}")
        if _file_sha256(path) != file_sha256:
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

    if release_digest.hexdigest() != binding.content_sha256:
        raise ValueError("Dataset Release content does not match its platform SHA-256")
    return WorkspaceDatasetRelease(
        binding=binding,
        display_name=display_name,
        description=description,
        files=files,
    )


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
    root = root.resolve(strict=True)
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


def _update_digest(digest: hashlib._Hash, value: str) -> None:
    encoded = value.encode("utf-8")
    digest.update(len(encoded).to_bytes(8, "big"))
    digest.update(encoded)
