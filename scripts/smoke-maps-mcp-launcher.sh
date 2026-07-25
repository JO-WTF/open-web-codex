#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
launcher="$repo_root/tools/maps-mcp/bin/maps-mcp-launcher"
data_dir="${OPEN_WEB_CODEX_DATA_DIR:-$repo_root/.local/open-web-codex}"
venv_dir="${OPEN_WEB_CODEX_MAPS_MCP_VENV:-${MAPS_MCP_VENV:-$data_dir/tool-envs/maps-mcp}}"

if [[ ! -x "$launcher" ]]; then
  echo "maps-mcp launcher is missing or not executable: $launcher" >&2
  exit 1
fi

OPEN_WEB_CODEX_MAPS_MCP_VENV="$venv_dir" "$repo_root/scripts/setup-maps-mcp-env.sh" >/tmp/open-web-codex-maps-mcp-setup.out

LAUNCHER="$launcher" OPEN_WEB_CODEX_MAPS_MCP_VENV="$venv_dir" python3 - <<'PY' >/tmp/open-web-codex-maps-mcp-help.out
import os
import subprocess

subprocess.run(
    [os.environ["LAUNCHER"], "--help"],
    check=True,
    timeout=int(os.environ.get("MAPS_MCP_HELP_TIMEOUT_SEC", "30")),
)
PY

REPO_ROOT="$repo_root" LAUNCHER="$launcher" OPEN_WEB_CODEX_MAPS_MCP_VENV="$venv_dir" python3 - <<'PY'
import json
import os
import select
import subprocess
import sys
import time

