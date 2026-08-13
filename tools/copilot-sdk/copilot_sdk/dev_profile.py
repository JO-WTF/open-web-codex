"""Isolated native Profile materialization for Copilot discovery probes."""

from __future__ import annotations

import hashlib
import json
import os
import shutil
import stat
import tempfile
import tomllib
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from .copilot_manifest import CopilotPackageSummary, validate_copilot_package


OWNER_MARKER = ".copilot-dev-profile.json"


class CopilotDevError(RuntimeError):
    """A typed failure from an isolated Copilot development probe."""

    def __init__(
        self, code: str, stage: str, path: str, message: str, cause: str | None = None
    ) -> None:
        self.code = code
        self.stage = stage
        self.path = path
        self.message = message
        self.cause = cause
        super().__init__(f"{code}: {stage}: {path}: {message}")


@dataclass(frozen=True)
class DeclaredSkill:
    id: str
    source: Path


@dataclass(frozen=True)
class DeclaredAgent:
    id: str
    source: Path


@dataclass(frozen=True)
class DeclaredTool:
    id: str
    source: Path


@dataclass(frozen=True)
class DevComposition:
    source_root: Path
    manifest_path: Path
    summary: CopilotPackageSummary
    supervisor_skill: str
    skills: tuple[DeclaredSkill, ...]
    agents: tuple[DeclaredAgent, ...]
    tools: tuple[DeclaredTool, ...]


@dataclass(frozen=True)
class PreparedDevProfile:
    composition: DevComposition
    profile_root: Path
    process_root: Path
    process_home: Path
    process_cwd: Path
    process_data: Path
    temporary: bool
    cleanup_on_exit: bool

    def cleanup(self) -> None:
        shutil.rmtree(self.process_root, ignore_errors=True)
        if self.cleanup_on_exit:
            shutil.rmtree(self.profile_root, ignore_errors=True)


def load_dev_composition(
    source_root: Path, manifest_path: Path = Path("copilot.toml")
) -> DevComposition:
    """Validate first, then load only the bounded fields needed by a dev probe."""

    summary = validate_copilot_package(source_root, manifest_path)
    root = Path(source_root).resolve(strict=True)
    manifest = root / manifest_path
    with manifest.open("rb") as handle:
        data = tomllib.load(handle)
    return DevComposition(
        source_root=root,
        manifest_path=manifest_path,
        summary=summary,
        supervisor_skill=data["supervisor"]["skill"],
        skills=tuple(
            DeclaredSkill(entry["id"], _validated_source(root, entry["path"]))
            for entry in data["skills"]
        ),
        agents=tuple(
            DeclaredAgent(entry["id"], _validated_source(root, entry["role"]))
            for entry in data["agents"]
        ),
        tools=tuple(
            DeclaredTool(entry["id"], _validated_source(root, entry["root"]))
            for entry in data["tools"]
        ),
    )


def prepare_dev_profile(
    composition: DevComposition,
    profile: Path | None = None,
    *,
    keep_profile: bool = False,
) -> PreparedDevProfile:
    """Materialize declared Skills and Roles into an isolated native Profile."""

    temporary = profile is None
    process_root = Path(tempfile.mkdtemp(prefix="copilot-dev-process-"))
    profile_root: Path | None = None
    try:
        if temporary:
            profile_root = Path(tempfile.mkdtemp(prefix="copilot-dev-profile-"))
        else:
            requested_profile = Path(profile)
            if os.path.lexists(requested_profile):
                mode = requested_profile.lstat().st_mode
                if stat.S_ISLNK(mode):
                    _error(
                        "ProfileConflict",
                        "profile",
                        str(profile),
                        "profile must not be a symlink",
                    )
                if not stat.S_ISDIR(mode):
                    _error(
                        "ProfileConflict",
                        "profile",
                        str(profile),
                        "profile must be a directory",
                    )
            requested_profile.mkdir(parents=True, exist_ok=True)
            profile_root = requested_profile.resolve(strict=True)

        expected_marker = _owner_marker(composition)
        marker_path = profile_root / OWNER_MARKER
        entries = list(profile_root.iterdir())
        if entries:
            if not os.path.lexists(marker_path):
                _error(
                    "ProfileConflict",
                    "profile",
                    str(profile_root),
                    "non-empty profile is not owned by this Copilot source",
                )
            marker_mode = marker_path.lstat().st_mode
            if stat.S_ISLNK(marker_mode) or not stat.S_ISREG(marker_mode):
                _error(
                    "ProfileConflict",
                    "profile",
                    OWNER_MARKER,
                    "owner marker must be a regular file and not a symlink",
                )
            try:
                existing_marker = json.loads(marker_path.read_text(encoding="utf-8"))
            except (OSError, UnicodeError, json.JSONDecodeError) as exc:
                _error(
                    "ProfileConflict",
                    "profile",
                    OWNER_MARKER,
                    "owner marker is invalid",
                    str(exc),
                )
            if existing_marker != expected_marker:
                _error(
                    "ProfileConflict",
                    "profile",
                    OWNER_MARKER,
                    "profile belongs to a different Copilot source",
                )
            for entry in entries:
                if entry != marker_path:
                    _remove_owned_entry(entry)

        marker_path.write_text(
            json.dumps(expected_marker, ensure_ascii=False, sort_keys=True, indent=2) + "\n",
            encoding="utf-8",
        )
        skills_root = profile_root / "skills"
        agents_root = profile_root / "agents"
        skills_root.mkdir()
        agents_root.mkdir()
        copied_targets: set[Path] = set()
        for skill in composition.skills:
            destination = skills_root / skill.id
            _claim_target(copied_targets, destination)
            _copy_skill_tree(skill.source, destination, composition.source_root)
        for agent in composition.agents:
            destination = agents_root / f"{agent.id}.toml"
            _claim_target(copied_targets, destination)
            _copy_regular_file(agent.source, destination, composition.source_root)

        process_home = process_root / "home"
        process_cwd = process_root / "cwd"
        process_data = process_root / "data"
        process_home.mkdir()
        process_cwd.mkdir()
        process_data.mkdir()
        return PreparedDevProfile(
            composition=composition,
            profile_root=profile_root,
            process_root=process_root,
            process_home=process_home,
            process_cwd=process_cwd,
            process_data=process_data,
            temporary=temporary,
            cleanup_on_exit=temporary and not keep_profile,
        )
    except Exception:
        shutil.rmtree(process_root, ignore_errors=True)
        if temporary and profile_root is not None:
            shutil.rmtree(profile_root, ignore_errors=True)
        raise


