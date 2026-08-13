"""CLI entrypoint for Copilot and Tool package authoring."""

from __future__ import annotations

import argparse
from dataclasses import asdict
from importlib.resources import files
import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

from .copilot_manifest import CopilotPackageError, validate_copilot_package
from .app_server_client import AppServerClient, AppServerClientError
from .dev_profile import (
    CopilotDevError,
    load_dev_composition,
    prepare_dev_profile,
    resolve_tool_setup,
    validate_workspace,
)
from .manifest import PackageError, pack_tool_package, validate_tool_package
from .test_runner import CopilotTestError, run_copilot_tests


PLUGIN_TEMPLATE = """{{
  \"name\": \"{name}\",
  \"version\": \"0.1.0\",
  \"description\": \"{description}\",
  \"author\": {{\"name\": \"Workspace author\"}},
  \"mcpServers\": \"./.mcp.json\",
  \"skills\": \"./skills/\"
}}
"""

MCP_TEMPLATE = """{{
  \"mcpServers\": {{
    \"{server_name}\": {{
      \"command\": \"./bin/{server_name}-launcher\",
      \"args\": [],
      \"cwd\": \".\",
      \"default_tools_approval_mode\": \"approve\"
    }}
  }}
}}
"""

SERVER_TEMPLATE = '''"""Implement the MCP tools for {name}."""

from mcp.server.fastmcp import FastMCP

mcp = FastMCP("{server_name}")


@mcp.tool()
def health() -> dict[str, str]:
    """Return a deterministic health result for local discovery tests."""

    return {{"status": "ok", "tool": "{server_name}.health"}}


if __name__ == "__main__":
    mcp.run()
'''

LAUNCHER_TEMPLATE = """#!/usr/bin/env bash
set -euo pipefail
root_dir=\"$(cd \"$(dirname \"${BASH_SOURCE[0]}\")/..\" && pwd)\"
python_bin=\"${OPEN_WEB_CODEX_TOOL_PYTHON:-$(command -v python3 || true)}\"
if [[ -z \"$python_bin\" || ! -x \"$python_bin\" ]]; then
  printf 'tool package requires Python 3.11+\\n' >&2
  exit 127
fi
export PYTHONPATH=\"$root_dir${PYTHONPATH:+:$PYTHONPATH}\"
exec \"$python_bin\" \"$root_dir/server.py\" \"$@\"
"""

SKILL_TEMPLATE = """# {name}

## 适用问题
说明这个 Tool 解决的业务问题，以及不适用的情况。

## 输入来源
列出输入字段、数据 owner、单位、允许的 Resource 类型和缺失处理。

## 工具调用
只调用已声明的 MCP Tool；不要把完整数据复制到 Agent 上下文。

## 交付件
说明输出 Resource schema、摘要预算和失败状态。

## 失败处理
数据缺失、参数不明确、权限不足和工具失败必须返回明确的 unavailable 或 failed 结果。
"""