repo_root = os.environ["REPO_ROOT"]
launcher = os.environ["LAUNCHER"]
startup_timeout = int(os.environ.get("MAPS_MCP_STARTUP_TIMEOUT_SEC", "60"))
env = os.environ.copy()
proc = subprocess.Popen(
    [launcher, "--workspace-root", repo_root],
    cwd=os.path.join(repo_root, "tools", "maps-mcp"),
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
    stderr=subprocess.PIPE,
    text=True,
    bufsize=1,
    env=env,
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
    instructions = initialize.get("result", {}).get("instructions", "")
    if (
        "structuredContent.data_ref" not in instructions
        or "data_ref unchanged into create_map_card" not in instructions
        or "data_ref.server as server" not in instructions
        or "data_ref.uri as uri" not in instructions
        or "mcp__map_utils" not in instructions
    ):
        raise SystemExit(f"maps MCP did not advertise its map-card output contract: {instructions!r}")
    send("notifications/initialized", request=False)

    request_id = send("resources/templates/list", {})
    templates = read_response(request_id, 30)
    resource_templates = templates.get("result", {}).get("resourceTemplates", [])
    if not any(
        template.get("uriTemplate") == "maps-data://geojson/{resource_id}"
        and template.get("name") == "maps_geojson"
        and template.get("title") == "Maps GeoJSON"
        and template.get("mimeType") == "application/geo+json"
        for template in resource_templates
    ):
        raise SystemExit(f"GeoJSON Resource template missing: {resource_templates!r}")

    request_id = send("tools/list", {})
    tools = read_response(request_id, 30)
    listed_tools = tools.get("result", {}).get("tools", [])
    tool_names = [tool.get("name") for tool in listed_tools]
    if "create_map_card" not in tool_names:
        raise SystemExit(f"create_map_card missing from MCP tools: {tool_names}")
    geocode_tool = next(tool for tool in listed_tools if tool.get("name") == "batch_geocode")
    geocode_output_schema = geocode_tool.get("outputSchema")
    required_geocode_fields = {
        "provider",
        "summary",
        "feature_count",
        "data_ref",
    }
    if not isinstance(geocode_output_schema, dict) or not required_geocode_fields.issubset(
        set(geocode_output_schema.get("required", []))
    ):
        raise SystemExit(
            f"batch_geocode did not advertise its canonical data_ref: {geocode_output_schema}"
        )
    geocode_resource_schema = geocode_output_schema.get("$defs", {}).get("McpResourceMapData", {})
    if (
        "server" not in geocode_resource_schema.get("required", [])
        or geocode_resource_schema.get("properties", {}).get("server", {}).get("const")
        != "map_utils"
    ):
        raise SystemExit(
            "batch_geocode data_ref did not require the raw map_utils server ID: "
            f"{geocode_resource_schema}"
        )
    map_card_tool = next(tool for tool in listed_tools if tool.get("name") == "create_map_card")
    map_card_resource_schema = (
        map_card_tool.get("inputSchema", {}).get("$defs", {}).get("McpResourceMapData", {})
    )
    if (
        "server" not in map_card_resource_schema.get("required", [])
        or map_card_resource_schema.get("properties", {}).get("server", {}).get("const")
        != "map_utils"
    ):
        raise SystemExit(
            "create_map_card did not require the raw map_utils server ID: "
            f"{map_card_resource_schema}"
        )
    output_schema = map_card_tool.get("outputSchema")
    required_output_fields = {"type", "kind", "artifact", "embed"}
    if not isinstance(output_schema, dict) or not required_output_fields.issubset(
        set(output_schema.get("required", []))
    ):
        raise SystemExit(f"create_map_card missing required outputSchema: {output_schema}")

    request_id = send(
        "tools/call",
        {
            "name": "create_map_card",
            "arguments": {
                "title": "Jakarta",
                "summary": "Maps MCP handshake smoke",
                "viewport": {
                    "mode": "camera",
                    "center": [106.827168, -6.1754049],
                    "zoom": 10,
                },
                "sources": [{
                    "id": "locations",
                    "data": {
                        "type": "inline",
                        "format": "geojson",
                        "geojson": {
                            "type": "FeatureCollection",
                            "features": [{
                                "type": "Feature",
                                "properties": {"label": "Jakarta"},
                                "geometry": {
                                    "type": "Point",
                                    "coordinates": [106.827168, -6.1754049],
                                },
                            }],
                        },
                    },
                }],
                "layers": [{
                    "id": "points",
                    "source": "locations",
                    "geometry": "point",
                    "label_property": "label",
                    "style": {
                        "color": "#ef4444",
                        "opacity": 0.9,
                        "radius": 8,
                        "stroke_color": "#ffffff",
                        "stroke_width": 2,
                    },
                }],
            },
        },
    )
    call = read_response(request_id, 30)
    if "error" in call:
        raise SystemExit(call["error"])
    content = call.get("result", {}).get("content", [])
    text = "\n".join(item.get("text", "") for item in content if item.get("type") == "text")
    structured_content = call.get("result", {}).get("structuredContent")
    if not isinstance(structured_content, dict):
        raise SystemExit(f"create_map_card did not return structuredContent: {call}")
    if structured_content.get("type") != "open-web-artifact":
        raise SystemExit(f"create_map_card returned an invalid type: {structured_content!r}")
    if structured_content.get("kind") != "inline-visualization.v1":
        raise SystemExit(f"create_map_card returned an invalid kind: {structured_content!r}")
    artifact = structured_content.get("artifact")
    if not isinstance(artifact, dict) or not artifact.get("ref", "").startswith("map-"):
        raise SystemExit(f"create_map_card returned an invalid Artifact: {artifact!r}")
    renderer = artifact.get("renderer")
    if not isinstance(renderer, dict) or renderer.get("kind") != "map.v2":
        raise SystemExit(f"create_map_card returned an invalid renderer: {renderer!r}")
    card = renderer.get("payload")
    if not isinstance(card, dict) or card.get("title") != "Jakarta":
        raise SystemExit(f"create_map_card returned an invalid card: {card!r}")
    embed = structured_content.get("embed")
    expected_embed = f'::codex-inline-vis{{artifact="{artifact["ref"]}"}}'
    if not isinstance(embed, dict) or embed.get("code") != expected_embed:
        raise SystemExit(f"create_map_card returned an invalid embed code: {embed!r}")
    if expected_embed not in text:
        raise SystemExit(f"create_map_card returned unexpected text content: {text[:500]}")
    print(json.dumps({
        "ok": True,
        "tools": tool_names,
        "structuredContentBytes": len(json.dumps(structured_content).encode()),
    }))
finally:
    try:
        proc.terminate()
        proc.wait(timeout=5)
    except Exception:
        proc.kill()
PY
