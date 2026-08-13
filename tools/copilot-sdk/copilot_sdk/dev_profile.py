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
from .tool_environment import (
    MaterializedCapabilityRoot,
    MaterializedToolComposition,
    ToolEnvironmentError,
    ToolRuntimeSource,
    materialize_capability_roots,
    prepare_tool_composition,
)
from .tool_runtime_manifest import load_tool_runtime_manifest


OWNER_MARKER = ".copilot-dev-profile.json"
TOOL_ENVIRONMENT_CACHE_DIR = "open-web-codex/copilot-sdk/tool-environments"


class CopilotDevError(RuntimeError):
    """A typed failure from an isolated Copilot development probe."""

    def __init__(
        self,
        code: str,
        stage: str,
        path: str,
        message: str,
        cause: str | None = None,
        diagnostics: dict[str, Any] | None = None,
    ) -> None:
        self.code = code
        self.stage = stage
        self.path = path
        self.message = message
        self.cause = cause
        self.diagnostics = diagnostics or {}
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
    runtime: Path


@dataclass(frozen=True)
class DevComposition:
    source_root: Path
    manifest_path: Path
    summary: CopilotPackageSummary
    supervisor_skill: str
    skills: tuple[DeclaredSkill, ...]
    agents: tuple[DeclaredAgent, ...]
    tools: tuple[DeclaredTool, ...]
    mcp_server_ids: tuple[str, ...]


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
    agents = tuple(
        DeclaredAgent(entry["id"], _validated_source(root, entry["role"]))
        for entry in data["agents"]
    )
    tools = tuple(
        DeclaredTool(
            entry["id"],
            _validated_source(root, entry["root"]),
            Path(entry["runtime"]),
        )
        for entry in data["tools"]
    )
    return DevComposition(
        source_root=root,
        manifest_path=manifest_path,
        summary=summary,
        supervisor_skill=data["supervisor"]["skill"],
        skills=tuple(
            DeclaredSkill(entry["id"], _validated_source(root, entry["path"]))
            for entry in data["skills"]
        ),
        agents=agents,
        tools=tools,
        mcp_server_ids=_declared_mcp_server_ids(root, agents, tools),
    )


def _declared_mcp_server_ids(
    root: Path,
    agents: tuple[DeclaredAgent, ...],
    tools: tuple[DeclaredTool, ...],
) -> tuple[str, ...]:
    tools_by_id = {tool.id: tool for tool in tools}
    server_ids: list[str] = []
    for agent in agents:
        with agent.source.open("rb") as handle:
            role = tomllib.load(handle)
        plugins = role.get("plugins", {})
        assert isinstance(plugins, dict)  # complete manifest validation proved this
        for tool_id, plugin_policy in plugins.items():
            assert isinstance(plugin_policy, dict)
            policies = plugin_policy.get("mcp_servers", {})
            assert isinstance(policies, dict)
            tool = tools_by_id[tool_id]
            runtime = load_tool_runtime_manifest(root, tool.source, tool.runtime)
            transports = {server.id for server in runtime.servers}
            for server_id in policies:
                if server_id not in transports:
                    _error(
                        "MaterializationFailed",
                        "role-projection",
                        str(agent.source),
                        f"Tool {tool_id!r} does not declare MCP server {server_id!r}",
                    )
                if server_id not in server_ids:
                    server_ids.append(server_id)
    return tuple(server_ids)


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


def prepare_dev_tool_composition(
    prepared: PreparedDevProfile,
    *,
    host_environment: dict[str, str] | None = None,
    output_root: Path | None = None,
) -> MaterializedToolComposition:
    """Prepare generic Tool environments, then project their transport into Roles."""

    composition = prepared.composition
    environment_root = output_root or default_tool_environment_root(composition)
    try:
        prepared_tools = prepare_tool_composition(
            source_root=composition.source_root,
            tools=tuple(
                ToolRuntimeSource(tool.id, tool.source, tool.runtime)
                for tool in composition.tools
            ),
            output_root=environment_root,
            composition_descriptor_sha256=(
                composition.summary.composition_descriptor_sha256
            ),
            host_environment=host_environment,
        )
    except ToolEnvironmentError as error:
        _error(
            error.code,
            "tool-environment",
            error.path,
            "declared Tool environment could not be prepared",
            error.cause,
        )
    tools = materialize_capability_roots(
        prepared_tools,
        profile_home=prepared.profile_root,
        state_root=prepared.process_data / "runtime",
    )
    roots = {root.id: root for root in tools.capability_roots}
    for agent in composition.agents:
        _materialize_role(
            agent,
            prepared.profile_root / "agents" / f"{agent.id}.toml",
            composition,
            roots,
        )
    return tools


def default_tool_environment_root(composition: DevComposition) -> Path:
    """Return the stable disposable cache owned by the local Copilot SDK."""

    configured_cache = os.environ.get("XDG_CACHE_HOME")
    cache_home = Path(configured_cache) if configured_cache else Path.home() / ".cache"
    if not cache_home.is_absolute():
        _error(
            "EnvironmentUnavailable",
            "tool-environment",
            "XDG_CACHE_HOME",
            "must be an absolute path when set",
        )
    source_identity = _owner_marker(composition)["sourceIdentitySha256"]
    assert isinstance(source_identity, str)
    return cache_home / TOOL_ENVIRONMENT_CACHE_DIR / source_identity


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
    agent: DeclaredAgent,
    destination: Path,
    composition: DevComposition,
    prepared_roots: dict[str, MaterializedCapabilityRoot],
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
    projected: dict[str, dict[str, Any]] = {}
    for tool_id, plugin_policy in plugins.items():
        prepared_root = prepared_roots.get(tool_id)
        if prepared_root is None:
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
        servers = {server.id: server for server in prepared_root.servers}
        for server_id, policy in policies.items():
            if server_id in projected:
                _error(
                    "MaterializationFailed",
                    "role-projection",
                    str(agent.source),
                    f"MCP server {server_id!r} would have multiple transports",
                )
            transport = servers.get(server_id)
            if transport is None:
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
            runtime_server: dict[str, Any] = {
                "command": str(transport.command),
                "args": list(transport.args),
                "env": dict(transport.env),
                "env_vars": list(transport.env_vars),
            }
            if transport.startup_timeout_sec is not None:
                runtime_server["startup_timeout_sec"] = transport.startup_timeout_sec
            if transport.tool_timeout_sec is not None:
                runtime_server["tool_timeout_sec"] = transport.tool_timeout_sec
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
    materialize_capability_roots,
