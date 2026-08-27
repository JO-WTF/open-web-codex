"""Turn Workspace scope parsing and race-safe create-new writes."""

from __future__ import annotations

import errno
import os
import secrets
import stat
from dataclasses import dataclass
from pathlib import Path, PurePosixPath
from typing import Any
from urllib.parse import unquote, urlparse

from .errors import WorkspaceFileError

SANDBOX_STATE_META_CAPABILITY = "codex/sandbox-state-meta"
MAX_WORKSPACE_FILE_BYTES = 32 * 1024 * 1024
MAX_WORKSPACE_RELATIVE_PATH_CHARS = 1024


@dataclass(frozen=True)
class CreatedWorkspaceFile:
    relative_path: str
    byte_size: int


def trusted_workspace_root(meta: Any) -> Path:
    extra = getattr(meta, "model_extra", None)
    state = extra.get(SANDBOX_STATE_META_CAPABILITY) if isinstance(extra, dict) else None
    sandbox_cwd = state.get("sandboxCwd") if isinstance(state, dict) else None
    if not isinstance(sandbox_cwd, str):
        raise WorkspaceFileError("workspace_scope_unavailable")
    parsed = urlparse(sandbox_cwd)
    if parsed.scheme != "file" or parsed.netloc not in ("", "localhost"):
        raise WorkspaceFileError("workspace_scope_invalid")
    try:
        root = Path(unquote(parsed.path)).resolve(strict=True)
    except (OSError, RuntimeError) as error:
        raise WorkspaceFileError("workspace_scope_invalid") from error
    if not root.is_dir():
        raise WorkspaceFileError("workspace_scope_invalid")
    return root


def create_workspace_file(
    workspace_root: Path,
    relative_path: str,
    content: bytes,
    *,
    max_bytes: int = MAX_WORKSPACE_FILE_BYTES,
) -> CreatedWorkspaceFile:
    parts = _relative_parts(relative_path)
    if not isinstance(content, bytes):
        raise WorkspaceFileError("workspace_content_invalid")
    if not isinstance(max_bytes, int) or isinstance(max_bytes, bool) or max_bytes <= 0:
        raise WorkspaceFileError("workspace_bound_invalid")
    if len(content) > max_bytes:
        raise WorkspaceFileError("workspace_file_too_large")
    try:
        root = workspace_root.resolve(strict=True)
    except (OSError, RuntimeError) as error:
        raise WorkspaceFileError("workspace_root_invalid") from error
    if not root.is_dir():
        raise WorkspaceFileError("workspace_root_invalid")

    root_fd = _open_directory(root)
    parent_fd = root_fd
    temporary_name: str | None = None
    try:
        for component in parts[:-1]:
            child_fd = _open_child_directory(parent_fd, component)
            if parent_fd != root_fd:
                os.close(parent_fd)
            parent_fd = child_fd
        target_name = parts[-1]
        _reject_existing_target(parent_fd, target_name)
        temporary_name, temporary_fd = _create_temporary(parent_fd)
        try:
            _write_all(temporary_fd, content)
            os.fsync(temporary_fd)
            temporary_identity = os.fstat(temporary_fd)
        except OSError as error:
            raise WorkspaceFileError("workspace_write_failed") from error
        finally:
            os.close(temporary_fd)
        target_linked = False
        try:
            os.link(
                temporary_name,
                target_name,
                src_dir_fd=parent_fd,
                dst_dir_fd=parent_fd,
                follow_symlinks=False,
            )
            target_linked = True
            os.fsync(parent_fd)
            os.unlink(temporary_name, dir_fd=parent_fd)
            temporary_name = None
            os.fsync(parent_fd)
        except FileExistsError as error:
            raise WorkspaceFileError("workspace_file_exists") from error
        except OSError as error:
            if target_linked:
                _unlink_matching_target(parent_fd, target_name, temporary_identity)
            raise WorkspaceFileError("workspace_write_failed") from error
    finally:
        if temporary_name is not None:
            try:
                os.unlink(temporary_name, dir_fd=parent_fd)
            except FileNotFoundError:
                pass
        if parent_fd != root_fd:
            os.close(parent_fd)
        os.close(root_fd)
    return CreatedWorkspaceFile(relative_path=relative_path, byte_size=len(content))


