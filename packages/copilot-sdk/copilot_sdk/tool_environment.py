"""Generic preparation and Runtime projection for declared Tool environments."""

from __future__ import annotations

import base64
import csv
import hashlib
import io
import json
import os
import re
import shutil
import signal
import stat
import subprocess
import tempfile
import time
import zipfile
from collections.abc import Callable, Mapping, Sequence
from contextlib import contextmanager
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Literal

import tomllib

try:  # pragma: no cover - the supported launcher platforms are POSIX.
    import fcntl
except ImportError:  # pragma: no cover
    fcntl = None

from .platform_packages import (
    ResolvedPlatformPackage,
    resolve_installed_platform_package,
)
from .tool_runtime_manifest import ToolRuntimeManifest, load_tool_runtime_manifest

PREPARED_DESCRIPTOR = Path("copilot-sdk/prepared-tools.v1.json")
OWNER_MARKER = ".copilot-tool-environment.json"
BUILD_DESCRIPTOR = Path(".copilot-tool-build.v1.json")
BUILD_DIRECTORY = Path("builds")
BUILD_STAGING_DIRECTORY = Path(".staging")
BUILD_LOCK_DIRECTORY = Path(".locks")
BUILD_CACHE_DIRECTORY = Path("cache")
PACKAGE_ID_PATTERN = re.compile(r"^[a-z0-9](?:[a-z0-9_-]{0,94}[a-z0-9])?$")
ADAPTER_FINGERPRINT_VERSION = "tool-environment-v2"
EXCLUDED_SOURCE_ENTRY_NAMES = frozenset(
    {
        ".codex",
        ".DS_Store",
        ".git",
        ".pytest_cache",
        ".ruff_cache",
        ".venv",
        "__pycache__",
        "build",
        "dist",
        "node_modules",
    }
)


class ToolEnvironmentError(RuntimeError):
    def __init__(self, code: str, path: str, message: str, cause: str | None = None) -> None:
        self.code = code
        self.path = path
        self.message = message
        self.cause = cause
        super().__init__(f"{code}: {path}: {message}")


@dataclass(frozen=True)
class ToolRuntimeSource:
    id: str
    root: Path
    runtime: Path
    source_root: Path | None = None


@dataclass(frozen=True)
class PreparedToolServer:
    id: str
    command: Path
    args: tuple[str, ...]
    env_bindings: tuple[dict[str, str], ...]
    startup_timeout_sec: int | None
    tool_timeout_sec: int | None


@dataclass(frozen=True)
class PreparedCapabilityRoot:
    id: str
    servers: tuple[PreparedToolServer, ...]


@dataclass(frozen=True)
class PreparedDelivery:
    id: str
    server: str
    tool: str
    kind: str
    schema: str
    mime_type: str
    display_name: str
    content_verifier: dict[str, Any] | None


@dataclass(frozen=True)
class PreparedToolComposition:
    descriptor_path: Path
    capability_roots: tuple[PreparedCapabilityRoot, ...]
    state: Literal["built", "reused"]
    deliveries: tuple[PreparedDelivery, ...] = ()


@dataclass(frozen=True)
class MaterializedToolServer:
    id: str
    command: Path
    args: tuple[str, ...]
    env: Mapping[str, str]
    env_vars: tuple[str, ...]
    startup_timeout_sec: int | None
    tool_timeout_sec: int | None


@dataclass(frozen=True)
class MaterializedCapabilityRoot:
    id: str
    projection_root: Path
    servers: tuple[MaterializedToolServer, ...]


@dataclass(frozen=True)
class MaterializedToolComposition:
    capability_roots: tuple[MaterializedCapabilityRoot, ...]


CommandRunner = Callable[[Sequence[str], Path, Mapping[str, str]], None]
ExecutableIdentityResolver = Callable[[str, Mapping[str, str]], tuple[Path, int, int]]
PlatformPackageResolver = Callable[[str], ResolvedPlatformPackage]


def prepare_tool_composition(
    *,
    source_root: Path,
    tools: Sequence[ToolRuntimeSource],
    output_root: Path,
    build_store_root: Path,
    composition_descriptor_sha256: str,
    deliveries: Sequence[PreparedDelivery] = (),
    host_environment: Mapping[str, str] | None = None,
    run_command: CommandRunner | None = None,
    executable_identity_resolver: ExecutableIdentityResolver | None = None,
    platform_package_resolver: PlatformPackageResolver | None = None,
) -> PreparedToolComposition:
    """Prepare dependencies in one shared immutable build store.

    ``output_root`` owns only the package-specific descriptor and marker. The
    actual dependency environments are keyed by fingerprint below
    ``build_store_root`` so multiple Copilots can reuse one verified build.
    """

    source_root = source_root.resolve(strict=True)
    environment = dict(os.environ if host_environment is None else host_environment)
    runner = _run_command if run_command is None else run_command
    identity_resolver = (
        _resolve_executable_identity
        if executable_identity_resolver is None
        else executable_identity_resolver
    )
    package_resolver = (
        resolve_installed_platform_package
        if platform_package_resolver is None
        else platform_package_resolver
    )
    declared = [
        (
            tool,
            load_tool_runtime_manifest(
                source_root if tool.source_root is None else tool.source_root,
                tool.root,
                tool.runtime,
            ),
        )
        for tool in tools
    ]
    executable_names = {
        "python3"
        for _, manifest in declared
        for dependency in manifest.dependencies
        if dependency.kind == "python-project"
    } | {
        "npm"
        for _, manifest in declared
        for dependency in manifest.dependencies
        if dependency.kind == "node-project"
    }
    identities = {
        name: identity_resolver(name, environment) for name in sorted(executable_names)
    }
    platform_package_ids = sorted(
        {
            package_id
            for _, manifest in declared
            for dependency in manifest.dependencies
            for package_id in dependency.platform_packages
        }
    )
    try:
        platform_packages = {
            package_id: package_resolver(package_id)
            for package_id in platform_package_ids
        }
    except ValueError as error:
        raise ToolEnvironmentError(
            "EnvironmentUnavailable",
            "platform_packages",
            "a declared platform package is not installed in the Copilot SDK environment",
        ) from error
    fingerprint = _preparation_fingerprint(declared, identities, platform_packages)
    output_root, existing_marker = _inspect_owned_output_root(output_root, source_root)
    build_store_root = _absolute_directory(build_store_root, "build store root")
    if (
        existing_marker is not None
        and existing_marker.get("preparationFingerprint") == fingerprint
    ):
        reused = _load_prepared_composition(
            output_root / PREPARED_DESCRIPTOR,
            existing_marker.get("compositionDescriptorSha256"),
        )
        if reused is not None:
            try:
                _validate_roots_in_build_store(reused.capability_roots, build_store_root, fingerprint)
            except ToolEnvironmentError:
                if not _roots_are_legacy_output(reused.capability_roots, output_root, fingerprint):
                    raise
                # An owned descriptor from the pre-shared-store layout is a
                # cache miss. It is never used as a runtime fallback.
                reused = None
        if reused is not None:
            _write_prepared_descriptor(
                output_root / PREPARED_DESCRIPTOR,
                composition_descriptor_sha256,
                reused.capability_roots,
                deliveries,
            )
            _write_owner_marker(output_root, source_root, composition_descriptor_sha256, fingerprint)
            return PreparedToolComposition(
                output_root / PREPARED_DESCRIPTOR,
                reused.capability_roots,
                "reused",
                tuple(deliveries),
            )

    _, prepared_roots, build_state = _ensure_shared_build(
        declared=declared,
        identities=identities,
        platform_packages=platform_packages,
        fingerprint=fingerprint,
        build_store_root=build_store_root,
        environment=environment,
        runner=runner,
    )
    descriptor_path = output_root / PREPARED_DESCRIPTOR
    if existing_marker is None:
        # A fresh output root has no ownership marker yet. Publish the marker
        # first so a descriptor-write interruption remains repairable on the
        # next invocation without accepting an unowned directory.
        _write_owner_marker(output_root, source_root, composition_descriptor_sha256, fingerprint)
        _write_prepared_descriptor(
            descriptor_path,
            composition_descriptor_sha256,
            tuple(prepared_roots),
            deliveries,
        )
    else:
        # For an owned root, keep the old marker until the new descriptor has
        # been atomically written. A marker failure then leaves a recoverable
        # old/new mismatch instead of two stale claims.
        _write_prepared_descriptor(
            descriptor_path,
            composition_descriptor_sha256,
            tuple(prepared_roots),
            deliveries,
        )
        _write_owner_marker(output_root, source_root, composition_descriptor_sha256, fingerprint)
    return PreparedToolComposition(
        descriptor_path, tuple(prepared_roots), build_state, tuple(deliveries)
    )


