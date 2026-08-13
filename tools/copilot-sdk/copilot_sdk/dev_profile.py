"""Isolated native Profile materialization for Copilot discovery probes."""

from __future__ import annotations

import hashlib
import json
import os
import copy
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
            _materialize_role(agent, destination, composition)

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


def _materialize_role(
    agent: DeclaredAgent, destination: Path, composition: DevComposition
) -> None:
    try:
        with agent.source.open("rb") as handle:
            role = tomllib.load(handle)
    except (OSError, tomllib.TOMLDecodeError) as error:
        _error(
            "MaterializationFailed",
            "role-projection",
            str(agent.source),
            "declared Role could not be read",
            str(error),
        )
    plugins = role.get("plugins")
    if plugins is None:
        _copy_regular_file(agent.source, destination, composition.source_root)
        return
    if not isinstance(plugins, dict):
        _error(
            "MaterializationFailed",
            "role-projection",
            str(agent.source),
            "Role plugins policy must be a table",
        )
    declared_tools = {tool.id: tool for tool in composition.tools}
    projected: dict[str, dict[str, Any]] = {}
    for tool_id, plugin_policy in plugins.items():
        tool = declared_tools.get(tool_id)
        if tool is None:
            _error(
                "MaterializationFailed",
                "role-projection",
                str(agent.source),
                f"Role references undeclared Tool {tool_id!r}",
            )
        if not isinstance(plugin_policy, dict):
            _error(
                "MaterializationFailed",
                "role-projection",
                str(agent.source),
                f"Plugin policy for {tool_id!r} must be a table",
            )
        policies = plugin_policy.get("mcp_servers")
        if not isinstance(policies, dict):
            continue
        descriptor = _load_tool_mcp_descriptor(tool, composition)
        servers = descriptor.get("mcpServers")
        assert isinstance(servers, dict)  # static package validation already proved this
        for server_id, policy in policies.items():
            if server_id in projected:
                _error(
                    "MaterializationFailed",
                    "role-projection",
                    str(agent.source),
                    f"MCP server {server_id!r} would have multiple transports",
                )
            transport = servers.get(server_id)
            if not isinstance(transport, dict):
                _error(
                    "MaterializationFailed",
                    "role-projection",
                    str(agent.source),
                    f"Tool {tool_id!r} does not declare MCP server {server_id!r}",
                )
            if not isinstance(policy, dict):
                _error(
                    "MaterializationFailed",
                    "role-projection",
                    str(agent.source),
                    f"MCP policy for {server_id!r} must be a table",
                )
            runtime_server = copy.deepcopy(transport)
            runtime_server["command"] = str(
                _resolve_transport_path(
                    tool,
                    runtime_server.get("command"),
                    composition,
                    field="command",
                    kind="file",
                )
            )
            runtime_server["cwd"] = str(
                _resolve_transport_path(
                    tool,
                    runtime_server.get("cwd", "."),
                    composition,
                    field="cwd",
                    kind="directory",
                )
            )
            allowed_policy = {
                "enabled",
                "default_tools_approval_mode",
                "enabled_tools",
                "disabled_tools",
                "tools",
            }
            for key, value in policy.items():
                if key in allowed_policy:
                    runtime_server[key] = copy.deepcopy(value)
            projected[server_id] = runtime_server
    runtime_role = copy.deepcopy(role)
    runtime_role.pop("plugins", None)
    if projected:
        runtime_role["mcp_servers"] = projected
    rendered = _dump_toml_document(runtime_role)
    destination.write_text(rendered, encoding="utf-8")
    destination.chmod(agent.source.stat().st_mode & 0o777)


