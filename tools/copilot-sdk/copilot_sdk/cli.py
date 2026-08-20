"""CLI entrypoint for Copilot source authoring and runtime preparation."""

from __future__ import annotations

import argparse
import json
import re
import shutil
import sys
import time
from dataclasses import asdict
from importlib.resources import files
from pathlib import Path

from .app_server_client import AppServerClient, AppServerClientError
from .copilot_manifest import CopilotPackageError, validate_copilot_package
from .dev_profile import (
    CopilotDevError,
    load_dev_composition,
    prepare_dev_profile,
    prepare_dev_tool_composition,
    prepared_deliveries,
    validate_workspace,
)
from .test_runner import CopilotTestError, run_copilot_tests
from .tool_environment import (
    ToolEnvironmentError,
    ToolRuntimeSource,
    garbage_collect_tool_builds,
    prepare_tool_composition,
)

COPILOT_TEMPLATE_FILES = {
    "copilot.toml": "copilot.toml",
    "skills/__SUPERVISOR_SKILL__/SKILL.md": "supervisor.SKILL.md",
    "skills/__CHILD_SKILL__/SKILL.md": "child.SKILL.md",
    "agents/__AGENT_ID__.toml": "child-agent.toml",
    "tools/__TOOL_DIR__/pyproject.toml": "tool-pyproject.toml",
    "tools/__TOOL_DIR__/requirements.lock": "tool-requirements.lock",
    "tools/__TOOL_DIR__/runtime.toml": "tool-runtime.toml",
    "tools/__TOOL_DIR__/server.py": "tool-server.py",
}


def _template_text(name: str) -> str:
    return files("copilot_sdk").joinpath("templates", name).read_text(encoding="utf-8")


def _replace_markers(text: str, replacements: dict[str, str]) -> str:
    rendered = text
    for marker, value in replacements.items():
        rendered = rendered.replace(f"__{marker}__", value)
    return rendered


def _render_template(name: str, replacements: dict[str, str]) -> str:
    return _replace_markers(_template_text(name), replacements)


def _init_copilot(path: Path, name: str) -> None:
    if re.fullmatch(r"[a-z][a-z0-9-]{0,63}", name) is None:
        raise CopilotPackageError(
            "invalid_id",
            "id",
            "must start with a lowercase letter and contain only lowercase letters, digits, or hyphens",
        )
    if path.exists() and (not path.is_dir() or any(path.iterdir())):
        raise CopilotPackageError(
            "destination_not_empty", ".", f"refusing to overwrite non-empty path: {path}"
        )
    agent_id = f"{name}-worker"
    tool_id = f"{name.replace('-', '_')}_tools"
    replacements = {
        "ID": name,
        "DISPLAY_NAME": name.replace("-", " ").title(),
        "SUPERVISOR_SKILL": f"{name}-supervisor",
        "CHILD_SKILL": f"{name}-worker",
        "AGENT_ID": agent_id,
        "TOOL_ID": tool_id,
        "TOOL_DIR": f"{name}-tools",
    }
    for destination_template, source_template in COPILOT_TEMPLATE_FILES.items():
        destination = path / _replace_markers(destination_template, replacements)
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(
            _render_template(source_template, replacements), encoding="utf-8"
        )


def _copilot_error_payload(error: CopilotPackageError) -> dict[str, object]:
    return {
        "ok": False,
        "error": {
            "code": error.code,
            "path": error.relative_path,
            "message": error.message,
        },
    }


def _print_copilot_summary(
    summary: object, *, manifest: Path, as_json: bool
) -> None:
    payload = asdict(summary)
    for delivery in payload.get("deliveries", []):
        delivery.pop("verifier_value", None)
    if as_json:
        print(json.dumps({"ok": True, "copilot": payload}, ensure_ascii=False, indent=2))
        return
    print(f"Copilot package '{payload['id']}' is valid.")
    print(f"  manifest: {manifest.as_posix()}")
    print(f"  skills: {len(payload['skill_ids'])}")
    print(f"  agents: {len(payload['agent_ids'])}")
    print(f"  tools: {len(payload['tool_ids'])}")