def _roots_are_legacy_output(
    roots: Sequence[PreparedCapabilityRoot], output_root: Path, fingerprint: str
) -> bool:
    legacy_root = output_root / BUILD_DIRECTORY / fingerprint
    paths: list[Path] = []
    for root in roots:
        for server in root.servers:
            paths.append(server.command)
            paths.extend(
                Path(binding["resolvedRoot"])
                for binding in server.env_bindings
                if "resolvedRoot" in binding
            )
    return bool(paths) and all(
        path.is_absolute() and path.is_relative_to(legacy_root) for path in paths
    )


def _ensure_shared_build(
    *,
    declared: Sequence[tuple[ToolRuntimeSource, ToolRuntimeManifest]],
    identities: Mapping[str, tuple[Path, int, int]],
    platform_packages: Mapping[str, ResolvedPlatformPackage],
    fingerprint: str,
    build_store_root: Path,
    environment: Mapping[str, str],
    runner: CommandRunner,
) -> tuple[Path, tuple[PreparedCapabilityRoot, ...], Literal["built", "reused"]]:
    builds_root = build_store_root / BUILD_DIRECTORY
    staging_root = build_store_root / BUILD_STAGING_DIRECTORY
    builds_root.mkdir(parents=True, exist_ok=True)
    staging_root.mkdir(parents=True, exist_ok=True)
    target = builds_root / fingerprint
    lock_path = build_store_root / BUILD_LOCK_DIRECTORY / f"{fingerprint}.lock"
    with _build_lock(lock_path):
        if os.path.lexists(target):
            if target.is_symlink() or not target.is_dir():
                raise ToolEnvironmentError(
                    "OutputConflict", str(target), "shared build target must be a directory"
                )
            prepared_roots = _load_build_manifest(target, fingerprint)
            return target, prepared_roots, "reused"

        staging = Path(tempfile.mkdtemp(prefix=f"{fingerprint}-", dir=staging_root))
        try:
            prepared_roots: list[PreparedCapabilityRoot] = []
            cache_root = build_store_root / BUILD_CACHE_DIRECTORY
            cache_root.mkdir(parents=True, exist_ok=True)
            for tool, manifest in declared:
                prepared_roots.append(
                    _prepare_tool(
                        tool,
                        manifest,
                        staging,
                        cache_root,
                        environment,
                        runner,
                        {name: identity[0] for name, identity in identities.items()},
                        platform_packages,
                    )
                )
            _validate_roots_under_root(prepared_roots, staging, require_exists=True)
            final_roots = _rewrite_prepared_roots(tuple(prepared_roots), staging, target)
            _write_build_manifest(staging, fingerprint, final_roots)
            _validate_roots_under_root(final_roots, target, require_exists=False)
            try:
                os.replace(staging, target)
            except OSError as error:
                # A process that did not share the lock may have published the
                # winner between our check and rename. Never overwrite it.
                if not os.path.lexists(target) or target.is_symlink() or not target.is_dir():
                    raise
                try:
                    winner = _load_build_manifest(target, fingerprint)
                except ToolEnvironmentError:
                    raise error
                shutil.rmtree(staging, ignore_errors=True)
                return target, winner, "reused"
            return target, final_roots, "built"
        except Exception:
            shutil.rmtree(staging, ignore_errors=True)
            raise


@contextmanager
def _build_lock(path: Path):
    path.parent.mkdir(parents=True, exist_ok=True)
    if fcntl is not None:
        with path.open("a+") as handle:
            fcntl.flock(handle.fileno(), fcntl.LOCK_EX)
            try:
                yield
            finally:
                fcntl.flock(handle.fileno(), fcntl.LOCK_UN)
        return

    # Windows has no fcntl. Use an atomic mkdir lock with a bounded wait so
    # concurrent losers still observe and reuse the winner's build.
    lock_directory = path.with_suffix(path.suffix + ".d")
    for _ in range(300):
        try:
            lock_directory.mkdir()
            break
        except FileExistsError:
            time.sleep(0.01)
    else:
        raise ToolEnvironmentError("EnvironmentUnavailable", str(path), "shared build lock timed out")
    try:
        yield
    finally:
        lock_directory.rmdir()


