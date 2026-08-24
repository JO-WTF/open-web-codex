#!/usr/bin/env bash

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
runtime_root="$repo_root/codex/codex-rs"

# shellcheck source=scripts/cargo-build-cache.sh
source "$script_dir/cargo-build-cache.sh"
cargo_build_cache_configure "$repo_root"
cargo_build_cache_describe

python_cmd="${PYTHON:-python3}"
cargo_adapter=("$python_cmd" "$repo_root/scripts/run-codex-cargo-with-v8.py" cargo)
checkout_test_helpers=()

add_checkout_test_helper() {
  local helper="$1" existing

  for existing in "${checkout_test_helpers[@]-}"; do
    [[ "$existing" == "$helper" ]] && return
  done
  checkout_test_helpers+=("$helper")
}

add_checkout_test_helpers_for_package() {
  case "$1" in
    codex-app-server)
      add_checkout_test_helper codex
      add_checkout_test_helper codex-code-mode-host
      ;;
    codex-cli|codex-tui)
      add_checkout_test_helper codex
      ;;
    codex-code-mode-host)
      add_checkout_test_helper codex-code-mode-host
      ;;
    codex-core)
      # Core's suite resolves all three helpers across its CLI, Code Mode, and
      # stdio MCP integration targets.
      add_checkout_test_helper codex
      add_checkout_test_helper codex-code-mode-host
      add_checkout_test_helper test_stdio_server
      ;;
    codex-rmcp-client)
      add_checkout_test_helper test_stdio_server
      ;;
  esac
}

select_checkout_test_helpers() {
  local argument package awaiting_package=0 saw_package=0

  for argument in "$@"; do
    if [[ "$awaiting_package" == "1" ]]; then
      package="$argument"
      awaiting_package=0
    else
      case "$argument" in
        --)
          break
          ;;
        --workspace)
          checkout_test_helpers=()
          saw_package=0
          break
          ;;
        -p|--package)
          saw_package=1
          awaiting_package=1
          continue
          ;;
        --package=*)
          saw_package=1
          package="${argument#--package=}"
          ;;
        -p?*)
          saw_package=1
          package="${argument#-p}"
          ;;
        *) continue ;;
      esac
    fi

    add_checkout_test_helpers_for_package "$package"
    package=""
  done

  # A test invocation without an explicit package may include any workspace
  # integration test, so it needs the complete current-checkout helper set.
  if [[ "$saw_package" == "0" ]]; then
    add_checkout_test_helper codex
    add_checkout_test_helper codex-code-mode-host
    add_checkout_test_helper test_stdio_server
  fi
}

select_checkout_test_helpers "$@"
if ((${#checkout_test_helpers[@]} == 0)); then
  (cd "$repo_root/codex" && "$python_cmd" "$repo_root/scripts/run-codex-cargo-with-v8.py" just test --cargo-profile ci-test "$@")
  exit $?
fi

if [[ -n "${CARGO_TARGET_DIR:-}" ]]; then
  if [[ "$CARGO_TARGET_DIR" == /* ]]; then
    runtime_target_dir="$CARGO_TARGET_DIR"
  else
    runtime_target_dir="$runtime_root/$CARGO_TARGET_DIR"
  fi
else
  runtime_target_dir="$runtime_root/target"
fi

cargo_profile="ci-test"
codex_bin="$runtime_target_dir/$cargo_profile/codex"
code_mode_host_bin="$runtime_target_dir/$cargo_profile/codex-code-mode-host"
stdio_server_bin="$runtime_target_dir/$cargo_profile/test_stdio_server"
helper_build_args=(build --locked --profile "$cargo_profile")

for helper in "${checkout_test_helpers[@]}"; do
  case "$helper" in
    codex)
      helper_build_args+=(-p codex-cli --bin codex)
      ;;
    codex-code-mode-host)
      helper_build_args+=(-p codex-code-mode-host --bin codex-code-mode-host)
      ;;
    test_stdio_server)
      helper_build_args+=(-p codex-rmcp-client --bin test_stdio_server)
      ;;
  esac
done

(cd "$runtime_root" && "${cargo_adapter[@]}" "${helper_build_args[@]}")

for helper in "${checkout_test_helpers[@]}"; do
  case "$helper" in
    codex) helper_bin="$codex_bin" ;;
    codex-code-mode-host) helper_bin="$code_mode_host_bin" ;;
    test_stdio_server) helper_bin="$stdio_server_bin" ;;
  esac
  if [[ ! -f "$helper_bin" || ! -x "$helper_bin" ]]; then
    printf 'error: current checkout test helper is missing or not executable: %s\n' "$helper_bin" >&2
    exit 1
  fi
done

# cargo_bin() first consults these values. Keep its test-only fallback from
# selecting a sibling of a nextest binary, a previous checkout, or an installed
# desktop application. The underscore spelling is the portable shell form for
# Cargo's dashed code-mode-host binary target.
test_environment=(
  env
  -u CARGO_BIN_EXE_codex
  -u CARGO_BIN_EXE_codex-code-mode-host
  -u CARGO_BIN_EXE_codex_code_mode_host
  -u CARGO_BIN_EXE_test_stdio_server
  -u CODEX_BIN
  "PATH=$runtime_target_dir/$cargo_profile:$PATH"
)
for helper in "${checkout_test_helpers[@]}"; do
  case "$helper" in
    codex)
      test_environment+=("CARGO_BIN_EXE_codex=$codex_bin" "CODEX_BIN=$codex_bin")
      ;;
    codex-code-mode-host)
      test_environment+=("CARGO_BIN_EXE_codex_code_mode_host=$code_mode_host_bin")
      ;;
    test_stdio_server)
      test_environment+=("CARGO_BIN_EXE_test_stdio_server=$stdio_server_bin")
      ;;
  esac
done
(
  cd "$repo_root/codex"
  "${test_environment[@]}" \
    "$python_cmd" "$repo_root/scripts/run-codex-cargo-with-v8.py" \
      just test --cargo-profile "$cargo_profile" "$@"
)
