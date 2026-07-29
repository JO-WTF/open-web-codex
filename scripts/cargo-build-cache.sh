#!/usr/bin/env bash

# Shared Cargo build-cache configuration for repository-owned shell workflows.
# This file is sourced by launch, deploy, and test scripts.

cargo_build_cache_default_dir() {
  case "$(uname -s)" in
    Darwin) printf '%s\n' "$HOME/Library/Caches/open-web-codex/sccache" ;;
    *) printf '%s\n' "${XDG_CACHE_HOME:-$HOME/.cache}/open-web-codex/sccache" ;;
  esac
}

cargo_build_cache_configure() {
  local repo_root="$1"
  local mode="${OPEN_WEB_CODEX_SCCACHE_MODE:-auto}"
  local original_rustc_wrapper="${RUSTC_WRAPPER:-}"
  local sccache_bin

  case "$mode" in
    auto|required|off) ;;
    *)
      printf 'error: OPEN_WEB_CODEX_SCCACHE_MODE must be auto, required, or off\n' >&2
      return 2
      ;;
  esac

  # Incremental rustc outputs cannot be stored by sccache and were not the
  # useful part of this repository's measured build cache.
  export CARGO_INCREMENTAL=0
  export OPEN_WEB_CODEX_SCCACHE_ACTIVE=0
  export OPEN_WEB_CODEX_SCCACHE_SERVER_OWNED=0

  if [[ "$mode" == "off" ]]; then
    return 0
  fi

  sccache_bin="$(command -v sccache || true)"
  if [[ -z "$sccache_bin" ]]; then
    if [[ "$mode" == "required" ]]; then
      printf 'error: sccache is required but is not installed\n' >&2
      return 1
    fi
    printf 'warning: sccache is unavailable; continuing with bounded Cargo targets only\n' >&2
    return 0
  fi

  if [[ -n "${RUSTC_WRAPPER:-}" \
    && "$RUSTC_WRAPPER" != "$sccache_bin" \
    && "${RUSTC_WRAPPER##*/}" != "sccache" ]]
  then
    if [[ "$mode" == "required" ]]; then
      printf 'error: RUSTC_WRAPPER is already owned by %s, not sccache\n' "$RUSTC_WRAPPER" >&2
      return 1
    fi
    printf 'warning: preserving existing RUSTC_WRAPPER=%s; sccache is not active\n' \
      "$RUSTC_WRAPPER" >&2
    return 0
  fi

  export RUSTC_WRAPPER="${RUSTC_WRAPPER:-$sccache_bin}"
  export SCCACHE_CACHE_SIZE="${SCCACHE_CACHE_SIZE:-8G}"
  export SCCACHE_DIR="${SCCACHE_DIR:-$(cargo_build_cache_default_dir)}"
  export SCCACHE_BASEDIRS="${SCCACHE_BASEDIRS:-$(dirname "$repo_root")}"
  mkdir -p "$SCCACHE_DIR"

  if [[ -z "${SCCACHE_SERVER_UDS:-}" && -z "${SCCACHE_SERVER_PORT:-}" ]]; then
    case "$(uname -s)" in
      Darwin|Linux)
        export SCCACHE_SERVER_UDS="$SCCACHE_DIR/server.sock"
        export OPEN_WEB_CODEX_SCCACHE_SERVER_OWNED=1
        ;;
    esac
  fi

  export OPEN_WEB_CODEX_SCCACHE_BIN="$sccache_bin"
  if ! cargo_build_cache_ensure_server; then
    if [[ "$mode" == "required" ]]; then
      return 1
    fi
    printf 'warning: sccache server validation failed; continuing without it\n' >&2
    if [[ -n "$original_rustc_wrapper" ]]; then
      export RUSTC_WRAPPER="$original_rustc_wrapper"
    else
      unset RUSTC_WRAPPER
    fi
    export OPEN_WEB_CODEX_SCCACHE_ACTIVE=0
    return 0
  fi
  export OPEN_WEB_CODEX_SCCACHE_ACTIVE=1
}

cargo_build_cache_size_bytes() {
  local normalized number unit multiplier
  normalized="$(printf '%s' "$1" | tr '[:lower:]' '[:upper:]')"
  if [[ "$normalized" =~ ^([0-9]+)(K|M|G|T)?(I?B)?$ ]]; then
    number=$((10#${BASH_REMATCH[1]}))
    unit="${BASH_REMATCH[2]}"
  else
    return 1
  fi
  ((number > 0)) || return 1

  case "$unit" in
    "") multiplier=1 ;;
    K) multiplier=1024 ;;
    M) multiplier=$((1024 * 1024)) ;;
    G) multiplier=$((1024 * 1024 * 1024)) ;;
    T) multiplier=$((1024 * 1024 * 1024 * 1024)) ;;
  esac
  printf '%s\n' "$((number * multiplier))"
}

cargo_build_cache_max_size_from_stats() {
  sed -n 's/.*"max_cache_size":\([0-9][0-9]*\).*/\1/p'
}