def _resolve_codex_bin(value: Path | None) -> Path:
    raw = "codex" if value is None else str(value)
    located = shutil.which(raw)
    if located is None:
        raise CopilotDevError(
            "AppServerUnavailable", "launch", raw, "Codex executable was not found"
        )
    path = Path(located).resolve(strict=True)
    if not path.is_file() or not path.stat().st_mode & 0o111:
        raise CopilotDevError(
            "AppServerUnavailable", "launch", str(path), "Codex executable is not executable"
        )
    return path


def _string_list(value: object, *, field: str) -> list[dict[str, object]]:
    if not isinstance(value, list):
        raise CopilotDevError(
            "DiscoveryFailed", "discovery", field, "Runtime returned an invalid inventory"
        )
    entries: list[dict[str, object]] = []
    for entry in value:
        if not isinstance(entry, dict):
            raise CopilotDevError(
                "DiscoveryFailed", "discovery", field, "Runtime returned an invalid inventory entry"
            )
        entries.append(entry)
    return entries


def _safe_expected_mcp_statuses(
    entries: list[dict[str, object]], expected: list[str]
) -> list[dict[str, str]]:
    """Project only fields present in the official McpServerStatus list contract."""

    expected_set = set(expected)
    statuses: list[dict[str, str]] = []
    for entry in entries:
        name = entry.get("name")
        if not isinstance(name, str) or name not in expected_set:
            continue
        status: dict[str, str] = {"name": name}
        auth_status = entry.get("authStatus")
        if isinstance(auth_status, str):
            status["authStatus"] = auth_status
        statuses.append(status)
    return statuses


