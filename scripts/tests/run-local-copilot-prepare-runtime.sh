#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
launcher="$repo_root/scripts/run-local.sh"
test_root="$(mktemp -d /tmp/run-local-copilot-prepare-runtime.XXXXXX)"
trap 'python3 - "$test_root" <<'PY'
import shutil
import sys
from pathlib import Path
shutil.rmtree(Path(sys.argv[1]), ignore_errors=True)
PY' EXIT

function_text="$(python3 - "$launcher" <<'PY'
from pathlib import Path
import sys

text = Path(sys.argv[1]).read_text(encoding="utf-8")
start = text.index("prepare_copilot_environments() {")
end = text.index("\nrebuild_development_database() {", start)
print(text[start:end])
PY
)"

run_case() {
  local case_name="$1" expected_status="$2" expected_gc="$3" fail_package="${4:-}"
  local case_root="$test_root/$case_name" package_root capture fake_python
  local result=0
  mkdir -p "$case_root/copilots" "$case_root/prepared" "$case_root/tool-builds"
  capture="$case_root/gc-args"
  fake_python="$case_root/fake-python"
  cat >"$fake_python" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == "-c" ]]; then
  shift
  exec python3 -c "$@"
fi
if [[ "${1:-}" == "-m" && "${2:-}" == "copilot_sdk" && "${3:-}" == "gc-builds" ]]; then
  printf '%s\n' "$@" >"$GC_CAPTURE"
  exit "${GC_EXIT:-0}"
fi
exit 99
SH
  chmod +x "$fake_python"

  case "$case_name" in
    empty)
      ;;
    duplicate)
      for package_root in "$case_root/copilots/one" "$case_root/copilots/two"; do
        mkdir -p "$package_root"
        printf 'schema_version = 1\nid = "same"\n' >"$package_root/copilot.toml"
      done
      ;;
    *)
      for package_root in "$case_root/copilots/one" "$case_root/copilots/two" "$case_root/copilots/three"; do
        mkdir -p "$package_root"
        printf 'schema_version = 1\nid = "%s"\n' "$(basename "$package_root")" >"$package_root/copilot.toml"
      done
      ;;
  esac

  export GC_CAPTURE="$capture" GC_EXIT=0
  eval "$function_text"
  copilot_sdk_python="$fake_python"
  copilot_prepared_root="$case_root/prepared"
  copilot_build_store_root="$case_root/tool-builds"
  copilot_tool_registry_root="$case_root/tools"
  copilots_root="$case_root/copilots"
  python_cmd=python3
  prepare_copilot_environment() {
    local source="$1" output="$2" id
    id="$(basename "$source")"
    if [[ "$id" == "$fail_package" ]]; then
      return 1
    fi
    mkdir -p "$output/copilot-sdk"
    printf '{}\n' >"$output/copilot-sdk/prepared-tools.v1.json"
  }
  error() { return 1; }

  set +e
  prepare_copilot_environments
  result=$?
  set -e
  [[ "$result" == "$expected_status" ]] || {
    printf 'case %s: expected status %s, got %s\n' "$case_name" "$expected_status" "$result" >&2
    return 1
  }
  if [[ "$expected_gc" == "yes" ]]; then
    [[ -s "$capture" ]] || { printf 'case %s: GC was not called\n' "$case_name" >&2; return 1; }
    grep -F -- '--active-package-id' "$capture" >/dev/null
    [[ "$(grep -c -- '--active-package-id' "$capture")" == "3" ]] || return 1
  else
    [[ ! -e "$capture" ]] || { printf 'case %s: GC must be skipped\n' "$case_name" >&2; return 1; }
  fi
}

run_case empty 1 no
run_case success 0 yes
run_case duplicate 1 no
run_case failure 1 no two
printf 'run-local Copilot prepare control-flow tests passed\n'
