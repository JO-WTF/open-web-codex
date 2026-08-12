#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
tools_root="${OPEN_WEB_CODEX_SUPPLY_CHAIN_ASSET_ROOT:-$repo_root/tools/supply-chain-network-planner}"
data_dir="${OPEN_WEB_CODEX_DATA_DIR:-$repo_root/.local/open-web-codex}"
log_dir="${OPEN_WEB_CODEX_LOG_DIR:-$data_dir/logs}"
venv_dir="${OPEN_WEB_CODEX_SUPPLY_CHAIN_MCP_VENV:-$data_dir/tool-envs/supply-chain-network-planner}"
log_file="${OPEN_WEB_CODEX_SUPPLY_CHAIN_MCP_SETUP_LOG:-$log_dir/supply-chain-mcp-env.log}"
python_cmd="${PYTHON:-python3}"
dependency_manifest="$tools_root/pyproject.toml"
dependency_stamp="$venv_dir/.open-web-codex-pyproject.cksum"

mkdir -p "$log_dir" "$(dirname "$venv_dir")"

log() {
  printf '[%s] %s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" "$*" | tee -a "$log_file" >&2
}

run_logged() {
  log "+ $*"
  "$@" >>"$log_file" 2>&1
}

manifest_fingerprint() {
  cksum "$dependency_manifest" | awk '{print $1 ":" $2}'
}

manifest_stamp_matches() {
  [[ -r "$dependency_stamp" ]] && [[ "$(<"$dependency_stamp")" == "$dependency_fingerprint" ]]
}

check_environment() {
  if ! PYTHONPATH="$tools_root" "$venv_dir/bin/python" - <<'PY' >>"$log_file" 2>&1
try:
    from importlib.metadata import version

    version("open-web-codex-supply-chain-network-planner")
    import mcp  # noqa: F401
    import pydantic  # noqa: F401
    import supply_chain_planner.data_server  # noqa: F401
    import supply_chain_planner.server  # noqa: F401
except Exception as exc:
    print(f"supply-chain MCP import check failed: {type(exc).__name__}: {exc}")
    raise SystemExit(1)
print("supply-chain MCP imports ok")
PY
  then
    return 1
  fi
  "$venv_dir/bin/python" -m pip check >>"$log_file" 2>&1
}

log "supply-chain MCP environment setup starting"
log "tools_root=$tools_root"
log "venv_dir=$venv_dir"
log "python=$python_cmd"

command -v "$python_cmd" >/dev/null 2>&1 || {
  log "python command not found: $python_cmd"
  exit 127
}
run_logged "$python_cmd" --version
[[ -f "$dependency_manifest" ]] || {
  log "dependency manifest not found: $dependency_manifest"
  exit 66
}
dependency_fingerprint="$(manifest_fingerprint)"

if [[ ! -x "$venv_dir/bin/python" ]]; then
  log "creating shared supply-chain MCP virtualenv"
  run_logged "$python_cmd" -m venv "$venv_dir"
else
  log "shared supply-chain MCP virtualenv already exists"
fi

run_logged "$venv_dir/bin/python" -m pip --version
refresh_reason=""
if [[ "${OPEN_WEB_CODEX_REFRESH_SUPPLY_CHAIN_MCP_ENV:-0}" == "1" ]]; then
  refresh_reason="explicit refresh requested"
elif ! manifest_stamp_matches; then
  refresh_reason="dependency manifest changed or was not previously recorded"
elif ! check_environment; then
  refresh_reason="installed environment does not satisfy declared dependencies"
fi
if [[ -n "$refresh_reason" ]]; then
  log "$refresh_reason"
  log "installing or refreshing supply-chain MCP dependencies"
  run_logged "$venv_dir/bin/python" -m pip install --disable-pip-version-check -e "$tools_root"
else
  log "supply-chain MCP dependencies already satisfy the current manifest"
fi
log "verifying supply-chain MCP environment after setup"
check_environment
printf '%s\n' "$dependency_fingerprint" >"$dependency_stamp"
log "supply-chain MCP environment setup complete"
printf '%s\n' "$venv_dir"
