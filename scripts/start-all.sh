#!/bin/bash
# start-all.sh — One-click startup for open-web-codex MVP
# Usage:
#   bash scripts/start-all.sh          # start all services
#   bash scripts/start-all.sh --stop   # stop all services
#
# Optional environment variables:
#   OPEN_WEB_CODEX_SERVER_PORT   Platform API port (default: 4800)
#   CODEX_MODE                   Platform codex adapter mode: real|fake (default: real)
#   DATABASE_URL                 PostgreSQL connection string for platform server

set -euo pipefail

STOP_MODE=false
if [ "${1:-}" = "--stop" ] || [ "${1:-}" = "stop" ]; then
  STOP_MODE=true
fi

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WEB_ROOT="$ROOT/apps/web"
DAEMON_BIN="$WEB_ROOT/target/debug/codex_monitor_daemon"
SERVER_BIN="$WEB_ROOT/target/debug/open-web-codex-server"
LOCAL_CODEX_BIN="$ROOT/codex/codex-rs/target/debug/codex"
DATA_DIR="${OPEN_WEB_CODEX_DATA_DIR:-$HOME/Library/Application Support/com.dimillian.codexmonitor}"
SERVER_PORT="${OPEN_WEB_CODEX_SERVER_PORT:-4800}"
CODEX_MODE="${CODEX_MODE:-real}"
GATEWAY_PORT="${OPEN_WEB_CODEX_GATEWAY_PORT:-4733}"
RPC_PORT="${OPEN_WEB_CODEX_RPC_PORT:-4732}"
WEB_PORT="${OPEN_WEB_CODEX_WEB_PORT:-1421}"

# ── Stop mode ──
if $STOP_MODE; then
  echo "=== Stopping all services ==="
  pkill -f "open-web-codex-server" 2>/dev/null && echo "  ✓ platform server stopped" || echo "  - platform server not running"
  pkill -f "codex_monitor_daemon" 2>/dev/null && echo "  ✓ daemon stopped" || echo "  - daemon not running"
  pkill -f "codex app-server"     2>/dev/null && echo "  ✓ codex app-server stopped" || echo "  - app-server not running"
  pkill -f "vite.*--port ${WEB_PORT}" 2>/dev/null && echo "  ✓ vite stopped" || echo "  - vite not running"
  pkill -f "esbuild.*${WEB_PORT}"     2>/dev/null || true
  lsof -ti :"${SERVER_PORT}" 2>/dev/null | xargs kill 2>/dev/null || true
  lsof -ti :"${RPC_PORT}"    2>/dev/null | xargs kill 2>/dev/null || true
  lsof -ti :"${GATEWAY_PORT}" 2>/dev/null | xargs kill 2>/dev/null || true
  lsof -ti :"${WEB_PORT}"    2>/dev/null | xargs kill 2>/dev/null || true
  sleep 1
  echo "All services stopped."
  exit 0
fi

echo "=== open-web-codex MVP Startup ==="
echo ""

# ── 1. Kill stale processes ──
echo "[1/6] Stopping stale services..."
pkill -f open-web-codex-server 2>/dev/null || true
pkill -f codex_monitor_daemon 2>/dev/null || true
pkill -f "codex app-server"     2>/dev/null || true
pkill -f "vite.*--port ${WEB_PORT}" 2>/dev/null || true
pkill -f "esbuild.*${WEB_PORT}"     2>/dev/null || true
lsof -ti :"${SERVER_PORT}" 2>/dev/null | xargs kill 2>/dev/null || true
lsof -ti :"${RPC_PORT}"    2>/dev/null | xargs kill 2>/dev/null || true
lsof -ti :"${GATEWAY_PORT}" 2>/dev/null | xargs kill 2>/dev/null || true
lsof -ti :"${WEB_PORT}"    2>/dev/null | xargs kill 2>/dev/null || true
sleep 1

# ── 2. Build binaries if needed ──
if [ ! -f "$DAEMON_BIN" ]; then
  echo "[2/6] Building daemon..."
  (cd "$WEB_ROOT" && cargo build --no-default-features -p codex-monitor --bin codex_monitor_daemon)
else
  echo "[2/6] Daemon binary found, skipping build."