COPILOT_TEMPLATE_FILES = {
    "copilot.toml": "copilot.toml",
    "skills/__SUPERVISOR_SKILL__/SKILL.md": "supervisor.SKILL.md",
    "skills/__CHILD_SKILL__/SKILL.md": "child.SKILL.md",
    "agents/__AGENT_ID__.toml": "child-agent.toml",
    "tools/__TOOL_DIR__/.codex-plugin/plugin.json": "tool-plugin.json",
    "tools/__TOOL_DIR__/.mcp.json": "tool-mcp.json",
    "tools/__TOOL_DIR__/bin/__TOOL_ID__-launcher": "tool-launcher",
    "tools/__TOOL_DIR__/bin/setup-env": "tool-setup-env",
    "tools/__TOOL_DIR__/requirements.txt": "tool-requirements.txt",
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
        if source_template in ("tool-launcher", "tool-setup-env"):
            destination.chmod(0o755)


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
    if as_json:
        print(json.dumps({"ok": True, "copilot": payload}, ensure_ascii=False, indent=2))
        return
    print(f"Copilot package '{payload['id']}' is valid.")
    print(f"  manifest: {manifest.as_posix()}")
    print(f"  skills: {len(payload['skill_ids'])}")
    print(f"  agents: {len(payload['agent_ids'])}")
    print(f"  tools: {len(payload['tool_ids'])}")


def _init_tool(path: Path, name: str, description: str) -> None:
    server_name = name.replace("-", "_")
    if path.exists() and any(path.iterdir()):
        raise PackageError(f"refusing to overwrite non-empty directory: {path}")
    (path / ".codex-plugin").mkdir(parents=True, exist_ok=True)
    (path / "bin").mkdir(parents=True, exist_ok=True)
    (path / "skills" / name).mkdir(parents=True, exist_ok=True)
    (path / ".codex-plugin/plugin.json").write_text(
        PLUGIN_TEMPLATE.format(name=name, description=description), encoding="utf-8"
    )
    (path / ".mcp.json").write_text(
        MCP_TEMPLATE.format(server_name=server_name), encoding="utf-8"
    )
    (path / "server.py").write_text(
        SERVER_TEMPLATE.format(name=name, server_name=server_name), encoding="utf-8"
    )
    launcher = path / "bin" / f"{server_name}-launcher"
    launcher.write_text(LAUNCHER_TEMPLATE, encoding="utf-8")
    launcher.chmod(0o755)
    (path / "skills" / name / "SKILL.md").write_text(
        SKILL_TEMPLATE.format(name=name), encoding="utf-8"
    )


def _run_tests(path: Path) -> int:
    tests = path / "tests"
    if not tests.is_dir():
        print("No tests directory; package validation still passed.")
        return 0
    return subprocess.run(
        [sys.executable, "-m", "unittest", "discover", "-s", str(tests), "-v"],
        cwd=path,
        check=False,
    ).returncode


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


def _run_dev_probe(args: argparse.Namespace) -> dict[str, object]:
    composition = load_dev_composition(args.source_root, args.manifest)
    workspace = validate_workspace(args.workspace)
    codex_bin = _resolve_codex_bin(args.codex_bin)
    prepared = prepare_dev_profile(
        composition, args.profile, keep_profile=args.keep_profile
    )
    client: AppServerClient | None = None
    try:
        environment = {"OPEN_WEB_CODEX_DATA_DIR": str(prepared.process_data)}
        for tool in composition.tools:
            setup = resolve_tool_setup(composition, tool)
            try:
                completed = subprocess.run(
                    [str(setup)],
                    cwd=tool.source,
                    env={**os.environ, **environment},
                    text=True,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    check=False,
                    timeout=120,
                )
            except (OSError, subprocess.TimeoutExpired) as error:
                raise CopilotDevError(
                    "EnvironmentUnavailable",
                    "tool-setup",
                    str(setup),
                    "Tool environment setup could not complete",
                    str(error),
                ) from error
            if completed.returncode != 0:
                cause = (completed.stderr or completed.stdout)[-2000:]
                raise CopilotDevError(
                    "EnvironmentUnavailable",
                    "tool-setup",
                    str(setup),
                    f"Tool environment setup exited with status {completed.returncode}",
                    cause,
                )
        client = AppServerClient.launch(
            codex_bin,
            profile_root=prepared.profile_root,
            process_home=prepared.process_home,
            process_cwd=prepared.process_cwd,
            environment=environment,
        )
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
                    "path": str(tool.source),
                },
            }
            for tool in composition.tools
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
        expected_tools = list(composition.summary.tool_ids)
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
            raise CopilotDevError(
                "DiscoveryIncomplete",
                "discovery",
                composition.manifest_path.as_posix(),
                f"Runtime did not discover declared capabilities: {missing}",
            )
        return {
            "ok": True,
            "state": "discovery_ready",
            "copilot": {
                "id": composition.summary.id,
                "compositionDescriptorSha256": composition.summary.composition_descriptor_sha256,
            },
            "profile": {
                "path": str(prepared.profile_root),
                "temporary": prepared.temporary,
                "preserved": not prepared.cleanup_on_exit,
            },
            "workspace": str(workspace),
            "runtime": {"codexBin": str(codex_bin), "threadId": thread_id},
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
    print(f"  workspace: {payload['workspace']}")
    print(f"  skills: {len(discovery['skills']['observed'])}/{len(discovery['skills']['expected'])}")
    print(f"  MCP servers: {len(discovery['mcpServers']['observed'])}/{len(discovery['mcpServers']['expected'])}")
    print("  role spawn: not_run")
    print("  model acceptance: not_run")


def _dev_error_payload(error: CopilotDevError) -> dict[str, object]:
    detail: dict[str, object] = {
        "code": error.code,
        "stage": error.stage,
        "path": error.path,
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
    validate_copilot.add_argument("--json", action="store_true")

    dev_copilot = subparsers.add_parser("dev")
    dev_copilot.add_argument("source_root", type=Path)
    dev_copilot.add_argument("--workspace", type=Path, required=True)
    dev_copilot.add_argument("--manifest", type=Path, default=Path("copilot.toml"))
    dev_copilot.add_argument("--profile", type=Path)
    dev_copilot.add_argument("--keep-profile", action="store_true")
    dev_copilot.add_argument("--codex-bin", type=Path)
    dev_copilot.add_argument("--json", action="store_true")

    test_copilot = subparsers.add_parser("test")
    test_copilot.add_argument("source_root", type=Path)
    test_copilot.add_argument("--workspace", type=Path, required=True)
    test_copilot.add_argument("--manifest", type=Path, default=Path("copilot.toml"))
    test_copilot.add_argument("--codex-bin", type=Path)
    test_copilot.add_argument("--timeout-seconds", type=float, default=45.0)
    test_copilot.add_argument("--json", action="store_true")

    tool = subparsers.add_parser("tool")
    commands = tool.add_subparsers(dest="command", required=True)

    init = commands.add_parser("init")
    init.add_argument("path", type=Path)
    init.add_argument("--name", required=True)
    init.add_argument("--description", default="A Codex MCP Tool package")

    for command in ("validate", "test"):
        command_parser = commands.add_parser(command)
        command_parser.add_argument("path", type=Path)

    pack = commands.add_parser("pack")
    pack.add_argument("path", type=Path)
    pack.add_argument("--output", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        if args.resource == "init":
            _init_copilot(args.path, args.name)
            manifest = Path("copilot.toml")
            summary = validate_copilot_package(args.path, manifest)
            _print_copilot_summary(summary, manifest=manifest, as_json=args.json)
            return 0
        if args.resource == "validate":
            summary = validate_copilot_package(args.source_root, args.manifest)
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
            )
            _print_test_result(payload, as_json=args.json)
            return 0
        if args.command == "init":
            _init_tool(args.path, args.name, args.description)
            print(json.dumps(validate_tool_package(args.path), ensure_ascii=False, indent=2))
            return 0
        if args.command == "validate":
            print(json.dumps(validate_tool_package(args.path), ensure_ascii=False, indent=2))
            return 0
        if args.command == "test":
            validate_tool_package(args.path)
            return _run_tests(args.path)
        print(json.dumps(pack_tool_package(args.path, args.output), ensure_ascii=False, indent=2))
        return 0
    except PackageError as error:
        print(f"copilot-sdk: {error}", file=sys.stderr)
        return 2
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
                f"copilot: {error.code}: {error.stage}: {error.path}: {error.message}",
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
