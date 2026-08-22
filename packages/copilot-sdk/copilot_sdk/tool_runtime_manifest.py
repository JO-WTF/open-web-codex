"""Typed, language-adapter-neutral Tool runtime declarations."""

from __future__ import annotations

import json
import os
import re
import stat
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Literal

import tomllib

from .platform_packages import platform_package

DependencyKind = Literal["python-project", "node-project"]
EnvironmentSource = Literal[
    "profile_home", "tool_state_root", "dependency_root", "host"
]
HOST_ENVIRONMENT_NAMES = frozenset(
    {
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "NO_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
        "no_proxy",
    }
)


class ToolRuntimeManifestError(ValueError):
    """A typed static failure in a Tool runtime declaration."""

    def __init__(self, code: str, relative_path: str, message: str) -> None:
        self.code = code
        self.relative_path = relative_path
        self.message = message
        super().__init__(f"{code}: {relative_path}: {message}")


@dataclass(frozen=True)
class RuntimeDependency:
    id: str
    kind: DependencyKind
    manifest: Path
    lock: Path
    platform_packages: tuple[str, ...]


@dataclass(frozen=True)
class RuntimeEnvironmentBinding:
    name: str
    source: EnvironmentSource
    dependency: str | None = None


@dataclass(frozen=True)
class PythonModuleEntry:
    dependency: str
    module: str


@dataclass(frozen=True)
class RuntimeServer:
    id: str
    entry: PythonModuleEntry
    args: tuple[str, ...]
    env: tuple[RuntimeEnvironmentBinding, ...]
    startup_timeout_sec: int | None
    tool_timeout_sec: int | None


@dataclass(frozen=True)
class ToolRuntimeManifest:
    path: Path
    dependencies: tuple[RuntimeDependency, ...]
    servers: tuple[RuntimeServer, ...]


_ID = re.compile(r"^[a-zA-Z0-9][a-zA-Z0-9_.-]{0,127}$")
_ENV = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*$")
_MODULE = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*(?:\.[A-Za-z_][A-Za-z0-9_]*)*$")
_SIMPLE_REQUIREMENT = re.compile(
    r"^([A-Za-z0-9][A-Za-z0-9._-]*)(?:\s*(?:===|==|~=|!=|<=|>=|<|>)\s*"
    r"[A-Za-z0-9][A-Za-z0-9.*+!_-]*(?:\s*,\s*(?:===|==|~=|!=|<=|>=|<|>)\s*"
    r"[A-Za-z0-9][A-Za-z0-9.*+!_-]*)*)?$"
)


