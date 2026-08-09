#!/usr/bin/env bash

set -euo pipefail
umask 077

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
supply_root="${OPEN_WEB_CODEX_SUPPLY_CHAIN_ASSET_ROOT:-$repo_root/tools/supply-chain-network-planner}"
maps_root="${OPEN_WEB_CODEX_MAPS_ASSET_ROOT:-$repo_root/tools/maps-mcp}"
supply_venv="${OPEN_WEB_CODEX_SUPPLY_CHAIN_MCP_VENV:-$repo_root/.local/open-web-codex/tool-envs/supply-chain-network-planner}"
maps_venv="${OPEN_WEB_CODEX_MAPS_MCP_VENV:-$repo_root/.local/open-web-codex/tool-envs/maps-mcp}"
supply_launcher="$supply_root/bin/supply-chain-planner-launcher"
maps_launcher="$maps_root/bin/maps-mcp-launcher"

for required in \
  "$supply_launcher" \
  "$maps_launcher" \
  "$supply_venv/bin/python" \
  "$maps_venv/bin/python"
do
  [[ -x "$required" ]] || {
    printf 'required executable is unavailable: %s\n' "$required" >&2
    exit 2
  }
done

probe_root="$(mktemp -d "${TMPDIR:-/tmp}/open-web-codex-native-mcp-assets.XXXXXX")"
trap 'rm -rf -- "$probe_root"' EXIT
profile_home="$probe_root/profile"
profile_runtime="$profile_home/.open-web-codex"
profile_logs="$profile_runtime/logs"
mkdir -p "$profile_logs" "$profile_runtime/mcp-state/maps-mcp"

snapshot_tree() {
  python3 - "$1" "$2" <<'PY'
import os
import pathlib
import stat
import sys

root = pathlib.Path(sys.argv[1]).resolve(strict=True)
output = pathlib.Path(sys.argv[2])
rows: list[str] = []
for directory, names, files in os.walk(root, followlinks=False):
    base = pathlib.Path(directory)
    for name in sorted([*names, *files]):
        path = base / name
        metadata = path.lstat()
        relative = path.relative_to(root).as_posix()
        target = os.readlink(path) if stat.S_ISLNK(metadata.st_mode) else ""
        rows.append(
            "\t".join(
                (
                    relative,
                    str(stat.S_IFMT(metadata.st_mode)),
                    str(metadata.st_mode & 0o7777),
                    str(metadata.st_size),
                    str(metadata.st_mtime_ns),
                    target,
                )
            )
        )
output.write_text("\n".join(sorted(rows)) + "\n", encoding="utf-8")
PY
}

snapshot_tree "$supply_root" "$probe_root/supply.before"
snapshot_tree "$maps_root" "$probe_root/maps.before"

export CODEX_HOME="$profile_home"
export OPEN_WEB_CODEX_DATA_DIR="$profile_runtime"
export OPEN_WEB_CODEX_LOG_DIR="$profile_logs"
export OPEN_WEB_CODEX_SUPPLY_CHAIN_MCP_VENV="$supply_venv"
export OPEN_WEB_CODEX_MAPS_MCP_VENV="$maps_venv"
export MAPS_MCP_VENV="$maps_venv"
export OPEN_WEB_CODEX_MAPS_MCP_LAUNCHER_LOG="$profile_logs/maps-mcp-launcher.log"
export SUPPLY_CHAIN_MCP_AUTO_INSTALL=0
export MAPS_MCP_AUTO_INSTALL=0
export PYTHONDONTWRITEBYTECODE=1

SUPPLY_LAUNCHER="$supply_launcher" SUPPLY_ROOT="$supply_root" \
  "$supply_venv/bin/python" - <<'PY'
import asyncio
import os

from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client

REQUIRED = {
    "--data-server": {
        "discover_workspace_sources",
        "inspect_workspace_sources",
        "normalize_network_input",
        "validate_normalized_network_input",
        "load_administrative_catalog",
        "resolve_place_names",
        "build_administrative_candidates",
        "validate_points_within_boundaries",
    },
    "--demo-server": {"create_demo_workspace_sources"},
    "network": {
        "compute_optimal_assignment",
        "evaluate_service_targets",
        "summarize_network_cost",
        "compare_network_scenarios",
        "plan_route_matrix",
        "build_haversine_route_matrix",
        "validate_route_matrix",
        "register_navigation_route_matrix",
        "plan_cost_matrix",
        "evaluate_network_baseline",
        "evaluate_facility_scenario",
        "solve_p_median",
        "solve_service_constrained_location",
        "render_network_comparison_map",
    },
}


async def inspect(mode: str) -> None:
    root = os.environ["SUPPLY_ROOT"]
    args = ["--workspace-root", root]
    if mode != "network":
        args.insert(0, mode)
    params = StdioServerParameters(
        command=os.environ["SUPPLY_LAUNCHER"],
        args=args,
        cwd=root,
        env=dict(os.environ),
    )
    async with stdio_client(params) as streams:
        async with ClientSession(*streams) as session:
            await asyncio.wait_for(session.initialize(), timeout=20)
            names = {tool.name for tool in (await asyncio.wait_for(session.list_tools(), timeout=20)).tools}
            missing = REQUIRED[mode] - names
            assert not missing, f"{mode} missing tools: {sorted(missing)}"


async def main() -> None:
    for mode in ("--data-server", "--demo-server", "network"):
        await inspect(mode)


asyncio.run(main())
PY

MAPS_LAUNCHER="$maps_launcher" MAPS_ROOT="$maps_root" MAPS_STATE="$profile_runtime/mcp-state/maps-mcp" \
  "$maps_venv/bin/python" - <<'PY'
import asyncio
import os

from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client

EXPECTED = {
    "batch_geocode",
    "batch_reverse_geocode",
    "get_route",
    "distance_matrix",
    "create_map_card",
}


async def main() -> None:
    params = StdioServerParameters(
        command=os.environ["MAPS_LAUNCHER"],
        args=["--workspace-root", os.environ["MAPS_STATE"]],
        cwd=os.environ["MAPS_ROOT"],
        env=dict(os.environ),
    )
    async with stdio_client(params) as streams:
        async with ClientSession(*streams) as session:
            await asyncio.wait_for(session.initialize(), timeout=60)
            names = {tool.name for tool in (await asyncio.wait_for(session.list_tools(), timeout=30)).tools}
            assert names == EXPECTED, f"unexpected maps inventory: {sorted(names)}"


asyncio.run(main())
PY

snapshot_tree "$supply_root" "$probe_root/supply.after"
snapshot_tree "$maps_root" "$probe_root/maps.after"
cmp "$probe_root/supply.before" "$probe_root/supply.after" || {
  diff -u "$probe_root/supply.before" "$probe_root/supply.after" | head -200 >&2 || true
  printf 'supply-chain application assets changed during MCP startup\n' >&2
  exit 1
}
cmp "$probe_root/maps.before" "$probe_root/maps.after" || {
  diff -u "$probe_root/maps.before" "$probe_root/maps.after" | head -200 >&2 || true
  printf 'maps application assets changed during MCP startup\n' >&2
  exit 1
}

printf 'native MCP asset probe passed: role tool inventory is present and shared assets remained unchanged\n'