def _rewrite_prepared_roots(
    roots: Sequence[PreparedCapabilityRoot], old_root: Path, new_root: Path
) -> tuple[PreparedCapabilityRoot, ...]:
    def rewrite(path: Path) -> Path:
        try:
            relative = path.relative_to(old_root)
        except ValueError as error:
            raise ToolEnvironmentError(
                "UnsafePath", str(path), "prepared path escapes the staging build"
            ) from error
        return new_root / relative

    return tuple(
        PreparedCapabilityRoot(
            root.id,
            tuple(
                PreparedToolServer(
                    server.id,
                    rewrite(server.command),
                    server.args,
                    tuple(
                        {
                            **binding,
                            **(
                                {"resolvedRoot": str(rewrite(Path(binding["resolvedRoot"])))}
                                if "resolvedRoot" in binding
                                else {}
                            ),
                        }
                        for binding in server.env_bindings
                    ),
                    server.startup_timeout_sec,
                    server.tool_timeout_sec,
                )
                for server in root.servers
            ),
        )
        for root in roots
    )


def _validate_roots_in_build_store(
    roots: Sequence[PreparedCapabilityRoot], build_store_root: Path, fingerprint: str
) -> None:
    build_root = build_store_root / BUILD_DIRECTORY / fingerprint
    _validate_roots_under_root(roots, build_root, require_exists=True)


def _validate_roots_under_root(
    roots: Sequence[PreparedCapabilityRoot], build_root: Path, *, require_exists: bool
) -> None:
    try:
        canonical_build_root = build_root.resolve(strict=False)
    except OSError as error:
        raise ToolEnvironmentError("UnsafePath", str(build_root), "build root is invalid") from error
    for root in roots:
        if not root.id or Path(root.id).name != root.id:
            raise ToolEnvironmentError("UnsafePath", str(build_root), "capability root id is invalid")
        for server in root.servers:
            _validate_owned_build_path(
                server.command,
                canonical_build_root,
                require_exists=require_exists,
                expect_directory=False,
            )
            for binding in server.env_bindings:
                resolved = binding.get("resolvedRoot")
                if resolved is not None:
                    _validate_owned_build_path(
                        Path(resolved),
                        canonical_build_root,
                        require_exists=require_exists,
                        expect_directory=True,
                    )


def _validate_owned_build_path(
    path: Path,
    build_root: Path,
    *,
    require_exists: bool,
    expect_directory: bool,
) -> None:
    if not path.is_absolute():
        raise ToolEnvironmentError("UnsafePath", str(path), "build reference must be absolute")
    try:
        relative = path.relative_to(build_root)
    except ValueError as error:
        raise ToolEnvironmentError("UnsafePath", str(path), "build reference escapes build root") from error
    current = build_root
    for component in relative.parts:
        current = current / component
        if os.path.lexists(current):
            mode = current.lstat().st_mode
            if stat.S_ISLNK(mode):
                raise ToolEnvironmentError("UnsafePath", str(current), "build reference contains a symlink")
    if require_exists:
        if not path.is_file() and not expect_directory:
            raise ToolEnvironmentError("EnvironmentUnavailable", str(path), "build command is missing")
        if not path.is_dir() and expect_directory:
            raise ToolEnvironmentError("EnvironmentUnavailable", str(path), "build dependency root is missing")


def _write_build_manifest(
    build_root: Path, fingerprint: str, capability_roots: Sequence[PreparedCapabilityRoot]
) -> None:
    _write_json_atomic(
        build_root / BUILD_DESCRIPTOR,
        {
            "schemaVersion": 1,
            "preparationFingerprint": fingerprint,
            "capabilityRoots": [_descriptor_root(root) for root in capability_roots],
        },
    )


def _load_build_manifest(build_root: Path, fingerprint: str) -> tuple[PreparedCapabilityRoot, ...]:
    descriptor = build_root / BUILD_DESCRIPTOR
    if descriptor.is_symlink() or not descriptor.is_file():
        raise ToolEnvironmentError("OutputConflict", str(descriptor), "shared build marker is missing")
    try:
        payload = json.loads(descriptor.read_text(encoding="utf-8"))
        if (
            payload.get("schemaVersion") != 1
            or payload.get("preparationFingerprint") != fingerprint
        ):
            raise ValueError("shared build marker does not match fingerprint")
        roots = _parse_capability_roots(payload)
        _validate_roots_under_root(roots, build_root, require_exists=True)
        return roots
    except (OSError, UnicodeError, json.JSONDecodeError, KeyError, TypeError, ValueError) as error:
        if isinstance(error, ToolEnvironmentError):
            raise
        raise ToolEnvironmentError("OutputConflict", str(descriptor), "shared build marker is malformed") from error


def _prepare_tool(
    source: ToolRuntimeSource,
    manifest: ToolRuntimeManifest,
    build_root: Path,
    cache_root: Path,
    host_environment: Mapping[str, str],
    run_command: CommandRunner,
    executables: Mapping[str, Path],
    platform_packages: Mapping[str, ResolvedPlatformPackage],
) -> PreparedCapabilityRoot:
    tool_root = source.root.resolve(strict=True)
    managed_root = build_root / "tool-environments" / source.id
    dependency_roots: dict[str, Path] = {}
    python_commands: dict[str, Path] = {}
    for dependency in manifest.dependencies:
        dependency_root = managed_root / "dependencies" / dependency.id
        dependency_root.mkdir(parents=True, exist_ok=True)
        dependency_roots[dependency.id] = dependency_root
        if dependency.kind == "python-project":
            python_commands[dependency.id] = _prepare_python_dependency(
                tool_root,
                dependency.manifest,
                dependency.lock,
                dependency_root,
                cache_root,
                run_command,
                host_environment,
                executables["python3"],
                tuple(platform_packages[item] for item in dependency.platform_packages),
            )
        elif dependency.kind == "node-project":
            _prepare_node_dependency(
                dependency.manifest,
                dependency.lock,
                dependency_root,
                cache_root,
                run_command,
                host_environment,
                executables["npm"],
            )
        else:  # pragma: no cover - parser rejects unknown kinds
            raise AssertionError(dependency.kind)

    servers: list[PreparedToolServer] = []
    for server in manifest.servers:
        python = python_commands[server.entry.dependency]
        args = ("-m", server.entry.module, *server.args)
        bindings: list[dict[str, str]] = []
        for binding in server.env:
            item = {"name": binding.name, "source": binding.source}
            if binding.source == "dependency_root":
                assert binding.dependency is not None
                item["dependency"] = binding.dependency
                item["resolvedRoot"] = str(dependency_roots[binding.dependency])
            bindings.append(item)
        prepared = PreparedToolServer(
            id=server.id,
            command=python,
            args=tuple(args),
            env_bindings=tuple(bindings),
            startup_timeout_sec=server.startup_timeout_sec,
            tool_timeout_sec=server.tool_timeout_sec,
        )
        servers.append(prepared)
    return PreparedCapabilityRoot(source.id, tuple(servers))


