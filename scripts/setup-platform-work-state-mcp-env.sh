#!/usr/bin/env bash
set -euo pipefail
umask 077

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
tools_root="$repo_root/tools/platform-work-state-mcp"
data_dir="${OPEN_WEB_CODEX_DATA_DIR:-$repo_root/.local/open-web-codex}"
venv_dir="${OPEN_WEB_CODEX_WORK_STATE_MCP_VENV:-$data_dir/tool-envs/platform-work-state}"
python_bin="${PYTHON_BIN:-python3}"

mkdir -p "$data_dir/tool-envs" "$data_dir/logs"
if [[ ! -x "$venv_dir/bin/python" ]]; then
  "$python_bin" -m venv "$venv_dir"
fi
if ! "$venv_dir/bin/python" -c 'import mcp' >/dev/null 2>&1; then
  "$venv_dir/bin/python" -m pip install --disable-pip-version-check -e "$tools_root"
fi
PYTHONPATH="$tools_root" "$venv_dir/bin/python" -m py_compile "$tools_root/server.py"
printf '%s\n' "$venv_dir"
