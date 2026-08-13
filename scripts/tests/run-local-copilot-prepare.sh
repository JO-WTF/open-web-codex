#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
launcher="$repo_root/scripts/run-local.sh"

bash -n "$launcher"

[[ "$(grep -c -- '-m copilot_sdk prepare' "$launcher")" == "1" ]]
grep -F -- 'warehouse_copilot_package_root="$repo_root/copilots/warehouse-network"' "$launcher" >/dev/null
grep -F -- 'meeting_copilot_package_root="$repo_root/copilots/meeting-action-review"' "$launcher" >/dev/null
grep -F -- 'copilot_sdk_environment_root="$data_dir/sdk-environments/copilot"' "$launcher" >/dev/null
grep -F -- '-e "$repo_root/tools/copilot-provider-sdk"' "$launcher" >/dev/null
grep -F -- '-e "$repo_root/tools/copilot-sdk"' "$launcher" >/dev/null
grep -F -- '"$copilot_sdk_python" -m copilot_sdk prepare "$package_root"' "$launcher" >/dev/null
grep -F -- '--output-root "$environment_root"' "$launcher" >/dev/null
! grep -F -- 'PYTHONPATH=' "$launcher" >/dev/null
grep -F -- '--copilot-package-source "warehouse-network-copilot" "$warehouse_copilot_package_root" "$warehouse_copilot_prepared_descriptor"' "$launcher" >/dev/null
grep -F -- '--copilot-package-source "meeting-action-review" "$meeting_copilot_package_root" "$meeting_copilot_prepared_descriptor"' "$launcher" >/dev/null
grep -F -- '--default-copilot-package "warehouse-network-copilot"' "$launcher" >/dev/null

for obsolete in \
  OPEN_WEB_CODEX_MAPS_ASSET_ROOT \
  OPEN_WEB_CODEX_MAPS_MCP_VENV \
  OPEN_WEB_CODEX_SKIP_MAPS_MCP_SETUP \
  OPEN_WEB_CODEX_SUPPLY_CHAIN_ASSET_ROOT \
  OPEN_WEB_CODEX_SUPPLY_CHAIN_MCP_VENV \
  OPEN_WEB_CODEX_SKIP_SUPPLY_CHAIN_MCP_SETUP \
  setup-maps-mcp-env.sh \
  setup-supply-chain-mcp-env.sh \
  apps/web/builtin/warehouse-network-copilot \
  tools/maps-mcp \
  tools/supply-chain-network-planner
do
  ! grep -F -- "$obsolete" "$launcher" >/dev/null
done

printf 'run-local generic Copilot preparation contract passed\n'