def _prepare_python_dependency(
    tool_root: Path,
    manifest: Path,
    lock: Path,
    dependency_root: Path,
    cache_root: Path,
    run_command: CommandRunner,
    host_environment: Mapping[str, str],
    python: Path,
    platform_packages: Sequence[ResolvedPlatformPackage],
) -> Path:
    venv = dependency_root / "venv"
    run_command(
        (str(python), "-m", "venv", "--copies", str(venv)),
        tool_root,
        host_environment,
    )
    venv_python = venv / ("Scripts/python.exe" if os.name == "nt" else "bin/python")
    pip_env = dict(host_environment)
    pip_env.update({"PIP_CACHE_DIR": str(cache_root / "pip"), "TMPDIR": str(cache_root / "tmp")})
    (cache_root / "pip").mkdir(parents=True, exist_ok=True)
    (cache_root / "tmp").mkdir(parents=True, exist_ok=True)
    run_command(
        (str(venv_python), "-m", "pip", "install", "--require-hashes", "-r", str(lock)),
        tool_root,
        pip_env,
    )
    platform_wheel_dir = dependency_root / "platform-wheels"
    platform_wheel_dir.mkdir(parents=True, exist_ok=True)
    for package in platform_packages:
        wheel = _build_platform_package_wheel(package, platform_wheel_dir)
        run_command(
            (str(venv_python), "-m", "pip", "install", "--no-deps", str(wheel)),
            tool_root,
            pip_env,
        )
    build_source = Path(tempfile.mkdtemp(prefix="tool-build-", dir=cache_root / "tmp"))
    wheel_dir = dependency_root / "wheel"
    wheel_dir.mkdir(parents=True, exist_ok=True)
    try:
        _copy_source_tree(tool_root, build_source)
        run_command(
            (
                str(venv_python),
                "-m",
                "pip",
                "wheel",
                "--no-deps",
                "--no-build-isolation",
                "--wheel-dir",
                str(wheel_dir),
                str(build_source),
            ),
            build_source,
            pip_env,
        )
        wheels = sorted(wheel_dir.glob("*.whl"))
        if len(wheels) != 1:
            raise ToolEnvironmentError("EnvironmentUnavailable", str(manifest), "Tool build must produce exactly one wheel")
        run_command(
            (str(venv_python), "-m", "pip", "install", "--no-deps", str(wheels[0])),
            tool_root,
            pip_env,
        )
        run_command((str(venv_python), "-m", "pip", "check"), tool_root, pip_env)
    finally:
        shutil.rmtree(build_source, ignore_errors=True)
    return venv_python


def _prepare_node_dependency(
    manifest: Path,
    lock: Path,
    dependency_root: Path,
    cache_root: Path,
    run_command: CommandRunner,
    host_environment: Mapping[str, str],
    npm: Path,
) -> None:
    shutil.copy2(manifest, dependency_root / "package.json")
    shutil.copy2(lock, dependency_root / "package-lock.json")
    env = dict(host_environment)
    env["npm_config_cache"] = str(cache_root / "npm")
    (cache_root / "npm").mkdir(parents=True, exist_ok=True)
    run_command(
        (str(npm), "ci", "--prefer-offline", "--no-audit", "--no-fund", "--ignore-scripts"),
        dependency_root,
        env,
    )


def _copy_source_tree(source: Path, destination: Path) -> None:
    destination.mkdir(parents=True, exist_ok=True)
    for entry in source.iterdir():
        if _excluded_source_entry(entry):
            continue
        mode = entry.lstat().st_mode
        if stat.S_ISLNK(mode):
            raise ToolEnvironmentError("UnsafePath", str(entry), "Tool source contains a symlink")
        target = destination / entry.name
        if entry.is_dir():
            _copy_source_tree(entry, target)
        elif entry.is_file():
            shutil.copy2(entry, target)


def materialize_capability_roots(
    prepared: PreparedToolComposition,
    *,
    profile_home: Path,
    state_root: Path,
) -> MaterializedToolComposition:
    """Resolve Profile-scoped bindings and create disposable Runtime projections."""

    profile_home = _absolute_directory(profile_home, "Profile home")
    state_root = _absolute_directory(state_root, "Tool state root")
    roots: list[MaterializedCapabilityRoot] = []
    for root in prepared.capability_roots:
        tool_state_root = state_root / "tool-state" / root.id
        tool_state_root.mkdir(parents=True, exist_ok=True)
        projection_root = state_root / "capability-roots" / root.id
        if projection_root.exists():
            shutil.rmtree(projection_root)
        (projection_root / ".codex-plugin").mkdir(parents=True)
        servers: list[MaterializedToolServer] = []
        projection_servers: dict[str, Any] = {}
        for server in root.servers:
            literal_env: dict[str, str] = {}
            inherited: list[str] = []
            for binding in server.env_bindings:
                name = binding["name"]
                source = binding["source"]
                if source == "profile_home":
                    literal_env[name] = str(profile_home)
                elif source == "tool_state_root":
                    literal_env[name] = str(tool_state_root)
                elif source == "dependency_root":
                    literal_env[name] = binding["resolvedRoot"]
                else:
                    inherited.append(name)
            materialized = MaterializedToolServer(
                id=server.id,
                command=server.command,
                args=server.args,
                env=literal_env,
                env_vars=tuple(inherited),
                startup_timeout_sec=server.startup_timeout_sec,
                tool_timeout_sec=server.tool_timeout_sec,
            )
            servers.append(materialized)
            transport: dict[str, Any] = {
                "command": str(server.command),
                "args": list(server.args),
                "env": literal_env,
                "env_vars": inherited,
            }
            if server.startup_timeout_sec is not None:
                transport["startup_timeout_sec"] = server.startup_timeout_sec
            if server.tool_timeout_sec is not None:
                transport["tool_timeout_sec"] = server.tool_timeout_sec
            projection_servers[server.id] = transport
        (projection_root / ".codex-plugin" / "plugin.json").write_text(
            json.dumps(
                {
                    "name": root.id,
                    "version": "0.0.0",
                    "mcpServers": "./.mcp.json",
                    "interface": {"displayName": root.id},
                },
                indent=2,
            )
            + "\n",
            encoding="utf-8",
        )
        (projection_root / ".mcp.json").write_text(
            json.dumps({"mcpServers": projection_servers}, indent=2) + "\n",
            encoding="utf-8",
        )
        roots.append(MaterializedCapabilityRoot(root.id, projection_root, tuple(servers)))
    return MaterializedToolComposition(tuple(roots))


