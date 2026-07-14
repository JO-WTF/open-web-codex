#!/bin/bash
# start-all.sh — One-click startup for open-web-codex MVP
# Usage:
#   bash scripts/start-all.sh          # start all services
#   bash scripts/start-all.sh --stop   # stop all services

set -euo pipefail

STOP_MODE=false
if [ "${1:-}" = "--stop" ] || [ "${1:-}" = "stop" ]; then
  STOP_MODE=true
fi

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DAEMON_BIN="$ROOT/apps/web/target/debug/codex_monitor_daemon"
LOCAL_CODEX_BIN="$ROOT/codex/codex-rs/target/debug/codex"
DATA_DIR="$HOME/Library/Application Support/com.dimillian.codexmonitor"

# ── Stop mode ──
if $STOP_MODE; then
  echo "=== Stopping all services ==="
  pkill -f "codex_monitor_daemon" 2>/dev/null && echo "  ✓ daemon stopped" || echo "  - daemon not running"
  pkill -f "codex app-server"     2>/dev/null && echo "  ✓ codex app-server stopped" || echo "  - app-server not running"
  pkill -f "vite.*--port 1421"    2>/dev/null && echo "  ✓ vite stopped" || echo "  - vite not running"
  pkill -f "esbuild.*1421"        2>/dev/null || true
  lsof -ti :4732 2>/dev/null | xargs kill 2>/dev/null || true
  lsof -ti :4733 2>/dev/null | xargs kill 2>/dev/null || true
  lsof -ti :1421 2>/dev/null | xargs kill 2>/dev/null || true
  sleep 1
  echo "All services stopped."
  exit 0
fi

echo "=== open-web-codex MVP Startup ==="
echo ""

# ── 1. Kill stale processes ──
echo "[1/5] Stopping stale services..."
pkill -f codex_monitor_daemon 2>/dev/null || true
pkill -f "codex app-server"     2>/dev/null || true
pkill -f "vite.*--port 1421"    2>/dev/null || true
pkill -f "esbuild.*1421"        2>/dev/null || true
lsof -ti :4732 2>/dev/null | xargs kill 2>/dev/null || true
lsof -ti :4733 2>/dev/null | xargs kill 2>/dev/null || true
lsof -ti :1421 2>/dev/null | xargs kill 2>/dev/null || true
sleep 1

# ── 2. Build daemon if needed ──
if [ ! -f "$DAEMON_BIN" ]; then
  echo "[2/5] Building daemon..."
  (cd "$ROOT" && cargo build --manifest-path apps/web/src-tauri/Cargo.toml --bin codex_monitor_daemon)
else
  echo "[2/5] Daemon binary found, skipping build."
fi

# ── 3. Start daemon ──
echo "[3/5] Starting daemon on :4732 (tcp) and :4733 (web)..."
if [ ! -x "$LOCAL_CODEX_BIN" ]; then
  echo "  Local Codex binary not found: $LOCAL_CODEX_BIN" >&2
  echo "  Build it first with: (cd codex/codex-rs && cargo build --bin codex)" >&2
  exit 1
fi
PATH="$(dirname "$LOCAL_CODEX_BIN"):$PATH" nohup "$DAEMON_BIN" \
  --listen 127.0.0.1:4732 \
  --web-listen 0.0.0.0:4733 \
  --data-dir "$DATA_DIR" \
  --insecure-no-auth > /tmp/codex-daemon.log 2>&1 &
echo "  Daemon PID: $!"
sleep 1

# ── 4. Start Vite frontend ──
echo "[4/5] Starting Vite frontend on :1421..."
cd "$ROOT/apps/web"
nohup npx vite --port 1421 --host 0.0.0.0 > /tmp/vite-1421.log 2>&1 & disown
echo "  Vite PID: $!"
sleep 3

# ── 5. Verify ──
echo "[5/5] Health check..."
echo ""
echo "=== Services ==="
echo "  Frontend:  http://127.0.0.1:1421/web"
echo "  Daemon:    http://127.0.0.1:4733"
echo ""

DAEMON_OK=false
VITE_OK=false

if curl -sf http://127.0.0.1:4733/api/health >/dev/null 2>&1; then
  echo "✓ Daemon healthy"
  DAEMON_OK=true
else
  echo "✗ Daemon not responding"
fi

if curl -sf http://127.0.0.1:1421/ >/dev/null 2>&1; then
  echo "✓ Frontend healthy"
  VITE_OK=true
else
  echo "✗ Frontend not responding"
fi

echo ""
echo "Stop with: bash scripts/start-all.sh --stop"
