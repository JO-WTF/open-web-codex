"""Deterministic Tool package validation and packing."""

from __future__ import annotations

import hashlib
import json
import os
import stat
import zipfile
from pathlib import Path
from typing import Any


MAX_FILES = 512
MAX_FILE_BYTES = 4 * 1024 * 1024
MAX_PACKAGE_BYTES = 32 * 1024 * 1024
REQUIRED_FILES = (".codex-plugin/plugin.json", ".mcp.json")


class PackageError(ValueError):
    pass


def _read_json(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise PackageError(f"invalid JSON: {path.relative_to(path.parent.parent)}") from exc
    if not isinstance(value, dict):
        raise PackageError(f"JSON document must be an object: {path}")
    return value


def validate_tool_package(root: Path) -> dict[str, Any]:
    root = root.resolve()
    if not root.is_dir():
        raise PackageError(f"package directory does not exist: {root}")
    for relative in REQUIRED_FILES:
        path = root / relative
        if not path.is_file():
            raise PackageError(f"missing required file: {relative}")
    plugin = _read_json(root / ".codex-plugin/plugin.json")
    servers = _read_json(root / ".mcp.json")
    name = plugin.get("name")
    version = plugin.get("version")
    if not isinstance(name, str) or not name.strip():
        raise PackageError("plugin name is required")
    if not isinstance(version, str) or not version.strip():
        raise PackageError("plugin version is required")
    if not isinstance(plugin.get("mcpServers"), str):
        raise PackageError("plugin mcpServers must reference .mcp.json")
    if not isinstance(servers.get("mcpServers"), dict) or not servers["mcpServers"]:
        raise PackageError(".mcp.json must declare at least one MCP server")
    files = []
    total_bytes = 0
    for path in sorted(root.rglob("*")):
        if path.is_symlink() or not path.is_file():
            continue
        relative = path.relative_to(root).as_posix()
        if relative.startswith((".git/", ".venv/", "__pycache__/")) or relative.endswith(".pyc"):
            continue
        if ".." in Path(relative).parts:
            raise PackageError(f"path traversal is not allowed: {relative}")
        size = path.stat().st_size
        if size > MAX_FILE_BYTES:
            raise PackageError(f"file exceeds {MAX_FILE_BYTES} bytes: {relative}")
        total_bytes += size
        files.append(relative)
    if len(files) > MAX_FILES or total_bytes > MAX_PACKAGE_BYTES:
        raise PackageError("package exceeds file or size limits")
    return {
        "name": name,
        "version": version,
        "files": files,
        "contentSha256": content_sha256(root, files),
    }


def content_sha256(root: Path, files: list[str] | None = None) -> str:
    paths = files or sorted(
        path.relative_to(root).as_posix()
        for path in root.rglob("*")
        if path.is_file() and not path.is_symlink()
    )
    digest = hashlib.sha256()
    for relative in sorted(paths):
        data = (root / relative).read_bytes()
        digest.update(relative.encode("utf-8"))
        digest.update(b"\0")
        digest.update(data)
        digest.update(b"\0")
    return digest.hexdigest()


def pack_tool_package(root: Path, output: Path) -> dict[str, Any]:
    summary = validate_tool_package(root)
    root = root.resolve()
    output = output.resolve()
    if output == root or root in output.parents:
        raise PackageError("output archive must be outside the package directory")
    output.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for relative in summary["files"]:
            path = root / relative
            info = zipfile.ZipInfo(relative, date_time=(1980, 1, 1, 0, 0, 0))
            info.external_attr = (stat.S_IFREG | 0o644) << 16
            info.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(info, path.read_bytes())
    summary["archive"] = str(output)
    summary["archiveSha256"] = hashlib.sha256(output.read_bytes()).hexdigest()
    return summary