cargo_build_cache_acquire_server_lock() {
  local attempt lock_file
  lock_file="$SCCACHE_DIR/server-lifecycle.lock"

  if command -v flock >/dev/null 2>&1; then
    exec 9>"$lock_file"
    if ! flock -w 10 9; then
      exec 9>&-
      printf 'error: timed out waiting for the sccache lifecycle lock\n' >&2
      return 1
    fi
    export OPEN_WEB_CODEX_SCCACHE_LOCK_FILE="$lock_file"
    export OPEN_WEB_CODEX_SCCACHE_LOCK_METHOD=flock
    return 0
  fi

  if command -v shlock >/dev/null 2>&1; then
    attempt=0
    while ((attempt < 200)); do
      if shlock -p "$$" -f "$lock_file"; then
        export OPEN_WEB_CODEX_SCCACHE_LOCK_FILE="$lock_file"
        export OPEN_WEB_CODEX_SCCACHE_LOCK_METHOD=shlock
        return 0
      fi
      attempt=$((attempt + 1))
      sleep 0.05
    done
    printf 'error: timed out waiting for the sccache lifecycle lock\n' >&2
    return 1
  fi

  printf 'error: sccache lifecycle management requires flock or shlock\n' >&2
  return 1
}

cargo_build_cache_release_server_lock() {
  local owner
  case "${OPEN_WEB_CODEX_SCCACHE_LOCK_METHOD:-}" in
    flock)
      flock -u 9
      exec 9>&-
      ;;
    shlock)
      owner="$(sed -n '1p' "$OPEN_WEB_CODEX_SCCACHE_LOCK_FILE" 2>/dev/null || true)"
      if [[ "$owner" == "$$" ]]; then
        unlink "$OPEN_WEB_CODEX_SCCACHE_LOCK_FILE"
      fi
      ;;
  esac
  unset OPEN_WEB_CODEX_SCCACHE_LOCK_FILE
  unset OPEN_WEB_CODEX_SCCACHE_LOCK_METHOD
}

cargo_build_cache_ensure_server() {
  local actual_size expected_size result stats
  if ! expected_size="$(cargo_build_cache_size_bytes "$SCCACHE_CACHE_SIZE")"; then
    printf 'error: SCCACHE_CACHE_SIZE must be a positive integer with an optional K, M, G, or T suffix\n' >&2
    return 2
  fi

  if stats="$("$OPEN_WEB_CODEX_SCCACHE_BIN" --show-stats --stats-format json 2>/dev/null)"; then
    actual_size="$(printf '%s\n' "$stats" | cargo_build_cache_max_size_from_stats)"
    if [[ "$actual_size" == "$expected_size" ]]; then
      return 0
    fi
  fi

  if [[ "$OPEN_WEB_CODEX_SCCACHE_SERVER_OWNED" != "1" ]]; then
    printf 'error: the configured sccache server does not expose the requested %s limit\n' \
      "$SCCACHE_CACHE_SIZE" >&2
    return 1
  fi

  if ! cargo_build_cache_acquire_server_lock; then
    return 1
  fi

  # Another build may have applied the requested limit while this process
  # waited. Recheck under the lifecycle lock before stopping the shared daemon.
  if stats="$("$OPEN_WEB_CODEX_SCCACHE_BIN" --show-stats --stats-format json 2>/dev/null)"; then
    actual_size="$(printf '%s\n' "$stats" | cargo_build_cache_max_size_from_stats)"
    if [[ "$actual_size" == "$expected_size" ]]; then
      cargo_build_cache_release_server_lock
      return 0
    fi
  fi

  printf 'Restarting the project sccache server to apply the %s limit\n' \
    "$SCCACHE_CACHE_SIZE" >&2
  "$OPEN_WEB_CODEX_SCCACHE_BIN" --stop-server >/dev/null 2>&1 || true
  rm -f "$SCCACHE_SERVER_UDS"
  result=0
  "$OPEN_WEB_CODEX_SCCACHE_BIN" --start-server >/dev/null || result=$?
  if ((result == 0)); then
    stats="$("$OPEN_WEB_CODEX_SCCACHE_BIN" --show-stats --stats-format json)" \
      || result=$?
  fi
  if ((result == 0)); then
    actual_size="$(printf '%s\n' "$stats" | cargo_build_cache_max_size_from_stats)"
  fi
  if ((result == 0)) && [[ "$actual_size" != "$expected_size" ]]; then
    printf 'error: sccache reported max_cache_size=%s; expected %s bytes\n' \
      "${actual_size:-unknown}" "$expected_size" >&2
    result=1
  fi
  cargo_build_cache_release_server_lock
  return "$result"
}

cargo_build_cache_describe() {
  if [[ "${OPEN_WEB_CODEX_SCCACHE_ACTIVE:-0}" == "1" ]]; then
    printf 'Cargo compiler cache: sccache (verified max %s, dir %s)\n' \
      "$SCCACHE_CACHE_SIZE" "$SCCACHE_DIR"
  else
    printf 'Cargo compiler cache: disabled\n'
  fi
  printf 'Cargo incremental compilation: disabled\n'
}