fi

if [ ! -f "$SERVER_BIN" ]; then
  echo "[2/6] Building platform server..."
  (cd "$WEB_ROOT" && cargo build -p open-web-codex-server)
else
  echo "[2/6] Platform server binary found, skipping build."
fi

# ── 3. Start daemon ──
echo "[3/6] Starting daemon on :${RPC_PORT} (tcp) and :${GATEWAY_PORT} (web)..."
if [ ! -x "$DAEMON_BIN" ]; then
  echo "  Daemon binary is not executable: $DAEMON_BIN" >&2
  exit 1
fi
if [ ! -x "$LOCAL_CODEX_BIN" ]; then
  echo "  Local Codex binary not found: $LOCAL_CODEX_BIN" >&2
  echo "  Build it first with: (cd codex/codex-rs && cargo build --bin codex)" >&2
  exit 1
fi
PATH="$(dirname "$LOCAL_CODEX_BIN"):$PATH" nohup "$DAEMON_BIN" \
  --listen "127.0.0.1:${RPC_PORT}" \
  --web-listen "0.0.0.0:${GATEWAY_PORT}" \
  --data-dir "$DATA_DIR" \
  --insecure-no-auth > /tmp/codex-daemon.log 2>&1 &
echo "  Daemon PID: $!"
sleep 1

# ── 4. Start platform server ──
echo "[4/6] Starting platform server on :${SERVER_PORT} (mode=${CODEX_MODE})..."
if [ ! -x "$SERVER_BIN" ]; then
  echo "  Platform server binary is not executable: $SERVER_BIN" >&2
  exit 1
fi

SERVER_ARGS=(
  --bind "127.0.0.1:${SERVER_PORT}"
  --codex-mode "$CODEX_MODE"
  --migrate
)
if [ "$CODEX_MODE" = "real" ]; then
  SERVER_ARGS+=(--daemon-url "http://127.0.0.1:${GATEWAY_PORT}")
fi

CODEX_MODE="$CODEX_MODE" \
  nohup "$SERVER_BIN" "${SERVER_ARGS[@]}" > /tmp/codex-platform-server.log 2>&1 &
echo "  Platform server PID: $!"
sleep 1

# ── 5. Start Vite frontend ──
echo "[5/6] Starting Vite frontend on :${WEB_PORT}..."
cd "$WEB_ROOT"
VITE_OPEN_WEB_CODEX_API="http://127.0.0.1:${SERVER_PORT}" \
  nohup npx vite --port "${WEB_PORT}" --host 0.0.0.0 > /tmp/vite-"${WEB_PORT}".log 2>&1 & disown
echo "  Vite PID: $!"
sleep 3

# ── 6. Verify ──
echo "[6/6] Health check..."
echo ""
echo "=== Services ==="
echo "  Frontend:     http://127.0.0.1:${WEB_PORT}/web"
echo "  Platform API: http://127.0.0.1:${SERVER_PORT}"
echo "  Daemon:       http://127.0.0.1:${GATEWAY_PORT}"
echo ""

DAEMON_OK=false
SERVER_OK=false
VITE_OK=false

if curl -sf "http://127.0.0.1:${GATEWAY_PORT}/api/health" >/dev/null 2>&1; then
  echo "✓ Daemon healthy"
  DAEMON_OK=true
else
  echo "✗ Daemon not responding (see /tmp/codex-daemon.log)"
fi

if curl -sf "http://127.0.0.1:${SERVER_PORT}/api/health" >/dev/null 2>&1; then
  echo "✓ Platform server healthy"
  SERVER_OK=true
else
  echo "✗ Platform server not responding (see /tmp/codex-platform-server.log)"
  echo "  Ensure PostgreSQL is running and DATABASE_URL is set if needed."
fi

if curl -sf "http://127.0.0.1:${WEB_PORT}/" >/dev/null 2>&1; then
  echo "✓ Frontend healthy"
  VITE_OK=true
else
  echo "✗ Frontend not responding (see /tmp/vite-${WEB_PORT}.log)"
fi

echo ""
echo "Platform Web login should use API base URL: http://127.0.0.1:${SERVER_PORT}"
echo "Stop with: bash scripts/start-all.sh --stop"
