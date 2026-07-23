#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
launcher="$repo_root/tools/maps-mcp/bin/maps-mcp-launcher"

if [[ ! -x "$launcher" ]]; then
  echo "maps-mcp launcher is missing or not executable: $launcher" >&2
  exit 1
fi

timeout "${MAPS_MCP_HELP_TIMEOUT_SEC:-${MAPS_MCP_STARTUP_TIMEOUT_SEC:-180}}" "$launcher" --help >/tmp/open-web-codex-maps-mcp-help.out

REPO_ROOT="$repo_root" LAUNCHER="$launcher" python3 - <<'PY'
import json
import os
import select
import subprocess
import sys
import time

repo_root = os.environ["REPO_ROOT"]
launcher = os.environ["LAUNCHER"]
startup_timeout = int(os.environ.get("MAPS_MCP_STARTUP_TIMEOUT_SEC", "180"))
proc = subprocess.Popen(
    [launcher, "--workspace-root", repo_root],
    cwd=os.path.join(repo_root, "tools", "maps-mcp"),
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
    stderr=subprocess.PIPE,
    text=True,
    bufsize=1,
)
next_id = 1

def send(method, params=None, request=True):
    global next_id
    message = {"jsonrpc": "2.0", "method": method}
    request_id = None
    if request:
        request_id = next_id
        next_id += 1
        message["id"] = request_id
    if params is not None:
        message["params"] = params
    proc.stdin.write(json.dumps(message) + "\n")
    proc.stdin.flush()
    return request_id

def read_response(request_id, timeout):
    deadline = time.time() + timeout
    stderr_tail: list[str] = []
    while time.time() < deadline:
        readable, _, _ = select.select([proc.stdout, proc.stderr], [], [], 0.2)
        for stream in readable:
            line = stream.readline()
            if not line:
                continue
            if stream is proc.stderr:
                stderr_tail.append(line.rstrip())
                stderr_tail = stderr_tail[-20:]
                continue
            try:
                message = json.loads(line)
            except json.JSONDecodeError as exc:
                raise SystemExit(f"non-JSON MCP stdout before response: {line[:200]!r}") from exc
            if message.get("id") == request_id:
                return message
    raise SystemExit("timed out waiting for MCP response; recent stderr=" + json.dumps(stderr_tail[-5:]))

try:
    request_id = send(
        "initialize",
        {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "open-web-codex-maps-mcp-smoke", "version": "0"},
        },
    )
    initialize = read_response(request_id, startup_timeout)
    if "error" in initialize:
        raise SystemExit(initialize["error"])
    send("notifications/initialized", request=False)

    request_id = send("tools/list", {})
    tools = read_response(request_id, 30)
    tool_names = [tool.get("name") for tool in tools.get("result", {}).get("tools", [])]
    if "create_map_card" not in tool_names:
        raise SystemExit(f"create_map_card missing from MCP tools: {tool_names}")

    request_id = send(
        "tools/call",
        {
            "name": "create_map_card",
            "arguments": {
                "title": "Jakarta",
                "summary": "Maps MCP handshake smoke",
                "points": [{"latitude": -6.1754049, "longitude": 106.827168, "label": "Jakarta"}],
            },
        },
    )
    call = read_response(request_id, 30)
    if "error" in call:
        raise SystemExit(call["error"])
    content = call.get("result", {}).get("content", [])
    text = "\n".join(item.get("text", "") for item in content if item.get("type") == "text")
    if "open-web-card map.v1" not in text:
        raise SystemExit(f"create_map_card did not return a map marker: {text[:500]}")
    print(json.dumps({"ok": True, "tools": tool_names, "markerBytes": len(text.encode())}))
finally:
    try:
        proc.terminate()
        proc.wait(timeout=5)
    except Exception:
        proc.kill()
PY
