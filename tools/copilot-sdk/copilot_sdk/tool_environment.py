"""Generic preparation and Runtime projection for declared Tool environments."""

from __future__ import annotations

import base64
import csv
import hashlib
import io
import json
import os
import shutil
import signal
import stat
import subprocess
import tempfile
import zipfile
from collections.abc import Callable, Mapping, Sequence
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Literal

from .platform_packages import (
    ResolvedPlatformPackage,
    resolve_installed_platform_package,
)
from .tool_runtime_manifest import ToolRuntimeManifest, load_tool_runtime_manifest

PREPARED_DESCRIPTOR = Path("copilot-sdk/prepared-tools.v1.json")
OWNER_MARKER = ".copilot-tool-environment.json"
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
    composition_descriptor_sha256: str,
    deliveries: Sequence[PreparedDelivery] = (),
    host_environment: Mapping[str, str] | None = None,
    run_command: CommandRunner | None = None,
    executable_identity_resolver: ExecutableIdentityResolver | None = None,
    platform_package_resolver: PlatformPackageResolver | None = None,
) -> PreparedToolComposition:
    """Prepare declared dependencies once and emit an unresolved internal descriptor."""

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
        (tool, load_tool_runtime_manifest(source_root, tool.root, tool.runtime))
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
    if (
        existing_marker is not None
        and existing_marker.get("preparationFingerprint") == fingerprint
    ):
        reused = _load_prepared_composition(
            output_root / PREPARED_DESCRIPTOR,
            existing_marker.get("compositionDescriptorSha256"),
        )
        if reused is not None:
            _write_owner_marker(
                output_root,
                source_root,
                composition_descriptor_sha256,
                fingerprint,
            )
            _write_prepared_descriptor(
                output_root / PREPARED_DESCRIPTOR,
                composition_descriptor_sha256,
                reused.capability_roots,
                deliveries,
            )
            return PreparedToolComposition(
                output_root / PREPARED_DESCRIPTOR,
                reused.capability_roots,
                "reused",
                tuple(deliveries),
            )

    _write_owner_marker(
        output_root,
        source_root,
        composition_descriptor_sha256,
        fingerprint,
    )
    descriptor_path = output_root / PREPARED_DESCRIPTOR
    descriptor_path.unlink(missing_ok=True)
    build_root = output_root / "builds" / fingerprint
    if build_root.exists():
        shutil.rmtree(build_root)
    build_root.mkdir(parents=True)
    prepared_roots: list[PreparedCapabilityRoot] = []
    try:
        for tool, manifest in declared:
            prepared_roots.append(
                _prepare_tool(
                    tool,
                    manifest,
                    build_root,
                    environment,
                    runner,
                    {name: identity[0] for name, identity in identities.items()},
                    platform_packages,
                )
            )
        _write_prepared_descriptor(
            descriptor_path,
            composition_descriptor_sha256,
            tuple(prepared_roots),
            deliveries,
        )
    except Exception:
        shutil.rmtree(build_root, ignore_errors=True)
        raise
    return PreparedToolComposition(
        output_root / PREPARED_DESCRIPTOR, tuple(prepared_roots), "built", tuple(deliveries)
    )


def _prepare_tool(
    source: ToolRuntimeSource,
    manifest: ToolRuntimeManifest,
    output_root: Path,
    host_environment: Mapping[str, str],
    run_command: CommandRunner,
    executables: Mapping[str, Path],
    platform_packages: Mapping[str, ResolvedPlatformPackage],
) -> PreparedCapabilityRoot:
    tool_root = source.root.resolve(strict=True)
    managed_root = output_root / "tool-environments" / source.id
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
                output_root,
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
                output_root,
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
    process_data: Path,
    run_command: CommandRunner,
    host_environment: Mapping[str, str],
    python: Path,
    platform_packages: Sequence[ResolvedPlatformPackage],
) -> Path:
    venv = dependency_root / "venv"
    run_command((str(python), "-m", "venv", str(venv)), tool_root, host_environment)
    venv_python = venv / ("Scripts/python.exe" if os.name == "nt" else "bin/python")
    pip_env = dict(host_environment)
    pip_env.update({"PIP_CACHE_DIR": str(process_data / "pip-cache"), "TMPDIR": str(process_data / "tmp")})
    (process_data / "tmp").mkdir(parents=True, exist_ok=True)
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
    build_source = Path(tempfile.mkdtemp(prefix="tool-build-", dir=process_data / "tmp"))
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
    process_data: Path,
    run_command: CommandRunner,
    host_environment: Mapping[str, str],
    npm: Path,
) -> None:
    shutil.copy2(manifest, dependency_root / "package.json")
    shutil.copy2(lock, dependency_root / "package-lock.json")
    env = dict(host_environment)
    env["npm_config_cache"] = str(process_data / "npm-cache")
    run_command((str(npm), "ci", "--ignore-scripts"), dependency_root, env)


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
    (root / OWNER_MARKER).write_text(
        json.dumps(expected, sort_keys=True, indent=2) + "\n", encoding="utf-8"
    )


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


def _load_prepared_composition(
    descriptor_path: Path,
    expected_composition_descriptor_sha256: object,
) -> PreparedToolComposition | None:
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
        roots: list[PreparedCapabilityRoot] = []
        for root in payload["capabilityRoots"]:
            servers: list[PreparedToolServer] = []
            for server in root["servers"]:
                command = Path(server["command"])
                bindings = tuple(server.get("envBindings", []))
                if not command.is_file():
                    return None
                for binding in bindings:
                    resolved = binding.get("resolvedRoot")
                    if resolved is not None and not Path(resolved).is_dir():
                        return None
                servers.append(
                    PreparedToolServer(
                        server["id"],
                        command,
                        tuple(server["args"]),
                        bindings,
                        server.get("startupTimeoutSec"),
                        server.get("toolTimeoutSec"),
                    )
                )
            roots.append(PreparedCapabilityRoot(root["id"], tuple(servers)))
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
        return PreparedToolComposition(descriptor_path, tuple(roots), "reused", deliveries)
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
    descriptor_temp = descriptor_path.with_suffix(".tmp")
    descriptor_temp.write_text(
        json.dumps(payload, ensure_ascii=False, sort_keys=True, indent=2) + "\n",
        encoding="utf-8",
    )
    descriptor_temp.replace(descriptor_path)


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