def _run_command(command: Sequence[str], cwd: Path, environment: Mapping[str, str]) -> None:
    try:
        process = subprocess.Popen(
            list(command), cwd=cwd, env=dict(environment), text=True,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            start_new_session=os.name != "nt",
        )
        try:
            stdout, stderr = process.communicate(timeout=180)
        except subprocess.TimeoutExpired:
            if os.name == "nt":
                process.kill()
            else:
                os.killpg(process.pid, signal.SIGKILL)
            stdout, stderr = process.communicate()
            raise ToolEnvironmentError(
                "EnvironmentUnavailable", str(cwd), "environment preparation timed out"
            )
    except OSError as error:
        raise ToolEnvironmentError("EnvironmentUnavailable", str(cwd), "environment preparation failed", str(error)) from error
    if process.returncode != 0:
        cause = (stderr or stdout)[-2000:]
        raise ToolEnvironmentError("EnvironmentUnavailable", str(cwd), f"environment preparation exited with status {process.returncode}", cause)


def _absolute_directory(path: Path, label: str) -> Path:
    path = Path(path)
    if not path.is_absolute():
        raise ToolEnvironmentError("InvalidPath", str(path), f"{label} must be absolute")
    if os.path.lexists(path) and path.is_symlink():
        raise ToolEnvironmentError("UnsafePath", str(path), f"{label} must not be a symlink")
    path.mkdir(parents=True, exist_ok=True)
    return path.resolve(strict=True)


def _inspect_owned_output_root(
    path: Path, source_root: Path
) -> tuple[Path, dict[str, Any] | None]:
    path = Path(path)
    if not path.is_absolute():
        raise ToolEnvironmentError("InvalidPath", str(path), "output root must be absolute")
    if os.path.lexists(path):
        mode = path.lstat().st_mode
        if stat.S_ISLNK(mode) or not stat.S_ISDIR(mode):
            raise ToolEnvironmentError("OutputConflict", str(path), "output root must be a directory")
        entries = list(path.iterdir())
        if entries:
            marker = path / OWNER_MARKER
            if not marker.is_file() or marker.is_symlink():
                raise ToolEnvironmentError("OutputConflict", str(path), "output root is not owned by this composition")
            descriptor = path / PREPARED_DESCRIPTOR
            if descriptor.is_symlink():
                raise ToolEnvironmentError("UnsafePath", str(descriptor), "prepared descriptor is a symlink")
            try:
                actual = json.loads(marker.read_text(encoding="utf-8"))
            except (OSError, UnicodeError, json.JSONDecodeError) as error:
                raise ToolEnvironmentError("OutputConflict", str(path), "output owner marker is invalid", str(error)) from error
            if actual.get("schemaVersion") != 1 or actual.get("sourceRoot") != str(source_root):
                raise ToolEnvironmentError("OutputConflict", str(path), "output root belongs to a different composition")
            return path.resolve(strict=True), actual
    else:
        path.mkdir(parents=True)
    return path.resolve(strict=True), None


def _write_owner_marker(
    root: Path,
    source_root: Path,
    composition_descriptor_sha256: str,
    fingerprint: str,
) -> None:
    expected = {
        "schemaVersion": 1,
        "sourceRoot": str(source_root),
        "compositionDescriptorSha256": composition_descriptor_sha256,
        "preparationFingerprint": fingerprint,
    }
    _write_json_atomic(root / OWNER_MARKER, expected)


def _resolve_executable_identity(
    name: str, environment: Mapping[str, str]
) -> tuple[Path, int, int]:
    host_path = environment.get("PATH")
    resolved = shutil.which(name, path=host_path) if host_path else None
    if resolved is None:
        raise ToolEnvironmentError("EnvironmentUnavailable", name, f"{name} is required")
    path = Path(resolved).resolve(strict=True)
    identity = path.stat()
    return path, identity.st_size, identity.st_mtime_ns


def _preparation_fingerprint(
    declared: Sequence[tuple[ToolRuntimeSource, ToolRuntimeManifest]],
    identities: Mapping[str, tuple[Path, int, int]],
    platform_packages: Mapping[str, ResolvedPlatformPackage],
) -> str:
    digest = hashlib.sha256()
    _digest_item(digest, ADAPTER_FINGERPRINT_VERSION.encode())
    for source, _ in declared:
        _digest_item(digest, source.id.encode())
        for relative, contents in _source_regular_files(source.root.resolve(strict=True)):
            _digest_item(digest, relative.as_posix().encode())
            _digest_item(digest, contents)
    for name, (path, size, mtime_ns) in sorted(identities.items()):
        _digest_item(digest, name.encode())
        _digest_item(digest, str(path).encode())
        _digest_item(digest, f"{size}:{mtime_ns}".encode())
    for package_id, package in sorted(platform_packages.items()):
        _digest_item(digest, package_id.encode())
        _digest_item(digest, package.spec.distribution.encode())
        _digest_item(digest, package.version.encode())
        for root in package.package_roots:
            for relative, contents in _source_regular_files(root):
                _digest_item(digest, f"{root.name}/{relative.as_posix()}".encode())
                _digest_item(digest, contents)
    return digest.hexdigest()


