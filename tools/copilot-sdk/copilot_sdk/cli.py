"""CLI entrypoint for Tool package authoring."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

from .manifest import PackageError, pack_tool_package, validate_tool_package


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


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="copilot-sdk")
    subparsers = parser.add_subparsers(dest="resource", required=True)
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
