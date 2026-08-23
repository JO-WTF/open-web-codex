#!/usr/bin/env bash

set -euo pipefail
umask 077

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
bootstrap="$script_dir/copilot-sdk-bootstrap.sh"
sdk_environment_root="${OPEN_WEB_CODEX_COPILOT_SDK_ENVIRONMENT_ROOT:-$repo_root/.local/open-web-codex/sdk-environments/copilot}"
tool_registry_root="$repo_root/tools"
codex_bin="$repo_root/codex/codex-rs/target/dev-small/codex"
build_store_root="$repo_root/.local/open-web-codex/tool-builds"
platform_prepared_root="$repo_root/.local/open-web-codex/tool-environments"

usage() {
  cat <<'EOF'
Usage: ./scripts/copilot.sh COMMAND [options]

New developer entrypoint for the repository Copilot SDK:
  init PATH --name ID [--template single-agent|multi-agent]
  tool init PATH --name ID
  validate SOURCE
  check SOURCE
  sync copilots/PACKAGE [--workspace ABS] [--timeout-seconds SEC] [--build]
  dev SOURCE --workspace PATH
  test SOURCE --workspace PATH

This wrapper fixes shared Tool discovery to this repository's trusted tools/
registry. dev, test, and check use the current checkout's dev-small Codex
binary and reusable local Tool build store. sync accepts only a direct child of
the trusted copilots/ root, runs the complete check gate, and then delegates the
cold restart to run-local.sh.
EOF
}

json_requested="0"
for argument in "$@"; do
  if [[ "$argument" == "--json" ]]; then
    json_requested="1"
  fi
done

fail() {
  local code="$1" message="$2"
  if [[ "$json_requested" == "1" ]]; then
    printf '{"ok":false,"error":{"code":"%s","message":"%s"}}\n' "$code" "$message"
  else
    printf 'copilot: %s: %s\n' "$code" "$message" >&2
  fi
  exit 2
}

if (($# == 0)); then
  usage
  exit 2
fi
if [[ "$1" == "--help" || "$1" == "-h" ]]; then
  usage
  exit 0
fi

command_name="$1"
for argument in "$@"; do
  case "$argument" in
    --tool-registry-root|--tool-registry-root=*)
      fail "fixed_option" "--tool-registry-root is fixed to the trusted repository tools registry"
      ;;
  esac
done

runtime_command="0"
case "$command_name" in
  dev|test|check|sync) runtime_command="1" ;;
esac
if [[ "$runtime_command" == "1" ]]; then
  for argument in "$@"; do
    case "$argument" in
      --codex-bin|--codex-bin=*|--build-store|--build-store=*|--build-store-root|--build-store-root=*)
        fail "fixed_option" "Codex binary and Tool build store are owned by this repository wrapper"
        ;;
    esac
  done
  [[ -x "$codex_bin" ]] || {
    fail "codex_binary_missing" "the current checkout's dev-small Codex binary is missing; build the repository Runtime first"
  }
fi