def load_tool_runtime_manifest(
    source_root: Path,
    tool_root: Path,
    runtime_path: str | os.PathLike[str],
) -> ToolRuntimeManifest:
    """Load one explicit runtime v1 descriptor without discovering implementation files."""

    source = Path(source_root).resolve(strict=True)
    tool = Path(tool_root).resolve(strict=True)
    try:
        tool.relative_to(source)
    except ValueError:
        _fail("invalid_path", str(tool_root), "Tool root must remain inside source root")
    runtime_relative = _relative(runtime_path, "runtime")
    runtime = _safe_file(source, runtime_relative)
    try:
        runtime.relative_to(tool)
    except ValueError:
        _fail("invalid_path", runtime_relative.as_posix(), "runtime must be inside its Tool root")
    data = _toml(runtime, runtime_relative.as_posix())
    _keys(data, {"schema_version", "dependencies", "servers"}, runtime_relative.as_posix())
    if data.get("schema_version") != 1:
        _fail("schema_version", f"{runtime_relative}.schema_version", "must equal 1")

    raw_dependencies = data.get("dependencies")
    if not isinstance(raw_dependencies, list) or not raw_dependencies:
        _fail("required_field", f"{runtime_relative}.dependencies", "must be a non-empty array")
    dependencies: list[RuntimeDependency] = []
    dependency_ids: set[str] = set()
    dependency_kinds: dict[str, DependencyKind] = {}
    for index, raw in enumerate(raw_dependencies):
        location = f"{runtime_relative}.dependencies[{index}]"
        if not isinstance(raw, dict):
            _fail("invalid_type", location, "must be a table")
        _keys(raw, {"id", "kind", "manifest", "lock", "platform_packages"}, location)
        dependency_id = _identifier(raw.get("id"), f"{location}.id")
        if dependency_id in dependency_ids:
            _fail("duplicate_id", f"{location}.id", f"duplicate id {dependency_id!r}")
        kind = raw.get("kind")
        if kind not in ("python-project", "node-project"):
            _fail("invalid_type", f"{location}.kind", "must be python-project or node-project")
        manifest = _tool_file(tool, raw.get("manifest"), f"{location}.manifest")
        lock = _tool_file(tool, raw.get("lock"), f"{location}.lock")
        raw_platform_packages = raw.get("platform_packages", [])
        if not isinstance(raw_platform_packages, list) or any(
            not isinstance(package, str) or not _ID.fullmatch(package)
            for package in raw_platform_packages
        ):
            _fail(
                "invalid_type",
                f"{location}.platform_packages",
                "must be an array of registered package identifiers",
            )
        if len(set(raw_platform_packages)) != len(raw_platform_packages):
            _fail(
                "duplicate_id",
                f"{location}.platform_packages",
                "must not contain duplicate package identifiers",
            )
        if kind != "python-project" and raw_platform_packages:
            _fail(
                "invalid_field",
                f"{location}.platform_packages",
                "is supported only for python-project dependencies",
            )
        for package in raw_platform_packages:
            try:
                platform_package(package)
            except ValueError:
                _fail(
                    "missing_reference",
                    f"{location}.platform_packages",
                    f"references unregistered platform package {package!r}",
                )
        if kind == "python-project":
            locked_names = _validate_python_lock(lock, f"{location}.lock")
            _validate_python_build_requirements(
                manifest,
                locked_names,
                f"{location}.manifest",
            )
            _validate_python_project_requirements(
                manifest,
                locked_names,
                set(raw_platform_packages),
                f"{location}.manifest",
            )
        else:
            _validate_node_lock(manifest, lock, location)
        dependencies.append(
            RuntimeDependency(
                dependency_id,
                kind,
                manifest,
                lock,
                tuple(raw_platform_packages),
            )
        )
        dependency_ids.add(dependency_id)
        dependency_kinds[dependency_id] = kind

    raw_servers = data.get("servers")
    if not isinstance(raw_servers, list) or not raw_servers:
        _fail("required_field", f"{runtime_relative}.servers", "must be a non-empty array")
    servers: list[RuntimeServer] = []
    server_ids: set[str] = set()
    for index, raw in enumerate(raw_servers):
        location = f"{runtime_relative}.servers[{index}]"
        if not isinstance(raw, dict):
            _fail("invalid_type", location, "must be a table")
        _keys(
            raw,
            {"id", "entry", "args", "env", "startup_timeout_sec", "tool_timeout_sec"},
            location,
        )
        server_id = _identifier(raw.get("id"), f"{location}.id")
        if server_id in server_ids:
            _fail("duplicate_id", f"{location}.id", f"duplicate id {server_id!r}")
        entry = raw.get("entry")
        if not isinstance(entry, dict):
            _fail("required_field", f"{location}.entry", "must be a table")
        _keys(entry, {"kind", "dependency", "module"}, f"{location}.entry")
        if entry.get("kind") != "python-module":
            _fail("invalid_type", f"{location}.entry.kind", "must equal python-module")
        entry_dependency = _identifier(entry.get("dependency"), f"{location}.entry.dependency")
        if dependency_kinds.get(entry_dependency) != "python-project":
            _fail(
                "missing_reference",
                f"{location}.entry.dependency",
                "must reference a declared python-project dependency",
            )
        module = entry.get("module")
        if not isinstance(module, str) or not _MODULE.fullmatch(module):
            _fail("invalid_type", f"{location}.entry.module", "must be a dotted Python module")
        args = raw.get("args", [])
        if not isinstance(args, list) or any(not isinstance(arg, str) for arg in args):
            _fail("invalid_type", f"{location}.args", "must be a string array")
        raw_env = raw.get("env", [])
        if not isinstance(raw_env, list):
            _fail("invalid_type", f"{location}.env", "must be an array of tables")
        env: list[RuntimeEnvironmentBinding] = []
        env_names: set[str] = set()
        for env_index, binding in enumerate(raw_env):
            env_location = f"{location}.env[{env_index}]"
            if not isinstance(binding, dict):
                _fail("invalid_type", env_location, "must be a table")
            _keys(binding, {"name", "source", "dependency"}, env_location)
            name = binding.get("name")
            if not isinstance(name, str) or not _ENV.fullmatch(name):
                _fail("invalid_type", f"{env_location}.name", "must be an environment variable name")
            if name in env_names:
                _fail("duplicate_id", f"{env_location}.name", f"duplicate binding {name!r}")
            source_kind = binding.get("source")
            if source_kind not in ("profile_home", "tool_state_root", "dependency_root", "host"):
                _fail("invalid_type", f"{env_location}.source", "is not a supported environment source")
            bound_dependency = binding.get("dependency")
            if source_kind == "dependency_root":
                bound_dependency = _identifier(bound_dependency, f"{env_location}.dependency")
                if bound_dependency not in dependency_ids:
                    _fail("missing_reference", f"{env_location}.dependency", "references an undeclared dependency")
            elif source_kind == "host" and name not in HOST_ENVIRONMENT_NAMES:
                _fail(
                    "unsupported_host_environment",
                    f"{env_location}.name",
                    "host bindings are limited to standard proxy environment variables",
                )
            elif bound_dependency is not None:
                _fail("invalid_field", f"{env_location}.dependency", "is allowed only for dependency_root")
            env.append(RuntimeEnvironmentBinding(name, source_kind, bound_dependency))
            env_names.add(name)
        servers.append(
            RuntimeServer(
                server_id,
                PythonModuleEntry(entry_dependency, module),
                tuple(args),
                tuple(env),
                _optional_positive_integer(
                    raw.get("startup_timeout_sec"), f"{location}.startup_timeout_sec"
                ),
                _optional_positive_integer(
                    raw.get("tool_timeout_sec"), f"{location}.tool_timeout_sec"
                ),
            )
        )
        server_ids.add(server_id)
    return ToolRuntimeManifest(runtime, tuple(dependencies), tuple(servers))