def _load_tool_mcp_descriptor(
    tool: DeclaredTool, composition: DevComposition
) -> dict[str, Any]:
    descriptor_path = _validated_source(
        composition.source_root,
        (tool.source / ".mcp.json").relative_to(composition.source_root).as_posix(),
    )
    try:
        descriptor = json.loads(descriptor_path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        _error(
            "MaterializationFailed",
            "role-projection",
            str(descriptor_path),
            "Tool MCP descriptor could not be read",
            str(error),
        )
    if not isinstance(descriptor, dict) or not isinstance(
        descriptor.get("mcpServers"), dict
    ):
        _error(
            "MaterializationFailed",
            "role-projection",
            str(descriptor_path),
            "Tool MCP descriptor must contain mcpServers",
        )
    return descriptor


def _resolve_transport_path(
    tool: DeclaredTool,
    value: Any,
    composition: DevComposition,
    *,
    field: str,
    kind: str,
) -> Path:
    if not isinstance(value, str) or not value:
        _error(
            "MaterializationFailed",
            "role-projection",
            str(tool.source / ".mcp.json"),
            f"MCP transport {field} must be a non-empty relative path",
        )
    authored = Path(value)
    if authored.is_absolute() or any(part == ".." for part in authored.parts):
        _error(
            "UnsafePath",
            "role-projection",
            value,
            f"MCP transport {field} must remain inside its Tool root",
        )
    candidate = tool.source / authored
    resolved = _validated_source(
        composition.source_root,
        candidate.relative_to(composition.source_root).as_posix(),
    )
    if kind == "file" and not resolved.is_file():
        _error("UnsafePath", "role-projection", value, "transport command must be a file")
    if kind == "directory" and not resolved.is_dir():
        _error("UnsafePath", "role-projection", value, "transport cwd must be a directory")
    return resolved.resolve(strict=True)


def _dump_toml_tables(prefix: tuple[str, ...], table: dict[str, Any]) -> str:
    lines: list[str] = []
    scalars = {
        key: value
        for key, value in table.items()
        if not isinstance(value, dict) and not _is_array_of_tables(value)
    }
    nested = {key: value for key, value in table.items() if isinstance(value, dict)}
    arrays = {key: value for key, value in table.items() if _is_array_of_tables(value)}
    if scalars:
        lines.append("[" + ".".join(_toml_key(part) for part in prefix) + "]")
        for key, value in scalars.items():
            lines.append(f"{_toml_key(key)} = {_toml_value(value)}")
        lines.append("")
    for key, value in nested.items():
        lines.append(_dump_toml_tables((*prefix, key), value).rstrip())
        lines.append("")
    for key, value in arrays.items():
        lines.append(_dump_toml_array((*prefix, key), value).rstrip())
        lines.append("")
    return "\n".join(lines).rstrip() + "\n"


def _dump_toml_document(table: dict[str, Any]) -> str:
    lines: list[str] = []
    scalars = {
        key: value
        for key, value in table.items()
        if not isinstance(value, dict) and not _is_array_of_tables(value)
    }
    nested = {key: value for key, value in table.items() if isinstance(value, dict)}
    arrays = {key: value for key, value in table.items() if _is_array_of_tables(value)}
    for key, value in scalars.items():
        lines.append(f"{_toml_key(key)} = {_toml_value(value)}")
    if scalars and (nested or arrays):
        lines.append("")
    for key, value in nested.items():
        lines.append(_dump_toml_tables((key,), value).rstrip())
        lines.append("")
    for key, value in arrays.items():
        lines.append(_dump_toml_array((key,), value).rstrip())
        lines.append("")
    return "\n".join(lines).rstrip() + "\n"


def _dump_toml_array(prefix: tuple[str, ...], values: list[Any]) -> str:
    lines: list[str] = []
    for value in values:
        if not isinstance(value, dict):
            _error(
                "MaterializationFailed",
                "role-projection",
                ".",
                "TOML array-of-tables entries must be tables",
            )
        lines.append("[[" + ".".join(_toml_key(part) for part in prefix) + "]]")
        scalars = {key: item for key, item in value.items() if not isinstance(item, dict)}
        nested = {key: item for key, item in value.items() if isinstance(item, dict)}
        for key, item in scalars.items():
            lines.append(f"{_toml_key(key)} = {_toml_value(item)}")
        for key, item in nested.items():
            lines.append("")
            lines.append(_dump_toml_tables((*prefix, key), item).rstrip())
        lines.append("")
    return "\n".join(lines).rstrip() + "\n"


def _is_array_of_tables(value: Any) -> bool:
    return isinstance(value, list) and bool(value) and all(
        isinstance(item, dict) for item in value
    )


def _toml_key(value: str) -> str:
    return json.dumps(value, ensure_ascii=False)


def _toml_value(value: Any) -> str:
    if isinstance(value, str):
        return json.dumps(value, ensure_ascii=False)
    if isinstance(value, bool):
        return "true" if value else "false"
    if type(value) in (int, float):
        return str(value)
    if isinstance(value, list):
        return "[" + ", ".join(_toml_value(item) for item in value) + "]"
    if value is None:
        _error(
            "MaterializationFailed",
            "role-projection",
            ".",
            "TOML projection does not support null values",
        )
    _error(
        "MaterializationFailed",
        "role-projection",
        ".",
        f"unsupported TOML projection value {type(value).__name__}",
    )


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