def validate_workspace(workspace: Path) -> Path:
    if not workspace.is_absolute():
        _error("WorkspaceInvalid", "workspace", str(workspace), "must be absolute")
    try:
        canonical = workspace.resolve(strict=True)
    except OSError as exc:
        _error("WorkspaceInvalid", "workspace", str(workspace), "does not exist", str(exc))
    if not canonical.is_dir():
        _error("WorkspaceInvalid", "workspace", str(workspace), "must be a directory")
    return canonical


def resolve_tool_setup(composition: DevComposition, tool: DeclaredTool) -> Path:
    candidate = tool.source / "bin/setup-env"
    if not os.path.lexists(candidate):
        _error(
            "EnvironmentUnavailable",
            "tool-setup",
            str(candidate),
            "declared Tool must provide an executable bin/setup-env",
        )
    relative = candidate.relative_to(composition.source_root)
    setup = _validated_source(composition.source_root, relative.as_posix())
    if not setup.is_file() or not setup.stat().st_mode & 0o111:
        _error(
            "EnvironmentUnavailable",
            "tool-setup",
            str(setup),
            "declared Tool must provide an executable bin/setup-env",
        )
    return setup


def _owner_marker(composition: DevComposition) -> dict[str, Any]:
    identity = {
        "sourceRoot": str(composition.source_root),
        "manifestPath": composition.manifest_path.as_posix(),
        "copilotId": composition.summary.id,
    }
    encoded = json.dumps(identity, sort_keys=True, separators=(",", ":")).encode()
    return {
        "schemaVersion": 1,
        "sourceIdentity": identity,
        "sourceIdentitySha256": hashlib.sha256(encoded).hexdigest(),
    }


def _validated_source(root: Path, relative: str) -> Path:
    path = root / relative
    current = root
    for part in Path(relative).parts:
        current = current / part
        try:
            mode = current.lstat().st_mode
        except FileNotFoundError as exc:
            _error("UnsafePath", "materialize", relative, "source path is missing", str(exc))
        if stat.S_ISLNK(mode):
            _error("UnsafePath", "materialize", relative, "source path contains a symlink")
    try:
        path.resolve(strict=True).relative_to(root)
    except (OSError, ValueError) as exc:
        _error("UnsafePath", "materialize", relative, "source path escapes source root", str(exc))
    return path


def _copy_skill_tree(source: Path, destination: Path, source_root: Path) -> None:
    if not source.is_dir():
        _error("UnsafePath", "materialize", str(source), "Skill source must be a directory")
    destination.mkdir()
    for child in sorted(source.iterdir(), key=lambda item: item.name):
        _validated_source(source_root, child.relative_to(source_root).as_posix())
        target = destination / child.name
        if child.is_dir():
            _copy_skill_tree(child, target, source_root)
        elif child.is_file():
            _copy_regular_file(child, target, source_root)
        else:
            _error("UnsafePath", "materialize", str(child), "unsupported Skill entry type")


def _copy_regular_file(source: Path, destination: Path, source_root: Path) -> None:
    _validated_source(source_root, source.relative_to(source_root).as_posix())
    if not source.is_file():
        _error("UnsafePath", "materialize", str(source), "source must be a regular file")
    shutil.copyfile(source, destination, follow_symlinks=False)
    destination.chmod(source.stat().st_mode & 0o777)


def _claim_target(targets: set[Path], target: Path) -> None:
    if target in targets:
        _error("MaterializationFailed", "materialize", str(target), "duplicate target")
    targets.add(target)


def _remove_owned_entry(path: Path) -> None:
    if path.is_symlink():
        _error("ProfileConflict", "profile", str(path), "owned profile contains a symlink")
    if path.is_dir():
        shutil.rmtree(path)
    else:
        path.unlink()


def _error(
    code: str, stage: str, path: str, message: str, cause: str | None = None
) -> None:
    raise CopilotDevError(code, stage, path, message, cause)
