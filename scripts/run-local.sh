#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
script_path="$repo_root/scripts/run-local.sh"
web_root="$repo_root/apps/web"
daemon_root="$web_root/src-tauri"
runtime_root="$repo_root/codex/codex-rs"

data_dir="${OPEN_WEB_CODEX_DATA_DIR:-$repo_root/.cache/mvp}"
gateway_port="${OPEN_WEB_CODEX_GATEWAY_PORT:-4733}"
rpc_port="${OPEN_WEB_CODEX_RPC_PORT:-4732}"
server_port="${OPEN_WEB_CODEX_SERVER_PORT:-4800}"
web_port="${OPEN_WEB_CODEX_WEB_PORT:-1420}"
codex_mode="${CODEX_MODE:-real}"
skip_build="${OPEN_WEB_CODEX_SKIP_BUILD:-0}"

run_dir="$data_dir/run"
log_dir="$data_dir/logs"
supervisor_pid_file="$run_dir/run-local.pid"
supervisor_log="$log_dir/run-local.log"

action="run"
supervisor_pid="$$"

usage() {
  cat <<'EOF'
Usage: ./scripts/run-local.sh [option]

Options:
  --background  Start the stack in the background.
  --restart     Stop the current stack and restart it in the background.
  --stop        Stop the stack recorded for OPEN_WEB_CODEX_DATA_DIR.
  --status      Show supervisor and endpoint status.
  --no-build    Reuse existing Rust binaries instead of compiling them.
  -h, --help    Show this help.

Environment:
  CODEX_MODE                     real (default) or fake
  OPEN_WEB_CODEX_BIN             Explicit Codex binary; skips repository Codex build
  OPEN_WEB_CODEX_SKIP_BUILD      Set to 1 to reuse all existing Rust binaries
  OPEN_WEB_CODEX_DATA_DIR        Runtime state and logs directory
  OPEN_WEB_CODEX_{RPC,GATEWAY,SERVER,WEB}_PORT
EOF
}

while (($# > 0)); do
  case "$1" in
    --background)
      action="background"
      ;;
    --restart)
      action="restart"
      ;;
    --stop)
      action="stop"
      ;;
    --status)
      action="status"
      ;;
    --no-build)
      skip_build="1"
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      printf 'Unknown option: %s\n\n' "$1" >&2
      usage >&2
      exit 2
      ;;
  esac
  shift
done

case "$skip_build" in
  0|1) ;;
  *)
    printf 'OPEN_WEB_CODEX_SKIP_BUILD must be 0 or 1, got: %s\n' "$skip_build" >&2
    exit 2
    ;;
esac

case "$codex_mode" in
  real|fake) ;;
  *)
    printf 'CODEX_MODE must be real or fake, got: %s\n' "$codex_mode" >&2
    exit 2
    ;;
esac

read_supervisor_pid() {
  if [[ -f "$supervisor_pid_file" ]]; then
    tr -d '[:space:]' <"$supervisor_pid_file"
  fi
}

is_live_pid() {
  local pid="${1:-}"
  [[ "$pid" =~ ^[0-9]+$ ]] && kill -0 "$pid" 2>/dev/null
}

is_supervisor_pid() {
  local pid="${1:-}"
  local command_line
  if ! is_live_pid "$pid"; then
    return 1
  fi
  command_line="$(ps -p "$pid" -o command= 2>/dev/null || true)"
  [[ "$command_line" == *run-local.sh* ]]
}

remove_stale_pid_file() {
  local pid
  pid="$(read_supervisor_pid)"
  if [[ -n "$pid" ]] && ! is_supervisor_pid "$pid"; then
    rm -f "$supervisor_pid_file"
  fi
}

endpoint_status() {
  local label="$1"
  local url="$2"
  if curl --silent --fail "$url" >/dev/null 2>&1; then
    printf '%-9s healthy  %s\n' "$label" "$url"
  else
    printf '%-9s offline  %s\n' "$label" "$url"
  fi
}

show_status() {
  local pid
  remove_stale_pid_file
  pid="$(read_supervisor_pid)"
  if is_supervisor_pid "$pid"; then
    printf 'Supervisor running (PID %s)\n' "$pid"
  else
    printf 'Supervisor not running\n'
  fi
  if command -v curl >/dev/null 2>&1; then
    if [[ "$codex_mode" == "real" ]]; then
      endpoint_status "Gateway" "http://127.0.0.1:$gateway_port/api/health"
    fi
    endpoint_status "Server" "http://127.0.0.1:$server_port/api/health"
    endpoint_status "Web" "http://127.0.0.1:$web_port/web"
  fi
}

