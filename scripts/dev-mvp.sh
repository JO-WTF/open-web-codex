#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
web_root="$repo_root/apps/web"
daemon_root="$web_root/src-tauri"
runtime_root="$repo_root/codex/codex-rs"
data_dir="${OPEN_WEB_CODEX_DATA_DIR:-$repo_root/.cache/mvp}"
gateway_port="${OPEN_WEB_CODEX_GATEWAY_PORT:-4733}"
rpc_port="${OPEN_WEB_CODEX_RPC_PORT:-4732}"
server_port="${OPEN_WEB_CODEX_SERVER_PORT:-4800}"
web_port="${OPEN_WEB_CODEX_WEB_PORT:-1420}"

for command in cargo npm curl; do
  if ! command -v "$command" >/dev/null 2>&1; then
    printf 'Missing required command: %s\n' "$command" >&2
    exit 1
  fi
done

codex_bin="${OPEN_WEB_CODEX_BIN:-}"
if [[ -z "$codex_bin" && -x "$runtime_root/target/debug/codex" ]]; then
  codex_bin="$runtime_root/target/debug/codex"
fi
if [[ -z "$codex_bin" ]] && command -v codex >/dev/null 2>&1; then
  codex_bin="$(command -v codex)"
fi
if [[ -z "$codex_bin" ]]; then
  printf 'Building the repository Codex runtime...\n'
  (cd "$runtime_root" && cargo build -p codex-cli --bin codex)
  codex_bin="$runtime_root/target/debug/codex"
fi
if [[ ! -x "$codex_bin" ]]; then
  printf 'Codex binary is not executable: %s\n' "$codex_bin" >&2
  exit 1
fi

printf 'Building the local Web gateway (Tauri daemon)...\n'
(cd "$daemon_root" && cargo build --no-default-features --bin codex_monitor_daemon)
daemon_bin="$daemon_root/target/debug/codex_monitor_daemon"

printf 'Building the platform server (new architecture)...\n'
(cd "$web_root" && cargo build -p open-web-codex-server)
server_bin="$web_root/target/debug/open-web-codex-server"

if [[ ! -d "$web_root/node_modules" ]]; then
  printf 'Installing Web dependencies...\n'
  (cd "$web_root" && npm ci)
fi

mkdir -p "$data_dir/logs"
daemon_pid=""
server_pid=""
web_pid=""

cleanup() {
  if [[ -n "$web_pid" ]]; then
    kill "$web_pid" 2>/dev/null || true
  fi
  if [[ -n "$server_pid" ]]; then
    kill "$server_pid" 2>/dev/null || true
  fi
  if [[ -n "$daemon_pid" ]]; then
    kill "$daemon_pid" 2>/dev/null || true
  fi
}
trap cleanup EXIT INT TERM

printf 'Starting the loopback gateway (Tauri daemon)...\n'
PATH="$(dirname "$codex_bin"):$PATH" \
  "$daemon_bin" \
  --listen "127.0.0.1:$rpc_port" \
  --web-listen "127.0.0.1:$gateway_port" \
  --data-dir "$data_dir" \
  --insecure-no-auth \
  >"$data_dir/logs/daemon.log" 2>&1 &
daemon_pid=$!

health_url="http://127.0.0.1:$gateway_port/api/health"
for _ in $(seq 1 60); do
  if curl --silent --fail "$health_url" >/dev/null 2>&1; then
    break
  fi
  if ! kill -0 "$daemon_pid" 2>/dev/null; then
    printf 'Gateway failed to start. See %s\n' "$data_dir/logs/daemon.log" >&2
    exit 1
  fi
  sleep 0.25
done
curl --silent --fail "$health_url" >/dev/null
printf 'Daemon health check passed.\n'

printf 'Starting the platform server (new architecture)...\n'
"$server_bin" \
  --bind "127.0.0.1:$server_port" \
  --daemon-url "http://127.0.0.1:$gateway_port" \
  --migrate \
  >"$data_dir/logs/server.log" 2>&1 &
server_pid=$!

server_health_url="http://127.0.0.1:$server_port/api/health"
for _ in $(seq 1 60); do
  if curl --silent --fail "$server_health_url" >/dev/null 2>&1; then
    break
  fi
  if ! kill -0 "$server_pid" 2>/dev/null; then
    printf 'Platform server failed to start. See %s\n' "$data_dir/logs/server.log" >&2
    exit 1
  fi
  sleep 0.25
done
curl --silent --fail "$server_health_url" >/dev/null
printf 'Platform server health check passed.\n'

printf 'Starting the Web client (pointing to platform server)...\n'
(
  cd "$web_root"
  VITE_CODEX_MONITOR_WEB_API="http://127.0.0.1:$server_port" \
    npm run dev -- --host 127.0.0.1 --port "$web_port"
) >"$data_dir/logs/web.log" 2>&1 &
web_pid=$!

web_url="http://127.0.0.1:$web_port/web"
for _ in $(seq 1 60); do
  if curl --silent --fail "$web_url" >/dev/null 2>&1; then
    break
  fi
  if ! kill -0 "$web_pid" 2>/dev/null; then
    printf 'Web client failed to start. See %s\n' "$data_dir/logs/web.log" >&2
    exit 1
  fi
  sleep 0.25
done
curl --silent --fail "$web_url" >/dev/null

printf '\n=== open-web-codex MVP is running. ===\n'
printf 'Web UI:  %s\n' "$web_url"
printf 'Server:  %s (health: %s)\n' "$server_health_url" "$(curl --silent "$server_health_url")"
printf 'Daemon:  %s (health: %s)\n' "$health_url" "$(curl --silent "$health_url")"
printf 'Codex:   %s\n' "$codex_bin"
printf 'Data:    %s\n' "$data_dir"
printf 'Press Ctrl-C to stop all processes.\n\n'

while kill -0 "$daemon_pid" 2>/dev/null && kill -0 "$server_pid" 2>/dev/null && kill -0 "$web_pid" 2>/dev/null; do
  sleep 1
done

printf 'An MVP process exited. Check logs under %s/logs.\n' "$data_dir" >&2
exit 1