def _build_platform_package_wheel(
    package: ResolvedPlatformPackage,
    wheel_dir: Path,
) -> Path:
    """Repack one trusted installed pure-Python distribution for a managed Tool venv."""

    wheel_name = package.spec.distribution.replace("-", "_")
    wheel_path = wheel_dir / f"{wheel_name}-{package.version}-py3-none-any.whl"
    dist_info = f"{wheel_name}-{package.version}.dist-info"
    files: dict[str, bytes] = {}
    for root in package.package_roots:
        for relative, contents in _source_regular_files(root):
            files[f"{root.name}/{relative.as_posix()}"] = contents
    files[f"{dist_info}/METADATA"] = (
        "Metadata-Version: 2.1\n"
        f"Name: {package.spec.distribution}\n"
        f"Version: {package.version}\n"
        "Requires-Python: >=3.11\n\n"
    ).encode()
    files[f"{dist_info}/WHEEL"] = (
        b"Wheel-Version: 1.0\n"
        b"Generator: open-web-codex-copilot-sdk\n"
        b"Root-Is-Purelib: true\n"
        b"Tag: py3-none-any\n\n"
    )
    record_path = f"{dist_info}/RECORD"
    rows: list[tuple[str, str, str]] = []
    for path, contents in sorted(files.items()):
        digest = base64.urlsafe_b64encode(hashlib.sha256(contents).digest()).rstrip(b"=").decode()
        rows.append((path, f"sha256={digest}", str(len(contents))))
    rows.append((record_path, "", ""))
    record = io.StringIO(newline="")
    csv.writer(record, lineterminator="\n").writerows(rows)
    files[record_path] = record.getvalue().encode()
    temporary = wheel_path.with_suffix(".tmp")
    with zipfile.ZipFile(temporary, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for path, contents in sorted(files.items()):
            archive.writestr(path, contents)
    temporary.replace(wheel_path)
    return wheel_path


def _source_regular_files(root: Path) -> list[tuple[Path, bytes]]:
    files: list[tuple[Path, bytes]] = []

    def visit(directory: Path) -> None:
        for entry in sorted(directory.iterdir(), key=lambda item: item.name):
            if _excluded_source_entry(entry):
                continue
            mode = entry.lstat().st_mode
            if stat.S_ISLNK(mode):
                raise ToolEnvironmentError("UnsafePath", str(entry), "Tool source contains a symlink")
            if entry.is_dir():
                visit(entry)
            elif entry.is_file():
                files.append((entry.relative_to(root), entry.read_bytes()))

    visit(root)
    return files


def _excluded_source_entry(entry: Path) -> bool:
    """Apply one bounded author-source contract to fingerprints and staged builds."""

    return entry.name in EXCLUDED_SOURCE_ENTRY_NAMES or entry.name.endswith(".pyc")


def _digest_item(digest: Any, value: bytes) -> None:
    digest.update(len(value).to_bytes(8, "big"))
    digest.update(value)


def _parse_capability_roots(payload: Mapping[str, Any]) -> tuple[PreparedCapabilityRoot, ...]:
    raw_roots = payload.get("capabilityRoots")
    if not isinstance(raw_roots, list):
        raise TypeError("capabilityRoots must be a list")
    roots: list[PreparedCapabilityRoot] = []
    for raw_root in raw_roots:
        if not isinstance(raw_root, dict) or set(raw_root) != {"id", "servers"}:
            raise TypeError("capability root descriptor is malformed")
        root_id = raw_root["id"]
        raw_servers = raw_root["servers"]
        if not isinstance(root_id, str) or not isinstance(raw_servers, list):
            raise TypeError("capability root descriptor is malformed")
        servers: list[PreparedToolServer] = []
        for raw_server in raw_servers:
            if not isinstance(raw_server, dict):
                raise TypeError("server descriptor is malformed")
            allowed = {
                "id",
                "transport",
                "command",
                "args",
                "envBindings",
                "startupTimeoutSec",
                "toolTimeoutSec",
            }
            if set(raw_server) - allowed or raw_server.get("transport") != "stdio":
                raise TypeError("server descriptor is malformed")
            server_id = raw_server.get("id")
            command = raw_server.get("command")
            args = raw_server.get("args")
            bindings = raw_server.get("envBindings", [])
            if (
                not isinstance(server_id, str)
                or not isinstance(command, str)
                or not isinstance(args, list)
                or not all(isinstance(item, str) for item in args)
                or not isinstance(bindings, list)
            ):
                raise TypeError("server descriptor is malformed")
            normalized_bindings: list[dict[str, str]] = []
            for binding in bindings:
                if not isinstance(binding, dict):
                    raise TypeError("environment binding is malformed")
                if set(binding) - {"name", "source", "dependency", "resolvedRoot"}:
                    raise TypeError("environment binding is malformed")
                if not isinstance(binding.get("name"), str) or not isinstance(
                    binding.get("source"), str
                ):
                    raise TypeError("environment binding is malformed")
                if binding["source"] not in {
                    "profile_home",
                    "tool_state_root",
                    "dependency_root",
                    "host",
                }:
                    raise ValueError("environment binding source is invalid")
                normalized = {"name": binding["name"], "source": binding["source"]}
                for key in ("dependency", "resolvedRoot"):
                    if key in binding:
                        if not isinstance(binding[key], str):
                            raise TypeError("environment binding is malformed")
                        normalized[key] = binding[key]
                if normalized["source"] == "dependency_root" and "resolvedRoot" not in normalized:
                    raise ValueError("dependency binding has no resolved root")
                normalized_bindings.append(normalized)
            startup_timeout = raw_server.get("startupTimeoutSec")
            tool_timeout = raw_server.get("toolTimeoutSec")
            if startup_timeout is not None and (
                not isinstance(startup_timeout, int) or isinstance(startup_timeout, bool)
            ):
                raise TypeError("server timeout is malformed")
            if tool_timeout is not None and (
                not isinstance(tool_timeout, int) or isinstance(tool_timeout, bool)
            ):
                raise TypeError("server timeout is malformed")
            servers.append(
                PreparedToolServer(
                    id=server_id,
                    command=Path(command),
                    args=tuple(args),
                    env_bindings=tuple(normalized_bindings),
                    startup_timeout_sec=startup_timeout,
                    tool_timeout_sec=tool_timeout,
                )
            )
        roots.append(PreparedCapabilityRoot(root_id, tuple(servers)))
    return tuple(roots)


def _load_prepared_composition(
    descriptor_path: Path,
    expected_composition_descriptor_sha256: object,
) -> PreparedToolComposition | None:
    if descriptor_path.is_symlink():
        raise ToolEnvironmentError("UnsafePath", str(descriptor_path), "prepared descriptor is a symlink")
    try:
        payload = json.loads(descriptor_path.read_text(encoding="utf-8"))
        if payload.get("schemaVersion") != 1:
            return None
        if (
            not isinstance(expected_composition_descriptor_sha256, str)
            or payload.get("compositionDescriptorSha256")
            != expected_composition_descriptor_sha256
        ):
            return None
        roots = _parse_capability_roots(payload)
        for root in roots:
            for server in root.servers:
                if not server.command.is_file() or server.command.is_symlink():
                    return None
                for binding in server.env_bindings:
                    resolved = binding.get("resolvedRoot")
                    if resolved is not None and (
                        not Path(resolved).is_dir() or Path(resolved).is_symlink()
                    ):
                        return None
        deliveries = tuple(
            PreparedDelivery(
                id=item["id"],
                server=item["server"],
                tool=item["tool"],
                kind=item["kind"],
                schema=item["schema"],
                mime_type=item["mimeType"],
                display_name=item["displayName"],
                content_verifier=item.get("contentVerifier"),
            )
            for item in payload.get("deliveries", [])
        )
        return PreparedToolComposition(descriptor_path, roots, "reused", deliveries)
    except (OSError, UnicodeError, json.JSONDecodeError, KeyError, TypeError):
        return None


def _write_prepared_descriptor(
    descriptor_path: Path,
    composition_descriptor_sha256: str,
    capability_roots: Sequence[PreparedCapabilityRoot],
    deliveries: Sequence[PreparedDelivery],
) -> None:
    descriptor_path.parent.mkdir(parents=True, exist_ok=True)
    payload = {
        "schemaVersion": 1,
        "compositionDescriptorSha256": composition_descriptor_sha256,
        "capabilityRoots": [_descriptor_root(root) for root in capability_roots],
        "deliveries": [
            {
                "id": delivery.id,
                "server": delivery.server,
                "tool": delivery.tool,
                "kind": delivery.kind,
                "schema": delivery.schema,
                "mimeType": delivery.mime_type,
                "displayName": delivery.display_name,
                **(
                    {"contentVerifier": delivery.content_verifier}
                    if delivery.content_verifier is not None
                    else {}
                ),
            }
            for delivery in deliveries
        ],
    }
    _write_json_atomic(descriptor_path, payload, ensure_ascii=False)


def _write_json_atomic(path: Path, payload: Mapping[str, Any], *, ensure_ascii: bool = True) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor_fd, descriptor_name = tempfile.mkstemp(
        prefix=f".{path.name}.", suffix=".tmp", dir=path.parent
    )
    os.close(descriptor_fd)
    descriptor_temp = Path(descriptor_name)
    try:
        descriptor_temp.write_text(
            json.dumps(payload, ensure_ascii=ensure_ascii, sort_keys=True, indent=2) + "\n",
            encoding="utf-8",
        )
        os.replace(descriptor_temp, path)
    finally:
        descriptor_temp.unlink(missing_ok=True)


def garbage_collect_tool_builds(
    *,
    prepared_root: Path,
    build_store_root: Path,
    packages_root: Path,
    active_package_ids: Sequence[str],
) -> dict[str, int]:
    """Collect only builds referenced by the current authoritative package set."""

    prepared_root = _absolute_directory(prepared_root, "prepared root")
    build_store_root = _absolute_directory(build_store_root, "build store root")
    packages_root = _absolute_directory(packages_root, "packages root")
    active_ids = _validated_active_package_ids(active_package_ids)
    current_packages = _current_package_sources(packages_root)
    missing_current = sorted(set(active_ids) - set(current_packages))
    if missing_current:
        raise ToolEnvironmentError(
            "OutputConflict",
            str(packages_root),
            f"active package manifest is missing: {', '.join(missing_current)}",
        )

    builds_root = build_store_root / BUILD_DIRECTORY
    builds_root.mkdir(parents=True, exist_ok=True)
    referenced: set[str] = set()
    active_package_roots: list[Path] = []
    legacy_removed_entries = 0
    legacy_removed_bytes = 0

    # Validate every active package before changing any package or build path.
    for package_id in active_ids:
        package_root = prepared_root / package_id
        if package_root.is_symlink() or not package_root.is_dir():
            raise ToolEnvironmentError("UnsafePath", str(package_root), "active package output is unsafe")
        descriptor_path = package_root / PREPARED_DESCRIPTOR
        marker_path = package_root / OWNER_MARKER
        if descriptor_path.is_symlink() or marker_path.is_symlink():
            raise ToolEnvironmentError("UnsafePath", str(package_root), "active package marker or descriptor is a symlink")
        if not descriptor_path.is_file() or not marker_path.is_file():
            raise ToolEnvironmentError("OutputConflict", str(package_root), "active package output is incomplete")
        try:
            payload = json.loads(descriptor_path.read_text(encoding="utf-8"))
            marker = json.loads(marker_path.read_text(encoding="utf-8"))
            composition_sha = payload.get("compositionDescriptorSha256")
            fingerprint = marker.get("preparationFingerprint")
            if (
                payload.get("schemaVersion") != 1
                or not isinstance(composition_sha, str)
                or len(composition_sha) != 64
                or any(char not in "0123456789abcdef" for char in composition_sha)
                or marker.get("schemaVersion") != 1
                or not isinstance(fingerprint, str)
                or len(fingerprint) != 64
                or any(char not in "0123456789abcdef" for char in fingerprint)
            ):
                raise ValueError("active package marker or descriptor schema is invalid")
            source_root = marker.get("sourceRoot")
            if not isinstance(source_root, str) or Path(source_root).resolve(strict=True) != current_packages[package_id]:
                raise ValueError("active package source root is not authoritative")
            roots = _parse_capability_roots(payload)
            build_root = builds_root / fingerprint
            _validate_roots_under_root(roots, build_root, require_exists=True)
            _load_build_manifest(build_root, fingerprint)
        except ToolEnvironmentError:
            raise
        except (OSError, UnicodeError, json.JSONDecodeError, KeyError, TypeError, ValueError) as error:
            raise ToolEnvironmentError("OutputConflict", str(descriptor_path), "active package descriptor is malformed") from error
        referenced.add(fingerprint)
        active_package_roots.append(package_root)

    for package_root in active_package_roots:
        removed_entries, removed_bytes = _cleanup_legacy_package_entries(package_root)
        legacy_removed_entries += removed_entries
        legacy_removed_bytes += removed_bytes

    stale_removed_entries = 0
    stale_removed_bytes = 0
    unknown_skipped_entries = 0
    active_set = set(active_ids)
    for package_root in sorted(prepared_root.iterdir(), key=lambda item: item.name):
        if package_root.name in active_set:
            continue
        mode = package_root.lstat().st_mode
        if stat.S_ISLNK(mode):
            raise ToolEnvironmentError("UnsafePath", str(package_root), "prepared package is a symlink")
        if not stat.S_ISDIR(mode) or not _is_superseded_package_output(
            package_root, current_packages, active_set
        ):
            unknown_skipped_entries += 1
            continue
        stale_removed_bytes += _tree_size_without_symlinks(package_root)
        shutil.rmtree(package_root)
        stale_removed_entries += 1

    removed = 0
    for build in sorted(builds_root.iterdir(), key=lambda item: item.name):
        if build.is_symlink() or not build.is_dir():
            raise ToolEnvironmentError("UnsafePath", str(build), "shared build entry is unsafe")
        if len(build.name) != 64 or any(char not in "0123456789abcdef" for char in build.name):
            raise ToolEnvironmentError("OutputConflict", str(build), "shared build fingerprint is invalid")
        _load_build_manifest(build, build.name)
        if build.name not in referenced:
            shutil.rmtree(build)
            removed += 1
    return {
        "descriptors": len(active_package_roots),
        "referenced": len(referenced),
        "removed": removed,
        "legacyRemovedEntries": legacy_removed_entries,
        "legacyRemovedBytes": legacy_removed_bytes,
        "staleRemovedEntries": stale_removed_entries,
        "staleRemovedBytes": stale_removed_bytes,
        "unknownSkippedEntries": unknown_skipped_entries,
    }


def _validated_active_package_ids(active_package_ids: Sequence[str]) -> tuple[str, ...]:
    if not active_package_ids:
        raise ToolEnvironmentError("InvalidPath", "active_package_ids", "active package set must not be empty")
    if len(set(active_package_ids)) != len(active_package_ids):
        raise ToolEnvironmentError("InvalidPath", "active_package_ids", "active package IDs must be unique")
    if any(not isinstance(package_id, str) or not PACKAGE_ID_PATTERN.fullmatch(package_id) for package_id in active_package_ids):
        raise ToolEnvironmentError("InvalidPath", "active_package_ids", "active package ID is invalid")
    return tuple(active_package_ids)


def _current_package_sources(packages_root: Path) -> dict[str, Path]:
    packages: dict[str, Path] = {}
    for package_root in sorted(packages_root.iterdir(), key=lambda item: item.name):
        mode = package_root.lstat().st_mode
        if stat.S_ISLNK(mode):
            raise ToolEnvironmentError("UnsafePath", str(package_root), "trusted package root contains a symlink")
        if not stat.S_ISDIR(mode):
            continue
        manifest = package_root / "copilot.toml"
        if not os.path.lexists(manifest):
            continue
        if manifest.is_symlink() or not manifest.is_file():
            raise ToolEnvironmentError("UnsafePath", str(manifest), "trusted package manifest is unsafe")
        try:
            payload = tomllib.loads(manifest.read_text(encoding="utf-8"))
            package_id = payload.get("id")
        except (OSError, UnicodeError, tomllib.TOMLDecodeError) as error:
            raise ToolEnvironmentError("OutputConflict", str(manifest), "trusted package manifest is malformed") from error
        if not isinstance(package_id, str) or not PACKAGE_ID_PATTERN.fullmatch(package_id):
            raise ToolEnvironmentError("OutputConflict", str(manifest), "trusted package ID is invalid")
        if package_id in packages:
            raise ToolEnvironmentError("OutputConflict", str(manifest), "trusted package ID is duplicated")
        packages[package_id] = package_root.resolve(strict=True)
    return packages


def _is_superseded_package_output(
    package_root: Path, current_packages: Mapping[str, Path], active_ids: set[str]
) -> bool:
    marker_path = package_root / OWNER_MARKER
    if marker_path.is_symlink() or not marker_path.is_file():
        return False
    try:
        marker = json.loads(marker_path.read_text(encoding="utf-8"))
        source_root = marker.get("sourceRoot")
        if marker.get("schemaVersion") != 1 or not isinstance(source_root, str):
            return False
        canonical_source = Path(source_root).resolve(strict=True)
    except (OSError, UnicodeError, json.JSONDecodeError, ValueError):
        return False
    current_id = next(
        (package_id for package_id, source in current_packages.items() if source == canonical_source),
        None,
    )
    return current_id in active_ids and current_id != package_root.name


def _iter_prepared_descriptors(root: Path) -> list[Path]:
    descriptors: list[Path] = []
    for package_root in sorted(root.iterdir(), key=lambda item: item.name):
        mode = package_root.lstat().st_mode
        if stat.S_ISLNK(mode):
            raise ToolEnvironmentError("UnsafePath", str(package_root), "prepared package is a symlink")
        if not stat.S_ISDIR(mode):
            continue
        descriptor_path = package_root / PREPARED_DESCRIPTOR
        if descriptor_path.is_symlink():
            raise ToolEnvironmentError("UnsafePath", str(descriptor_path), "prepared descriptor is a symlink")
        if os.path.lexists(descriptor_path):
            if not descriptor_path.is_file():
                raise ToolEnvironmentError("UnsafePath", str(descriptor_path), "prepared descriptor is unsafe")
            marker_path = package_root / OWNER_MARKER
            if marker_path.is_symlink():
                raise ToolEnvironmentError("UnsafePath", str(marker_path), "prepared owner marker is a symlink")
            descriptors.append(descriptor_path)
    return descriptors


def _cleanup_legacy_package_entries(package_root: Path) -> tuple[int, int]:
    removed_entries = 0
    removed_bytes = 0
    for name in ("builds", "pip-cache", "npm-cache", "tmp"):
        entry = package_root / name
        if not os.path.lexists(entry):
            continue
        mode = entry.lstat().st_mode
        if stat.S_ISLNK(mode):
            raise ToolEnvironmentError("UnsafePath", str(entry), "legacy build entry is a symlink")
        if not stat.S_ISDIR(mode):
            raise ToolEnvironmentError("OutputConflict", str(entry), "legacy build entry is not a directory")
        removed_bytes += _tree_size_without_symlinks(entry)
        shutil.rmtree(entry)
        removed_entries += 1
    return removed_entries, removed_bytes


def _tree_size_without_symlinks(root: Path) -> int:
    total = 0
    for entry in root.iterdir():
        mode = entry.lstat().st_mode
        if stat.S_ISLNK(mode):
            # Symlinks are disposable entries inside an owned legacy root.
            # Count only the link itself and never follow its target.
            total += entry.lstat().st_size
            continue
        if stat.S_ISDIR(mode):
            total += _tree_size_without_symlinks(entry)
        elif stat.S_ISREG(mode):
            total += entry.stat().st_size
        else:
            raise ToolEnvironmentError("OutputConflict", str(entry), "legacy build tree contains an unsupported entry")
    return total


def _descriptor_root(root: PreparedCapabilityRoot) -> dict[str, Any]:
    return {
        "id": root.id,
        "servers": [
            {
                "id": server.id,
                "transport": "stdio",
                "command": str(server.command),
                "args": list(server.args),
                "envBindings": list(server.env_bindings),
                **(
                    {"startupTimeoutSec": server.startup_timeout_sec}
                    if server.startup_timeout_sec is not None
                    else {}
                ),
                **(
                    {"toolTimeoutSec": server.tool_timeout_sec}
                    if server.tool_timeout_sec is not None
                    else {}
                ),
            }
            for server in root.servers
        ],
    }
