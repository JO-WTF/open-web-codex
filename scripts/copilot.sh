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
  dev SOURCE --workspace PATH
  test SOURCE --workspace PATH

This wrapper fixes shared Tool discovery to this repository's trusted tools/
registry. dev, test, and check use the current checkout's dev-small Codex
binary and reusable local Tool build store.
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
  dev|test|check) runtime_command="1" ;;
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
