#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
launcher="$repo_root/scripts/run-local.sh"

bash -n "$launcher"

[[ "$(grep -c -- '-m copilot_sdk prepare' "$launcher")" == "1" ]]
grep -F -- 'copilots_root="$repo_root/copilots"' "$launcher" >/dev/null
grep -F -- 'copilot_tool_registry_root="$repo_root/tools"' "$launcher" >/dev/null
grep -F -- 'copilot_prepared_root="$data_dir/tool-environments"' "$launcher" >/dev/null
grep -F -- 'copilot_build_store_root="$data_dir/tool-builds"' "$launcher" >/dev/null
grep -F -- 'copilot_sdk_environment_root="$data_dir/sdk-environments/copilot"' "$launcher" >/dev/null
grep -F -- '-e "$repo_root/packages/copilot-provider-sdk"' "$launcher" >/dev/null
grep -F -- '-e "$repo_root/packages/copilot-sdk"' "$launcher" >/dev/null
grep -F -- 'source := pathlib.Path(p)).resolve()' "$launcher" >/dev/null
grep -F -- '"$copilot_sdk_python" -m copilot_sdk prepare "$package_root"' "$launcher" >/dev/null
grep -F -- '--tool-registry-root "$copilot_tool_registry_root"' "$launcher" >/dev/null
grep -F -- '--output-root "$environment_root"' "$launcher" >/dev/null
grep -F -- '--build-store-root "$copilot_build_store_root"' "$launcher" >/dev/null
grep -F -- 'copilot_sdk gc-builds' "$launcher" >/dev/null
grep -F -- '--prepared-root "$copilot_prepared_root"' "$launcher" >/dev/null
grep -F -- '--packages-root "$copilots_root"' "$launcher" >/dev/null
grep -F -- 'gc_args+=(--active-package-id "$package_id")' "$launcher" >/dev/null
python3 - "$launcher" <<'PY'
from pathlib import Path
import sys

text = Path(sys.argv[1]).read_text(encoding="utf-8")
start = text.index("prepare_copilot_environments()")
loop_end = text.index("done < <(find \"$copilots_root\"", start)
gc = text.index('copilot_sdk gc-builds', start)
assert gc > loop_end, "GC must run only after every package prepare loop succeeds"
PY
grep -F -- 'find "$copilots_root" -mindepth 2 -maxdepth 2 -type f -name copilot.toml' "$launcher" >/dev/null
! grep -F -- 'PYTHONPATH=' "$launcher" >/dev/null
grep -F -- '--copilots-root "$copilots_root"' "$launcher" >/dev/null
grep -F -- '--copilot-prepared-root "$copilot_prepared_root"' "$launcher" >/dev/null
grep -F -- '--copilot-build-store-root "$copilot_build_store_root"' "$launcher" >/dev/null

for hard_coded_package in \
  warehouse_copilot_package_root \
  meeting_copilot_package_root \
  --copilot-package-source \
  --default-copilot-package
do
  ! grep -F -- "$hard_coded_package" "$launcher" >/dev/null
done

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

"$repo_root/scripts/tests/run-local-copilot-prepare-runtime.sh"

printf 'run-local generic Copilot preparation contract passed\n'