def _run_dev_probe(args: argparse.Namespace) -> dict[str, object]:
    if args.timeout_seconds <= 0 or args.timeout_seconds > 300:
        raise CopilotDevError(
            "InvalidArgument",
            "timeout",
            "timeout-seconds",
            "must be greater than 0 and at most 300",
        )
    composition = load_dev_composition(
        args.source_root,
        args.manifest,
        tool_registry_root=args.tool_registry_root,
    )
    workspace = validate_workspace(args.workspace)
    codex_bin = _resolve_codex_bin(args.codex_bin)
    prepared = prepare_dev_profile(
        composition, args.profile, keep_profile=args.keep_profile
    )
    client: AppServerClient | None = None
    runtime_deadline: float | None = None
    try:
        prepared_tools = prepare_dev_tool_composition(
            prepared,
            output_root=args.tool_environment_root,
            build_store_root=args.build_store_root,
        )
        environment: dict[str, str] = {}
        client = AppServerClient.launch(
            codex_bin,
            profile_root=prepared.profile_root,
            process_home=prepared.process_home,
            process_cwd=prepared.process_cwd,
            environment=environment,
            timeout_seconds=args.timeout_seconds,
        )
        runtime_deadline = time.monotonic() + args.timeout_seconds
        client.initialize()
        skills_result = client.request(
            "skills/list", {"cwds": [str(workspace)], "forceReload": True}
        )
        skill_entries = _string_list(skills_result.get("data"), field="skills/list.data")
        discovered_skill_names: set[str] = set()
        for entry in skill_entries:
            errors = _string_list(entry.get("errors"), field="skills/list.data.errors")
            if errors:
                raise CopilotDevError(
                    "DiscoveryIncomplete",
                    "discovery",
                    composition.manifest_path.as_posix(),
                    "Runtime reported Skill discovery errors",
                )
            for skill in _string_list(entry.get("skills"), field="skills/list.data.skills"):
                name = skill.get("name")
                if isinstance(name, str):
                    discovered_skill_names.add(name)

        selected_roots = [
            {
                "id": tool.id,
                "location": {
                    "type": "environment",
                    "environmentId": "local",
                    "path": str(tool.projection_root),
                },
            }
            for tool in prepared_tools.capability_roots
        ]
        thread_result = client.request(
            "thread/start",
            {
                "cwd": str(workspace),
                "ephemeral": True,
                "approvalPolicy": "never",
                "sandbox": "read-only",
                "environments": [
                    {
                        "environmentId": "local",
                        "cwd": str(workspace),
                        "runtimeWorkspaceRoots": [str(workspace)],
                    }
                ],
                "selectedCapabilityRoots": selected_roots,
            },
        )
        thread = thread_result.get("thread")
        thread_id = thread.get("id") if isinstance(thread, dict) else None
        if not isinstance(thread_id, str) or not thread_id:
            raise CopilotDevError(
                "DiscoveryFailed",
                "thread/start",
                "thread.id",
                "Runtime did not return a thread identity",
            )
        mcp_result = client.request(
            "mcpServerStatus/list",
            {"threadId": thread_id, "detail": "toolsAndAuthOnly", "limit": 100},
            timeout_seconds=max(0.1, runtime_deadline - time.monotonic()),
        )
        mcp_entries = _string_list(
            mcp_result.get("data"), field="mcpServerStatus/list.data"
        )
        discovered_mcp_names = {
            entry["name"]
            for entry in mcp_entries
            if isinstance(entry.get("name"), str)
            and isinstance(entry.get("tools"), dict)
            and bool(entry["tools"])
        }
        expected_skills = list(composition.summary.skill_ids)
        expected_tools = list(composition.mcp_server_ids)
        observed_skills = [
            name for name in expected_skills if name in discovered_skill_names
        ]
        observed_tools = [
            name for name in expected_tools if name in discovered_mcp_names
        ]
        missing_skills = [name for name in expected_skills if name not in discovered_skill_names]
        missing_tools = [name for name in expected_tools if name not in discovered_mcp_names]
        if missing_skills or missing_tools:
            missing = ", ".join(missing_skills + missing_tools)
            safe_statuses = _safe_expected_mcp_statuses(mcp_entries, expected_tools)
            observed_status = ", ".join(
                f"{entry['name']}({entry.get('authStatus', 'unknown')})"
                for entry in safe_statuses
            ) or "none"
            raise CopilotDevError(
                "DiscoveryIncomplete",
                "discovery",
                composition.manifest_path.as_posix(),
                f"Runtime did not discover declared capabilities: {missing}; "
                f"observed MCP status: {observed_status}",
                diagnostics={"mcpServerStatus": safe_statuses},
            )
        return {
            "ok": True,
            "state": "discovery_ready",
            "copilot": {
                "id": composition.summary.id,
                "compositionDescriptorSha256": composition.summary.composition_descriptor_sha256,
            },
            "profile": {
                "temporary": prepared.temporary,
                "preserved": not prepared.cleanup_on_exit,
            },
            "discovery": {
                "skills": {"expected": expected_skills, "observed": observed_skills},
                "mcpServers": {"expected": expected_tools, "observed": observed_tools},
            },
            "roleSpawn": "not_run",
            "modelAcceptance": "not_run",
        }
    except AppServerClientError as error:
        raise CopilotDevError(
            error.code, "app-server", str(codex_bin), error.message, error.cause
        ) from error
    finally:
        if client is not None:
            client.close()
        prepared.cleanup()


def _print_dev_result(payload: dict[str, object], *, as_json: bool) -> None:
    if as_json:
        print(json.dumps(payload, ensure_ascii=False, indent=2))
        return
    copilot = payload["copilot"]
    discovery = payload["discovery"]
    assert isinstance(copilot, dict) and isinstance(discovery, dict)
    print(f"Copilot '{copilot['id']}' is discovery_ready.")
    print(f"  skills: {len(discovery['skills']['observed'])}/{len(discovery['skills']['expected'])}")
    print(f"  MCP servers: {len(discovery['mcpServers']['observed'])}/{len(discovery['mcpServers']['expected'])}")
    print("  role spawn: not_run")
    print("  model acceptance: not_run")


def _dev_error_payload(error: CopilotDevError) -> dict[str, object]:
    detail: dict[str, object] = {
        "code": error.code,
        "stage": error.stage,
        "message": error.message,
    }
    return {"ok": False, "error": detail}


def _print_test_result(payload: dict[str, object], *, as_json: bool) -> None:
    if as_json:
        print(json.dumps(payload, ensure_ascii=False, indent=2))
        return
    tests = payload["tests"]
    assert isinstance(tests, list)
    print(f"Copilot '{payload['copilot']['id']}' passed {len(tests)} native acceptance test(s).")
    print(f"  duration: {payload['durationMs']} ms")


