#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
launcher="$repo_root/tools/maps-mcp/bin/maps-mcp-launcher"
python_bin="$repo_root/tools/maps-mcp/.venv/bin/python"

if [[ ! -x "$launcher" ]]; then
  echo "maps-mcp launcher is missing or not executable: $launcher" >&2
  exit 1
fi

timeout "${MAPS_MCP_HELP_TIMEOUT_SEC:-30}" "$launcher" --help >/tmp/open-web-codex-maps-mcp-help.out

REPO_ROOT="$repo_root" "$python_bin" - <<'PY'
import asyncio
import json
import sys
from pathlib import Path

repo_root = Path(__import__("os").environ["REPO_ROOT"])
sys.path.insert(0, str(repo_root / "tools" / "maps-mcp"))
from maps_mcp.server import MapCardPoint, create_map_card  # noqa: E402

result = asyncio.run(
    create_map_card(
        title="maps-mcp launcher smoke",
        summary="Verifies that the local maps MCP package can create a map card.",
        points=[MapCardPoint(latitude=-2.5, longitude=118.0, label="Indonesia")],
    )
)
marker = result.get("marker", "")
assert result.get("type") == "open-web-card"
assert result.get("kind") == "map.v1"
assert "open-web-card map.v1" in marker
print(json.dumps({"ok": True, "kind": result["kind"], "markerBytes": len(marker.encode())}))
PY
