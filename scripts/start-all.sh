#!/bin/bash
# start-all.sh — One-click startup for open-web-codex MVP
set -e

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DAEMON_BIN="$ROOT/apps/web/target/debug/codex_monitor_daemon"
DATA_DIR="$HOME/Library/Application Support/com.dimillian.codexmonitor"

echo "=== open-web-codex MVP Startup ==="
echo ""

# ── Kill any stale processes ──
echo "[1/4] Stopping stale services..."
pkill -f codex_monitor_daemon 2>/dev/null || true
pkill -f "codex app-server" 2>/dev/null || true
sleep 1

# ── Build daemon if needed ──
if [ ! -f "$DAEMON_BIN" ]; then
  echo "[2/4] Building daemon..."
  cargo build --manifest-path "$ROOT/apps/web/src-tauri/Cargo.toml" --bin codex_monitor_daemon
else
  echo "[2/4] Daemon binary found, skipping build."
fi

# ── Start daemon ──
echo "[3/4] Starting daemon on :4732 (tcp) and :4733 (web)..."
nohup "$DAEMON_BIN" \
  --listen 127.0.0.1:4732 \
  --web-listen 0.0.0.0:4733 \
  --data-dir "$DATA_DIR" \
  --insecure-no-auth > /tmp/codex-daemon.log 2>&1 &
echo "  Daemon PID: $!"
sleep 1

# ── Start Vite frontend ──
echo "[4/4] Starting Vite frontend on :1420..."
cd "$ROOT/apps/web"
npx vite --port 1420 --host 0.0.0.0 &
VITE_PID=$!
sleep 3

# ── Verify ──
echo ""
echo "=== Services ==="
echo "  Frontend:  http://127.0.0.1:1420/web"
echo "  Daemon:    http://127.0.0.1:4733 (api)"
echo ""

# Quick health check
HEALTH=$(curl -s http://127.0.0.1:4733/api/health 2>/dev/null || echo '{"ok":false}')
if echo "$HEALTH" | grep -q '"ok":true'; then
  echo "✓ Daemon is healthy"
else
  echo "✗ Daemon health check failed"
fi

echo ""
echo "Press Ctrl+C to stop all services."
echo ""

# Wait for Ctrl+C
trap "echo 'Shutting down...'; kill $VITE_PID 2>/dev/null; pkill -f codex_monitor_daemon 2>/dev/null; exit 0" INT TERM
wait