def _run_prepare(args: argparse.Namespace) -> dict[str, object]:
    composition = load_dev_composition(
        args.source_root,
        args.manifest,
        tool_registry_root=args.tool_registry_root,
    )
    try:
        prepared = prepare_tool_composition(
            source_root=composition.source_root,
            tools=tuple(
                ToolRuntimeSource(
                    tool.id,
                    tool.source,
                    tool.runtime,
                    source_root=(
                        None
                        if tool.owner_root == composition.source_root
                        else tool.owner_root
                    ),
                )
                for tool in composition.tools
            ),
            output_root=args.output_root,
            build_store_root=args.build_store_root,
            composition_descriptor_sha256=(
                composition.summary.composition_descriptor_sha256
            ),
            deliveries=prepared_deliveries(composition.summary),
        )
    except ToolEnvironmentError as error:
        raise CopilotDevError(
            error.code,
            "tool-environment",
            error.path,
            "declared Tool environment could not be prepared",
            error.cause,
        ) from error
    return {
        "ok": True,
        "state": (
            "environment_reused" if prepared.state == "reused" else "environment_prepared"
        ),
        "copilot": {
            "id": composition.summary.id,
            "compositionDescriptorSha256": (
                composition.summary.composition_descriptor_sha256
            ),
        },
        "capabilityRoots": [
            {
                "id": root.id,
                "servers": [server.id for server in root.servers],
            }
            for root in prepared.capability_roots
        ],
    }


def _run_gc(args: argparse.Namespace) -> dict[str, object]:
    try:
        result = garbage_collect_tool_builds(
            prepared_root=args.prepared_root,
            build_store_root=args.build_store_root,
            packages_root=args.packages_root,
            active_package_ids=args.active_package_id,
        )
    except ToolEnvironmentError as error:
        raise CopilotDevError(
            error.code,
            "tool-gc",
            error.path,
            "shared Tool build garbage collection failed",
            error.cause,
        ) from error
    return {"ok": True, "state": "builds_collected", **result}


def _print_prepare_result(payload: dict[str, object], *, as_json: bool) -> None:
    if as_json:
        print(json.dumps(payload, ensure_ascii=False, indent=2))
        return
    copilot = payload["copilot"]
    roots = payload["capabilityRoots"]
    assert isinstance(copilot, dict) and isinstance(roots, list)
    print(f"Copilot '{copilot['id']}' environment is prepared.")
    print(f"  capability roots: {len(roots)}")


