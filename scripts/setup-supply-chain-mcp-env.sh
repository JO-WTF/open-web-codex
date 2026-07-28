#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
tools_root="$repo_root/tools/supply-chain-network-planner"
data_dir="${OPEN_WEB_CODEX_DATA_DIR:-$repo_root/.local/open-web-codex}"
log_dir="${OPEN_WEB_CODEX_LOG_DIR:-$data_dir/logs}"
venv_dir="${OPEN_WEB_CODEX_SUPPLY_CHAIN_MCP_VENV:-$data_dir/tool-envs/supply-chain-network-planner}"
log_file="${OPEN_WEB_CODEX_SUPPLY_CHAIN_MCP_SETUP_LOG:-$log_dir/supply-chain-mcp-env.log}"
python_cmd="${PYTHON:-python3}"

mkdir -p "$log_dir" "$(dirname "$venv_dir")"

log() {
  printf '[%s] %s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" "$*" | tee -a "$log_file" >&2
}

run_logged() {
  log "+ $*"
  "$@" >>"$log_file" 2>&1
}

check_imports() {
  PYTHONPATH="$tools_root" "$venv_dir/bin/python" - <<'PY' >>"$log_file" 2>&1
try:
    import mcp  # noqa: F401
    import pydantic  # noqa: F401
    import supply_chain_planner.data_server  # noqa: F401
    import supply_chain_planner.server  # noqa: F401
except Exception as exc:
    print(f"supply-chain MCP import check failed: {type(exc).__name__}: {exc}")
    raise SystemExit(1)
print("supply-chain MCP imports ok")
PY
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

if [[ ! -x "$venv_dir/bin/python" ]]; then
  log "creating shared supply-chain MCP virtualenv"
  run_logged "$python_cmd" -m venv "$venv_dir"
else
  log "shared supply-chain MCP virtualenv already exists"
fi

run_logged "$venv_dir/bin/python" -m pip --version
if [[ "${OPEN_WEB_CODEX_REFRESH_SUPPLY_CHAIN_MCP_ENV:-0}" == "1" ]] || ! check_imports
then
  log "installing or refreshing supply-chain MCP dependencies"
  run_logged "$venv_dir/bin/python" -m pip install --disable-pip-version-check -e "$tools_root"
else
  log "supply-chain MCP dependencies already import successfully"
fi
log "verifying supply-chain MCP imports after setup"
check_imports
log "supply-chain MCP environment setup complete"
printf '%s\n' "$venv_dir"