def _validate_python_lock(path: Path, location: str) -> set[str]:
    """Require pip's hash-checking form; the Tool project itself is installed separately."""

    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except (OSError, UnicodeError) as error:
        _fail("invalid_lock", location, str(error))
    requirement_seen = False
    locked_names: set[str] = set()
    current_requirement: str | None = None
    current_has_hash = False
    for line in lines:
        stripped = line.strip()
        if not stripped or stripped.startswith("#") or stripped == "\\":
            continue
        if stripped.startswith("--hash=sha256:"):
            if current_requirement is None:
                _fail("invalid_lock", location, "hash must follow an exact requirement")
            digest = stripped.removeprefix("--hash=sha256:").removesuffix(" \\").strip()
            if len(digest) != 64 or any(character not in "0123456789abcdef" for character in digest):
                _fail("invalid_lock", location, "contains an invalid sha256 hash")
            current_has_hash = True
            continue
        if stripped.startswith("-"):
            _fail("invalid_lock", location, "must not contain includes, editable installs, or local paths")
        if current_requirement is not None and not current_has_hash:
            _fail("invalid_lock", location, f"requirement {current_requirement!r} has no sha256 hash")
        requirement = stripped.removesuffix(" \\").strip()
        if "==" not in requirement or " @ " in requirement or requirement.startswith((".", "/")):
            _fail("invalid_lock", location, "must contain only exact named requirements")
        current_requirement = requirement.split("==", 1)[0].strip()
        locked_names.add(_normalize_package_name(current_requirement))
        requirement_seen = True
        current_has_hash = "--hash=sha256:" in stripped
    if current_requirement is not None and not current_has_hash:
        _fail("invalid_lock", location, f"requirement {current_requirement!r} has no sha256 hash")
    if not requirement_seen:
        _fail("invalid_lock", location, "must contain exact requirements with sha256 hashes")
    return locked_names


def _validate_python_build_requirements(
    manifest: Path, locked_names: set[str], location: str
) -> None:
    data = _toml(manifest, location)
    build_system = data.get("build-system")
    if not isinstance(build_system, dict):
        _fail("invalid_manifest", f"{location}.build-system", "required table is missing")
    requirements = build_system.get("requires")
    if not isinstance(requirements, list) or not requirements:
        _fail(
            "invalid_manifest",
            f"{location}.build-system.requires",
            "must be a non-empty string array",
        )
    for index, requirement in enumerate(requirements):
        field = f"{location}.build-system.requires[{index}]"
        if not isinstance(requirement, str):
            _fail("invalid_manifest", field, "must be a simple package requirement")
        matched = _SIMPLE_REQUIREMENT.fullmatch(requirement.strip())
        if matched is None:
            _fail(
                "unsupported_requirement",
                field,
                "runtime v1 supports only package names with simple version constraints",
            )
        name = _normalize_package_name(matched.group(1))
        if name not in locked_names:
            _fail(
                "invalid_lock",
                field,
                f"build requirement {name!r} must have an exact hashed lock entry",
            )


