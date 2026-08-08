#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
tools_root="$repo_root/tools/platform-coordination-mcp"
data_dir="${OPEN_WEB_CODEX_DATA_DIR:-$repo_root/.local/open-web-codex}"
venv_dir="${OPEN_WEB_CODEX_COORDINATION_MCP_VENV:-$data_dir/tool-envs/platform-coordination}"
log_dir="${OPEN_WEB_CODEX_LOG_DIR:-$data_dir/logs}"
log_file="${OPEN_WEB_CODEX_COORDINATION_MCP_SETUP_LOG:-$log_dir/platform-coordination-mcp-env.log}"
python_cmd="${PYTHON:-python3}"

mkdir -p "$log_dir" "$(dirname "$venv_dir")"
log() { printf '[%s] %s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" "$*" >>"$log_file"; }
run_logged() { log "+ $*"; "$@" >>"$log_file" 2>&1; }
check_imports() {
  "$venv_dir/bin/python" - <<'PY' >>"$log_file" 2>&1
import mcp
from mcp.server.fastmcp import FastMCP
print(f"platform coordination MCP imports ok: {mcp.__file__}")
PY
}

command -v "$python_cmd" >/dev/null 2>&1 || { log "python command not found: $python_cmd"; exit 127; }
if [[ ! -x "$venv_dir/bin/python" ]]; then
  run_logged "$python_cmd" -m venv "$venv_dir"
fi
if [[ "${OPEN_WEB_CODEX_REFRESH_COORDINATION_MCP_ENV:-0}" == "1" ]] || ! check_imports; then
  run_logged "$venv_dir/bin/python" -m pip install --disable-pip-version-check -e "$tools_root"
fi
check_imports
printf '%s\n' "$venv_dir"