def _test_error_payload(error: CopilotTestError) -> dict[str, object]:
    return {
        "ok": False,
        "error": {
            "code": error.code,
            "stage": error.stage,
            "message": error.public_message,
        },
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="copilot")
    subparsers = parser.add_subparsers(dest="resource", required=True)

    init_copilot = subparsers.add_parser("init")
    init_copilot.add_argument("path", type=Path)
    init_copilot.add_argument("--name", required=True)
    init_copilot.add_argument("--json", action="store_true")

    validate_copilot = subparsers.add_parser("validate")
    validate_copilot.add_argument("source_root", type=Path)
    validate_copilot.add_argument("--manifest", type=Path, default=Path("copilot.toml"))
    validate_copilot.add_argument("--tool-registry-root", type=Path)
    validate_copilot.add_argument("--json", action="store_true")

    dev_copilot = subparsers.add_parser("dev")
    dev_copilot.add_argument("source_root", type=Path)
    dev_copilot.add_argument("--workspace", type=Path, required=True)
    dev_copilot.add_argument("--manifest", type=Path, default=Path("copilot.toml"))
    dev_copilot.add_argument("--tool-registry-root", type=Path)
    dev_copilot.add_argument("--profile", type=Path)
    dev_copilot.add_argument("--keep-profile", action="store_true")
    dev_copilot.add_argument("--codex-bin", type=Path)
    dev_copilot.add_argument("--tool-environment-root", type=Path)
    dev_copilot.add_argument("--build-store-root", type=Path)
    dev_copilot.add_argument("--timeout-seconds", type=float, default=90.0)
    dev_copilot.add_argument("--json", action="store_true")

    test_copilot = subparsers.add_parser("test")
    test_copilot.add_argument("source_root", type=Path)
    test_copilot.add_argument("--workspace", type=Path, required=True)
    test_copilot.add_argument("--manifest", type=Path, default=Path("copilot.toml"))
    test_copilot.add_argument("--tool-registry-root", type=Path)
    test_copilot.add_argument("--codex-bin", type=Path)
    test_copilot.add_argument("--tool-environment-root", type=Path)
    test_copilot.add_argument("--build-store-root", type=Path)
    test_copilot.add_argument("--timeout-seconds", type=float, default=45.0)
    test_copilot.add_argument("--json", action="store_true")

    prepare_copilot = subparsers.add_parser("prepare")
    prepare_copilot.add_argument("source_root", type=Path)
    prepare_copilot.add_argument("--manifest", type=Path, default=Path("copilot.toml"))
    prepare_copilot.add_argument("--tool-registry-root", type=Path)
    prepare_copilot.add_argument("--output-root", type=Path, required=True)
    prepare_copilot.add_argument("--build-store-root", type=Path, required=True)
    prepare_copilot.add_argument("--json", action="store_true")

    gc_copilot = subparsers.add_parser("gc-builds")
    gc_copilot.add_argument("--prepared-root", type=Path, required=True)
    gc_copilot.add_argument("--build-store-root", type=Path, required=True)
    gc_copilot.add_argument("--packages-root", type=Path, required=True)
    gc_copilot.add_argument("--active-package-id", action="append", required=True)
    gc_copilot.add_argument("--json", action="store_true")

    args = parser.parse_args(argv)
    try:
        if args.resource == "init":
            _init_copilot(args.path, args.name)
            manifest = Path("copilot.toml")
            summary = validate_copilot_package(args.path, manifest)
            _print_copilot_summary(summary, manifest=manifest, as_json=args.json)
            return 0
        if args.resource == "validate":
            summary = validate_copilot_package(
                args.source_root,
                args.manifest,
                tool_registry_root=args.tool_registry_root,
            )
            _print_copilot_summary(summary, manifest=args.manifest, as_json=args.json)
            return 0
        if args.resource == "dev":
            _print_dev_result(_run_dev_probe(args), as_json=args.json)
            return 0
        if args.resource == "test":
            payload = run_copilot_tests(
                args.source_root,
                args.manifest,
                args.workspace,
                _resolve_codex_bin(args.codex_bin),
                timeout_seconds=args.timeout_seconds,
                tool_environment_root=args.tool_environment_root,
                build_store_root=args.build_store_root,
                tool_registry_root=args.tool_registry_root,
            )
            _print_test_result(payload, as_json=args.json)
            return 0
        if args.resource == "prepare":
            _print_prepare_result(_run_prepare(args), as_json=args.json)
            return 0
        if args.resource == "gc-builds":
            gc_result = _run_gc(args)
            if args.json:
                print(json.dumps(gc_result, ensure_ascii=False, indent=2))
            else:
                print("Tool builds collected.")
            return 0
    except CopilotPackageError as error:
        if getattr(args, "json", False):
            print(json.dumps(_copilot_error_payload(error), ensure_ascii=False, indent=2))
        else:
            print(
                f"copilot: {error.code}: {error.relative_path}: {error.message}",
                file=sys.stderr,
            )
        return 2
    except CopilotDevError as error:
        if getattr(args, "resource", None) == "test":
            test_error = CopilotTestError(
                error.code,
                error.stage,
                "native acceptance environment preparation failed",
            )
            if getattr(args, "json", False):
                print(json.dumps(_test_error_payload(test_error), ensure_ascii=False, indent=2))
            else:
                print(
                    f"copilot: {test_error.code}: {test_error.stage}: "
                    f"{test_error.public_message}",
                    file=sys.stderr,
                )
            return 2
        if getattr(args, "json", False):
            print(json.dumps(_dev_error_payload(error), ensure_ascii=False, indent=2))
        else:
            print(
                f"copilot: {error.code}: {error.stage}: {error.message}",
                file=sys.stderr,
            )
        return 2
    except CopilotTestError as error:
        if getattr(args, "json", False):
            print(json.dumps(_test_error_payload(error), ensure_ascii=False, indent=2))
        else:
            print(
                f"copilot: {error.code}: {error.stage}: {error.public_message}",
                file=sys.stderr,
            )
        return 2