def ensure_workspace_directory(workspace_root: Path, relative_path: str) -> str:
    """Create one bounded Workspace-relative directory tree without following symlinks."""

    parts = _relative_parts(relative_path)
    try:
        root = workspace_root.resolve(strict=True)
    except (OSError, RuntimeError) as error:
        raise WorkspaceFileError("workspace_root_invalid") from error
    if not root.is_dir():
        raise WorkspaceFileError("workspace_root_invalid")

    root_fd = _open_directory(root)
    parent_fd = root_fd
    try:
        for component in parts:
            try:
                os.mkdir(component, mode=0o755, dir_fd=parent_fd)
                os.fsync(parent_fd)
            except FileExistsError:
                pass
            child_fd = _open_child_directory(parent_fd, component)
            if parent_fd != root_fd:
                os.close(parent_fd)
            parent_fd = child_fd
    finally:
        if parent_fd != root_fd:
            os.close(parent_fd)
        os.close(root_fd)
    return PurePosixPath(*parts).as_posix()


def _relative_parts(relative_path: str) -> tuple[str, ...]:
    if (
        not isinstance(relative_path, str)
        or not relative_path
        or len(relative_path) > MAX_WORKSPACE_RELATIVE_PATH_CHARS
        or "\x00" in relative_path
        or "\\" in relative_path
    ):
        raise WorkspaceFileError("workspace_path_invalid")
    raw_parts = relative_path.split("/")
    path = PurePosixPath(relative_path)
    if path.is_absolute() or any(part in ("", ".", "..") for part in raw_parts):
        raise WorkspaceFileError("workspace_path_invalid")
    return tuple(raw_parts)


def _open_directory(path: Path) -> int:
    flags = os.O_RDONLY | os.O_DIRECTORY
    if hasattr(os, "O_NOFOLLOW"):
        flags |= os.O_NOFOLLOW
    try:
        return os.open(path, flags)
    except OSError as error:
        raise WorkspaceFileError("workspace_root_invalid") from error


def _open_child_directory(parent_fd: int, name: str) -> int:
    try:
        metadata = os.stat(name, dir_fd=parent_fd, follow_symlinks=False)
    except FileNotFoundError as error:
        raise WorkspaceFileError("workspace_parent_missing") from error
    except OSError as error:
        raise WorkspaceFileError("workspace_parent_invalid") from error
    if stat.S_ISLNK(metadata.st_mode):
        raise WorkspaceFileError("workspace_symlink_rejected")
    if not stat.S_ISDIR(metadata.st_mode):
        raise WorkspaceFileError("workspace_parent_not_directory")
    flags = os.O_RDONLY | os.O_DIRECTORY
    if hasattr(os, "O_NOFOLLOW"):
        flags |= os.O_NOFOLLOW
    try:
        return os.open(name, flags, dir_fd=parent_fd)
    except OSError as error:
        code = "workspace_symlink_rejected" if error.errno == errno.ELOOP else "workspace_parent_invalid"
        raise WorkspaceFileError(code) from error


def _reject_existing_target(parent_fd: int, name: str) -> None:
    try:
        metadata = os.stat(name, dir_fd=parent_fd, follow_symlinks=False)
    except FileNotFoundError:
        return
    except OSError as error:
        raise WorkspaceFileError("workspace_target_invalid") from error
    if stat.S_ISLNK(metadata.st_mode):
        raise WorkspaceFileError("workspace_symlink_rejected")
    raise WorkspaceFileError("workspace_file_exists")


def _create_temporary(parent_fd: int) -> tuple[str, int]:
    flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL
    if hasattr(os, "O_NOFOLLOW"):
        flags |= os.O_NOFOLLOW
    for _attempt in range(16):
        name = f".open-web-codex-{secrets.token_hex(12)}.tmp"
        try:
            return name, os.open(name, flags, 0o600, dir_fd=parent_fd)
        except FileExistsError:
            continue
        except OSError as error:
            raise WorkspaceFileError("workspace_write_failed") from error
    raise WorkspaceFileError("workspace_write_failed")


def _write_all(descriptor: int, content: bytes) -> None:
    view = memoryview(content)
    written = 0
    while written < len(view):
        count = os.write(descriptor, view[written:])
        if count <= 0:
            raise OSError("short write")
        written += count


def _unlink_matching_target(parent_fd: int, name: str, identity: os.stat_result) -> None:
    try:
        target = os.stat(name, dir_fd=parent_fd, follow_symlinks=False)
        if (target.st_dev, target.st_ino) == (identity.st_dev, identity.st_ino):
            os.unlink(name, dir_fd=parent_fd)
            os.fsync(parent_fd)
    except OSError:
        pass