def _validate_python_project_requirements(
    manifest: Path,
    locked_names: set[str],
    declared_platform_packages: set[str],
    location: str,
) -> None:
    data = _toml(manifest, location)
    project = data.get("project")
    if not isinstance(project, dict):
        _fail("invalid_manifest", f"{location}.project", "required table is missing")
    requirements = project.get("dependencies", [])
    if not isinstance(requirements, list):
        _fail(
            "invalid_manifest",
            f"{location}.project.dependencies",
            "must be a string array",
        )
    project_platform_packages: set[str] = set()
    for index, requirement in enumerate(requirements):
        field = f"{location}.project.dependencies[{index}]"
        if not isinstance(requirement, str):
            _fail("invalid_manifest", field, "must be a simple package requirement")
        matched = _SIMPLE_REQUIREMENT.fullmatch(requirement.strip())
        if matched is None:
            _fail(
                "unsupported_requirement",
                field,
                "runtime v1 supports only package names with simple version constraints",
            )
        name = _normalize_package_name(matched.group(1))
        try:
            platform_package(name)
        except ValueError:
            if name not in locked_names:
                _fail(
                    "invalid_lock",
                    field,
                    f"project requirement {name!r} must have an exact hashed lock entry",
                )
        else:
            project_platform_packages.add(name)
            if name not in declared_platform_packages:
                _fail(
                    "missing_reference",
                    field,
                    f"platform requirement {name!r} must be declared in platform_packages",
                )
    undeclared_requirements = declared_platform_packages - project_platform_packages
    if undeclared_requirements:
        package = min(undeclared_requirements)
        _fail(
            "missing_reference",
            f"{location}.project.dependencies",
            f"declared platform package {package!r} must be a project requirement",
        )


def _normalize_package_name(value: str) -> str:
    return re.sub(r"[-_.]+", "-", value).lower()


def _validate_node_lock(manifest: Path, lock: Path, location: str) -> None:
    try:
        package = json.loads(manifest.read_text(encoding="utf-8"))
        package_lock = json.loads(lock.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        _fail("invalid_lock", f"{location}.lock", str(error))
    if not isinstance(package, dict) or not isinstance(package_lock, dict):
        _fail("invalid_lock", f"{location}.lock", "Node manifests must be JSON objects")
    name = package.get("name")
    if not isinstance(name, str) or not name:
        _fail("invalid_lock", f"{location}.manifest", "package.json must declare name")
    if package_lock.get("lockfileVersion") != 3:
        _fail("invalid_lock", f"{location}.lock", "package-lock.json lockfileVersion must equal 3")
    if package_lock.get("name") != name:
        _fail("invalid_lock", f"{location}.lock", "package-lock name must match package.json")
    packages = package_lock.get("packages")
    if not isinstance(packages, dict) or not isinstance(packages.get(""), dict):
        _fail("invalid_lock", f"{location}.lock", "package-lock must declare the root package")
    if packages[""].get("name") != name:
        _fail("invalid_lock", f"{location}.lock", "root package name must match package.json")


def _tool_file(tool: Path, value: Any, field: str) -> Path:
    relative = _relative(value, field)
    current = tool
    for part in relative.parts:
        current = current / part
        try:
            mode = current.lstat().st_mode
        except FileNotFoundError:
            _fail("missing_file", field, "declared file does not exist")
        if stat.S_ISLNK(mode):
            _fail("unsafe_symlink", field, "declared path contains a symlink")
    if not current.is_file():
        _fail("invalid_type", field, "must be a file")
    return current


def _safe_file(root: Path, relative: Path) -> Path:
    current = root
    for part in relative.parts:
        current = current / part
        try:
            mode = current.lstat().st_mode
        except FileNotFoundError:
            _fail("missing_file", relative.as_posix(), "declared file does not exist")
        if stat.S_ISLNK(mode):
            _fail("unsafe_symlink", relative.as_posix(), "declared path contains a symlink")
    if not current.is_file():
        _fail("invalid_type", relative.as_posix(), "must be a file")
    return current


def _relative(value: Any, field: str) -> Path:
    if not isinstance(value, (str, os.PathLike)) or not os.fspath(value):
        _fail("invalid_type", field, "must be a non-empty relative path")
    path = Path(os.fspath(value))
    if path.is_absolute() or path == Path(".") or any(part in ("", ".", "..") for part in path.parts):
        _fail("invalid_path", field, "must be a normalized relative path")
    return path


def _identifier(value: Any, field: str) -> str:
    if not isinstance(value, str) or not _ID.fullmatch(value):
        _fail("invalid_type", field, "must be a stable identifier")
    return value


def _optional_positive_integer(value: Any, field: str) -> int | None:
    if value is None:
        return None
    if type(value) is not int or value <= 0:
        _fail("invalid_type", field, "must be a positive integer")
    return value


def _keys(table: dict[str, Any], allowed: set[str], location: str) -> None:
    unknown = sorted(set(table) - allowed)
    if unknown:
        _fail("invalid_field", f"{location}.{unknown[0]}", "field is not part of runtime v1")


def _toml(path: Path, location: str) -> dict[str, Any]:
    try:
        with path.open("rb") as handle:
            return tomllib.load(handle)
    except (OSError, tomllib.TOMLDecodeError) as error:
        _fail("invalid_toml", location, str(error))


def _fail(code: str, relative_path: str, message: str) -> None:
    raise ToolRuntimeManifestError(code, relative_path, message)