stop_stack() {
  local pid
  local attempt
  remove_stale_pid_file
  pid="$(read_supervisor_pid)"
  if ! is_supervisor_pid "$pid"; then
    printf 'open-web-codex is not running for data directory: %s\n' "$data_dir"
    return 0
  fi

  printf 'Stopping open-web-codex supervisor (PID %s)...\n' "$pid"
  kill -TERM "$pid"
  for attempt in $(seq 1 100); do
    if ! is_live_pid "$pid"; then
      rm -f "$supervisor_pid_file"
      printf 'Stopped.\n'
      return 0
    fi
    sleep 0.1
  done

  printf 'Supervisor did not stop within 10 seconds; PID %s is still running.\n' "$pid" >&2
  return 1
}

start_background() {
  local existing_pid
  local child_pid
  local attempt

  mkdir -p "$run_dir" "$log_dir"
  remove_stale_pid_file
  existing_pid="$(read_supervisor_pid)"
  if is_supervisor_pid "$existing_pid"; then
    printf 'open-web-codex is already running (PID %s).\n' "$existing_pid" >&2
    return 1
  fi

  if [[ "$skip_build" == "1" ]]; then
    nohup "$script_path" --no-build >"$supervisor_log" 2>&1 </dev/null &
  else
    nohup "$script_path" >"$supervisor_log" 2>&1 </dev/null &
  fi
  child_pid=$!

  for attempt in $(seq 1 50); do
    existing_pid="$(read_supervisor_pid)"
    if is_supervisor_pid "$existing_pid"; then
      printf 'open-web-codex is starting in the background (PID %s).\n' "$existing_pid"
      printf 'Logs: %s\n' "$supervisor_log"
      printf 'Status: %s --status\n' "$script_path"
      return 0
    fi
    if ! is_live_pid "$child_pid"; then
      printf 'Background startup failed. See %s\n' "$supervisor_log" >&2
      return 1
    fi
    sleep 0.1
  done

  printf 'Background process started but did not register its PID. See %s\n' "$supervisor_log" >&2
  return 1
}

case "$action" in
  background)
    start_background
    exit $?
    ;;
  restart)
    stop_stack
    start_background
    exit $?
    ;;
  stop)
    stop_stack
    exit $?
    ;;
  status)
    show_status
    exit 0
    ;;
esac

required_commands=(npm curl)
if [[ "$skip_build" == "0" ]]; then
  required_commands+=(cargo)
fi
for command_name in "${required_commands[@]}"; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    printf 'Missing required command: %s\n' "$command_name" >&2
    exit 1
  fi
done

mkdir -p "$run_dir" "$log_dir"
remove_stale_pid_file
existing_pid="$(read_supervisor_pid)"
if is_supervisor_pid "$existing_pid" && [[ "$existing_pid" != "$supervisor_pid" ]]; then
  printf 'open-web-codex is already running (PID %s).\n' "$existing_pid" >&2
  exit 1
fi
printf '%s\n' "$supervisor_pid" >"$supervisor_pid_file"

daemon_pid=""
server_pid=""
web_pid=""

child_pids() {
  local parent_pid="$1"
  ps -eo pid=,ppid= | awk -v parent="$parent_pid" '$2 == parent { print $1 }'
}

terminate_tree() {
  local pid="$1"
  local child_pid
  if ! is_live_pid "$pid"; then
    return 0
  fi
  while IFS= read -r child_pid; do
    if [[ -n "$child_pid" ]]; then
      terminate_tree "$child_pid"
    fi
  done < <(child_pids "$pid")
  kill -TERM "$pid" 2>/dev/null || true
}

cleanup() {
  local recorded_pid
  trap - EXIT INT TERM
  for process_pid in "$web_pid" "$server_pid" "$daemon_pid"; do
    if [[ -n "$process_pid" ]]; then
      terminate_tree "$process_pid"
    fi
  done
  for process_pid in "$web_pid" "$server_pid" "$daemon_pid"; do
    if [[ -n "$process_pid" ]]; then
      wait "$process_pid" 2>/dev/null || true
    fi
  done
  recorded_pid="$(read_supervisor_pid)"
  if [[ "$recorded_pid" == "$supervisor_pid" ]]; then
    rm -f "$supervisor_pid_file"
  fi
}

trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

require_executable() {
  local label="$1"
  local path="$2"
  if [[ ! -x "$path" ]]; then
    printf '%s binary is not executable: %s\n' "$label" "$path" >&2
    exit 1
  fi
}

wait_for_health() {
  local label="$1"
  local pid="$2"
  local url="$3"
  local log_file="$4"
  local attempt
  for attempt in $(seq 1 120); do
    if curl --silent --fail "$url" >/dev/null 2>&1; then
      printf '%s health check passed.\n' "$label"
      return 0
    fi
    if ! is_live_pid "$pid"; then
      printf '%s failed to start. See %s\n' "$label" "$log_file" >&2
      return 1
    fi
    sleep 0.25
  done
  printf '%s did not become healthy within 30 seconds. See %s\n' "$label" "$log_file" >&2
  return 1
}