sync_source=""
sync_workspace=""
sync_timeout_seconds=""
sync_build="0"
if [[ "$command_name" == "sync" ]]; then
  shift
  while (($# > 0)); do
    case "$1" in
      --workspace)
        (($# >= 2)) || fail "invalid_argument" "--workspace requires an absolute path"
        [[ -z "$sync_workspace" ]] || fail "invalid_argument" "--workspace may be supplied once"
        sync_workspace="$2"
        shift
        ;;
      --timeout-seconds)
        (($# >= 2)) || fail "invalid_argument" "--timeout-seconds requires a positive number"
        [[ -z "$sync_timeout_seconds" ]] || fail "invalid_argument" "--timeout-seconds may be supplied once"
        sync_timeout_seconds="$2"
        shift
        ;;
      --build)
        [[ "$sync_build" == "0" ]] || fail "invalid_argument" "--build may be supplied once"
        sync_build="1"
        ;;
      --json)
        ;;
      --case|--case=*)
        fail "invalid_argument" "sync always runs every declared check case"
        ;;
      --*)
        fail "invalid_argument" "unknown sync option: $1"
        ;;
      *)
        [[ -z "$sync_source" ]] || fail "invalid_argument" "sync accepts exactly one Copilot source"
        sync_source="$1"
        ;;
    esac
    shift
  done
  [[ -n "$sync_source" ]] || fail "invalid_argument" "sync requires a Copilot source"
  if [[ -n "$sync_workspace" ]]; then
    [[ "$sync_workspace" == /* ]] || fail "invalid_argument" "--workspace must be an absolute path"
    [[ -d "$sync_workspace" ]] || fail "invalid_argument" "--workspace must name an existing directory"
  fi
  if [[ -n "$sync_timeout_seconds" ]]; then
    python3 - "$sync_timeout_seconds" <<'PY' || fail "invalid_argument" "--timeout-seconds must be a positive number"
import math
import sys

try:
    value = float(sys.argv[1])
except ValueError:
    raise SystemExit(1)
raise SystemExit(0 if math.isfinite(value) and value > 0 else 1)
PY
  fi
  sync_source="$(python3 - "$repo_root/copilots" "$sync_source" <<'PY'
import os
import sys
from pathlib import Path

trusted_input = Path(sys.argv[1])
candidate_input = Path(sys.argv[2])
if not candidate_input.is_absolute():
    candidate_input = Path.cwd() / candidate_input
try:
    trusted = trusted_input.resolve(strict=True)
    candidate = candidate_input.resolve(strict=True)
except OSError:
    raise SystemExit(1)
if trusted_input.is_symlink() or candidate_input.is_symlink():
    raise SystemExit(1)
if candidate.parent != trusted or not candidate.is_dir():
    raise SystemExit(1)
relative = candidate.relative_to(trusted)
if len(relative.parts) != 1:
    raise SystemExit(1)
current = candidate
while current != trusted:
    if current.is_symlink():
        raise SystemExit(1)
    current = current.parent
print(os.fspath(candidate))
PY
  )" || fail "invalid_source" "sync source must be a non-symlink direct child of the trusted copilots root"
  [[ -f "$sync_source/copilot.toml" && ! -L "$sync_source/copilot.toml" ]] || {
    fail "invalid_source" "sync source must contain a regular copilot.toml"
  }
  if find "$sync_source" -type l -print -quit | grep -q .; then
    fail "invalid_source" "sync source must not contain symbolic links"
  fi
fi

if [[ "$command_name" == "check" ]]; then
  check_output=""
  expect_check_output="0"
  for argument in "$@"; do
    if [[ "$expect_check_output" == "1" ]]; then
      [[ -z "$check_output" ]] || {
        fail "fixed_option" "check accepts at most one explicit Tool environment root"
      }
      check_output="$argument"
      expect_check_output="0"
      continue
    fi
    case "$argument" in
      --tool-env|--tool-environment-root)
        expect_check_output="1"
        ;;
      --tool-env=*|--tool-environment-root=*)
        [[ -z "$check_output" ]] || {
          fail "fixed_option" "check accepts at most one explicit Tool environment root"
        }
        check_output="${argument#*=}"
        ;;
    esac
  done
  [[ "$expect_check_output" == "0" ]] || {
    fail "invalid_argument" "check Tool environment option requires a value"
  }
  if [[ -n "$check_output" ]] && python3 - "$check_output" "$platform_prepared_root" <<'PY'
import sys
from pathlib import Path

candidate = Path(sys.argv[1]).resolve(strict=False)
platform_root = Path(sys.argv[2]).resolve(strict=False)
raise SystemExit(0 if candidate == platform_root or candidate.is_relative_to(platform_root) else 1)
PY
  then
    fail "platform_output_forbidden" "check must not write the Platform prepared root or any of its descendants"
  fi
fi

bootstrap_log="$(mktemp "${TMPDIR:-/tmp}/open-web-codex-copilot-bootstrap.XXXXXX")"
cleanup() {
  rm -f -- "$bootstrap_log"
}
trap cleanup EXIT
if ! sdk_python="$(
  "$bootstrap" --environment-root "$sdk_environment_root" 2>"$bootstrap_log"
)"; then
  cat "$bootstrap_log" >&2
  fail "sdk_bootstrap_failed" "the isolated Copilot SDK environment could not be prepared"
fi
cat "$bootstrap_log" >&2
[[ "$sdk_python" == /* && -x "$sdk_python" ]] || {
  fail "sdk_bootstrap_failed" "the Copilot SDK bootstrap returned an invalid Python executable"
}

if [[ "$command_name" == "sync" ]]; then
  check_log="$(mktemp "${TMPDIR:-/tmp}/open-web-codex-copilot-sync-check.XXXXXX")"
  restart_log="$(mktemp "${TMPDIR:-/tmp}/open-web-codex-copilot-sync-restart.XXXXXX")"
  trap 'rm -f -- "$bootstrap_log" "$check_log" "$restart_log"' EXIT
  check_args=(
    -m copilot_sdk check "$sync_source"
    --tool-registry-root "$tool_registry_root"
    --codex-bin "$codex_bin"
    --build-store-root "$build_store_root"
    --json
  )
  if [[ -n "$sync_workspace" ]]; then
    check_args+=(--workspace "$sync_workspace")
  fi
  if [[ -n "$sync_timeout_seconds" ]]; then
    check_args+=(--timeout-seconds "$sync_timeout_seconds")
  fi
  if ! "$sdk_python" "${check_args[@]}" >"$check_log"; then
    if [[ "$json_requested" == "1" ]]; then
      python3 - "$check_log" <<'PY'
import json
import sys
from pathlib import Path

try:
    check = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
except (OSError, json.JSONDecodeError):
    check = None
print(json.dumps({
    "ok": False,
    "state": "sync_failed",
    "error": {
        "code": "check_failed",
        "message": "Copilot check failed; the running service was not restarted",
    },
    "check": check,
}, ensure_ascii=False, separators=(",", ":")))
PY
    else
      cat "$check_log" >&2
      printf 'copilot: check_failed: the running service was not restarted\n' >&2
    fi
    exit 2
  fi
  readarray_output="$({ python3 - "$check_log" <<'PY'
import json
import re
import sys
from pathlib import Path

try:
    payload = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
except (OSError, json.JSONDecodeError):
    raise SystemExit(1)
copilot = payload.get("copilot")
package_id = copilot.get("id") if isinstance(copilot, dict) else None
composition_hash = (
    copilot.get("compositionDescriptorSha256") if isinstance(copilot, dict) else None
)
if payload.get("ok") is not True:
    raise SystemExit(1)
if not isinstance(package_id, str) or re.fullmatch(r"[a-z0-9](?:[a-z0-9_-]{0,94}[a-z0-9])?", package_id) is None:
    raise SystemExit(1)
if not isinstance(composition_hash, str) or re.fullmatch(r"[0-9a-f]{64}", composition_hash) is None:
    raise SystemExit(1)
print(package_id)
print(composition_hash)
PY
  } 2>/dev/null)" || fail "invalid_check_output" "Copilot check returned invalid package identity or composition hash"
  package_id="${readarray_output%%$'\n'*}"
  composition_hash="${readarray_output#*$'\n'}"
  restart_args=(--restart)
  restart_mode="build"
  if [[ "$sync_build" == "0" ]]; then
    restart_args+=(--no-build)
    restart_mode="no-build"
  fi
  if ! "$script_dir/run-local.sh" "${restart_args[@]}" >"$restart_log" 2>&1; then
    cat "$restart_log" >&2
    fail "restart_failed" "run-local did not complete the requested cold restart"
  fi
  cat "$restart_log" >&2
  prepared_descriptor="$platform_prepared_root/$package_id/copilot-sdk/prepared-tools.v1.json"
  if ! python3 - "$prepared_descriptor" "$composition_hash" <<'PY'
import json
import re
import sys
from pathlib import Path

try:
    payload = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
except (OSError, json.JSONDecodeError):
    raise SystemExit(1)
actual = payload.get("compositionDescriptorSha256")
expected = sys.argv[2]
if not isinstance(actual, str) or re.fullmatch(r"[0-9a-f]{64}", actual) is None:
    raise SystemExit(1)
raise SystemExit(0 if actual == expected else 1)
PY
  then
    fail "composition_hash_mismatch" "the restarted service did not prepare the checked Copilot composition"
  fi
  if [[ "$json_requested" == "1" ]]; then
    python3 - "$package_id" "$composition_hash" "$restart_mode" <<'PY'
import json
import sys

print(json.dumps({
    "ok": True,
    "state": "synced",
    "copilot": {
        "id": sys.argv[1],
        "compositionDescriptorSha256": sys.argv[2],
    },
    "restart": {"mode": sys.argv[3], "healthy": True},
}, ensure_ascii=False, separators=(",", ":")))
PY
  else
    printf "Copilot '%s' synced at composition %s.\n" "$package_id" "$composition_hash"
  fi
  exit 0
fi

forwarded=("$@")
case "$command_name" in
  validate|prepare|dev|test|check)
    forwarded+=(--tool-registry-root "$tool_registry_root")
    ;;
esac
if [[ "$runtime_command" == "1" ]]; then
  forwarded+=(--codex-bin "$codex_bin" --build-store-root "$build_store_root")
fi

exec "$sdk_python" -m copilot_sdk "${forwarded[@]}"
