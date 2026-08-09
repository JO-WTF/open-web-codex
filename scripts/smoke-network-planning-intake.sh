#!/usr/bin/env bash
set -euo pipefail

# This is the release gate for the Workspace-wide Network Planning journey.
# Local mode exercises deterministic contracts.  Real mode is intentionally
# strict and never accepts an arbitrary shell command as evidence: the current
# repository still needs a dedicated browser + Runtime + MCP harness before the
# capability can be promoted.

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
python_bin="${repo_root}/.local/open-web-codex/tool-envs/supply-chain-network-planner/bin/python"
mode="${NETWORK_PLANNING_SMOKE_MODE:-real}"

if [[ ! -x "${python_bin}" ]]; then
  echo '{"code":"planner_tool_env_unavailable","message":"The supply-chain planner tool environment is not installed."}' >&2
  exit 2
fi

if [[ "${mode}" == "local" ]]; then
  RUN_REAL_STDIO_SMOKE=1 "${python_bin}" -m pytest -q \
    "${repo_root}/tools/supply-chain-network-planner/tests/test_workspace_intake.py" \
    "${repo_root}/tools/supply-chain-network-planner/tests/test_data_core.py::test_builds_planning_dataset_and_network_handoff" \
    "${repo_root}/tools/supply-chain-network-planner/tests/stdio_smoke.py"
  (cd "${repo_root}/apps/web" && npm run typecheck)
  echo 'Workspace intake local contract smoke passed; real Web + Runtime + MCP E2E remains a separate gate.'
  exit 0
fi

if [[ "${mode}" != "real" ]]; then
  echo '{"code":"invalid_network_planning_smoke_mode","message":"Use NETWORK_PLANNING_SMOKE_MODE=local or real."}' >&2
  exit 2
fi

required=(DATABASE_URL E2E_BASE_URL E2E_PROVIDER_ID E2E_MODEL)
missing=()
for name in "${required[@]}"; do
  [[ -n "${!name:-}" ]] || missing+=("${name}")
done
if ((${#missing[@]} > 0)); then
  printf '{"code":"network_planning_real_e2e_unavailable","missing":[' >&2
  printf '"%s",' "${missing[@]}" | sed 's/,$//' >&2
  printf '],"message":"Provide the real Web, Runtime, MCP and Provider prerequisites; no synthetic success is reported."}\n' >&2
  exit 2
fi

if [[ -n "${NETWORK_PLANNING_E2E_COMMAND:-}" ]]; then
  echo '{"code":"network_planning_real_e2e_command_rejected","message":"Arbitrary shell commands are not accepted as real journey evidence. Use the checked-in browser harness when it is available."}' >&2
  exit 2
fi

echo '{"code":"network_planning_real_e2e_unavailable","message":"The repository does not yet contain the dedicated current Web + Runtime + MCP browser harness. Local contract smoke passed separately; capability remains unavailable."}' >&2
exit 2