printf 'Using codex mode: %s\n' "$codex_mode"
if [[ "$skip_build" == "1" ]]; then
  printf 'Rust builds: skipped by request\n'
fi

codex_bin=""
daemon_bin="$web_root/target/debug/codex_monitor_daemon"
server_bin="$web_root/target/debug/open-web-codex-server"

if [[ "$codex_mode" == "real" ]]; then
  if [[ -n "${OPEN_WEB_CODEX_BIN:-}" ]]; then
    codex_bin="$OPEN_WEB_CODEX_BIN"
    printf 'Using explicit Codex binary: %s\n' "$codex_bin"
  else
    codex_bin="$runtime_root/target/debug/codex"
    if [[ "$skip_build" == "0" ]]; then
      printf 'Building the repository Codex runtime (incremental)...\n'
      (cd "$runtime_root" && cargo build -p codex-cli --bin codex)
    fi
  fi
  require_executable "Codex" "$codex_bin"

  if [[ "$skip_build" == "0" ]]; then
    printf 'Building the local Web gateway...\n'
    (cd "$daemon_root" && cargo build --no-default-features --bin codex_monitor_daemon)
  fi
  require_executable "Gateway" "$daemon_bin"
fi

if [[ "$skip_build" == "0" ]]; then
  printf 'Building the platform server...\n'
  (cd "$web_root" && cargo build -p open-web-codex-server)
fi
require_executable "Platform server" "$server_bin"

if [[ ! -d "$web_root/node_modules" ]]; then
  printf 'Installing Web dependencies...\n'
  (cd "$web_root" && npm ci)
fi

if [[ "$codex_mode" == "real" ]]; then
  printf 'Starting the loopback gateway...\n'
  PATH="$(dirname "$codex_bin"):$PATH" \
    "$daemon_bin" \
    --listen "127.0.0.1:$rpc_port" \
    --web-listen "127.0.0.1:$gateway_port" \
    --data-dir "$data_dir" \
    --insecure-no-auth \
    >"$log_dir/daemon.log" 2>&1 &
  daemon_pid=$!
  gateway_health_url="http://127.0.0.1:$gateway_port/api/health"
  wait_for_health "Gateway" "$daemon_pid" "$gateway_health_url" "$log_dir/daemon.log"
fi

printf 'Starting the platform server...\n'
server_args=(
  --bind "127.0.0.1:$server_port"
  --codex-mode "$codex_mode"
  --migrate
)
if [[ "$codex_mode" == "real" ]]; then
  server_args+=(--daemon-url "http://127.0.0.1:$gateway_port")
fi
CODEX_MODE="$codex_mode" \
  "$server_bin" "${server_args[@]}" \
  >"$log_dir/server.log" 2>&1 &
server_pid=$!
server_health_url="http://127.0.0.1:$server_port/api/health"
wait_for_health "Platform server" "$server_pid" "$server_health_url" "$log_dir/server.log"

printf 'Starting the Web client...\n'
(
  cd "$web_root"
  VITE_CODEX_MONITOR_WEB_API="http://127.0.0.1:$server_port" \
    npm run dev -- --host 127.0.0.1 --port "$web_port"
) >"$log_dir/web.log" 2>&1 &
web_pid=$!
web_url="http://127.0.0.1:$web_port/web"
wait_for_health "Web client" "$web_pid" "$web_url" "$log_dir/web.log"

printf '\n=== open-web-codex is running ===\n'
printf 'Web UI:  %s\n' "$web_url"
printf 'Server:  %s\n' "$server_health_url"
printf 'Mode:    %s\n' "$codex_mode"
if [[ "$codex_mode" == "real" ]]; then
  printf 'Gateway: %s\n' "$gateway_health_url"
  printf 'Codex:   %s\n' "$codex_bin"
fi
printf 'Data:    %s\n' "$data_dir"
printf 'Logs:    %s\n' "$log_dir"
printf 'Stop:    %s --stop\n\n' "$script_path"

while true; do
  if ! is_live_pid "$server_pid"; then
    printf 'Platform server exited. See %s\n' "$log_dir/server.log" >&2
    exit 1
  fi
  if ! is_live_pid "$web_pid"; then
    printf 'Web client exited. See %s\n' "$log_dir/web.log" >&2
    exit 1
  fi
  if [[ "$codex_mode" == "real" ]] && ! is_live_pid "$daemon_pid"; then
    printf 'Gateway exited. See %s\n' "$log_dir/daemon.log" >&2
    exit 1
  fi
  sleep 1
done
