#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
launcher="$repo_root/scripts/run-local.sh"

bash -n "$launcher"

[[ "$(grep -c -- '-m copilot_sdk prepare' "$launcher")" == "1" ]]
grep -F -- '--manifest apps/web/builtin/warehouse-network-copilot/copilot.toml' "$launcher" >/dev/null
grep -F -- '--output-root "$copilot_environment_root"' "$launcher" >/dev/null
grep -F -- '--copilot-prepared-descriptor "$copilot_prepared_descriptor"' "$launcher" >/dev/null

for obsolete in \
  OPEN_WEB_CODEX_MAPS_ASSET_ROOT \
  OPEN_WEB_CODEX_MAPS_MCP_VENV \
  OPEN_WEB_CODEX_SKIP_MAPS_MCP_SETUP \
  OPEN_WEB_CODEX_SUPPLY_CHAIN_ASSET_ROOT \
  OPEN_WEB_CODEX_SUPPLY_CHAIN_MCP_VENV \
  OPEN_WEB_CODEX_SKIP_SUPPLY_CHAIN_MCP_SETUP \
  setup-maps-mcp-env.sh \
  setup-supply-chain-mcp-env.sh \
  tools/maps-mcp \
  tools/supply-chain-network-planner
do
  ! grep -F -- "$obsolete" "$launcher" >/dev/null
done

printf 'run-local generic Copilot preparation contract passed\n'
