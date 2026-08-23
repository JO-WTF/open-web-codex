#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
bootstrap="$repo_root/scripts/copilot-sdk-bootstrap.sh"
fixture_root="$(mktemp -d "${TMPDIR:-/tmp}/open-web-codex-sdk-bootstrap.XXXXXX")"
fixture_root="$(cd "$fixture_root" && pwd)"

cleanup() {
  python3 - "$fixture_root" <<'PY'
import shutil
import sys
from pathlib import Path

shutil.rmtree(Path(sys.argv[1]), ignore_errors=True)
PY
}
trap cleanup EXIT

bash -n "$bootstrap"

help_environment="$fixture_root/help-environment"
OPEN_WEB_CODEX_COPILOT_SDK_ENVIRONMENT_ROOT="$help_environment" \
  "$bootstrap" --help >/dev/null
[[ ! -e "$help_environment" ]]

environment_root="$fixture_root/environment"
first_python="$("$bootstrap" --environment-root "$environment_root" 2>"$fixture_root/first.log")"
[[ "$first_python" == "$environment_root/bin/python" ]]
[[ -x "$first_python" ]]
[[ -s "$environment_root/source-fingerprint" ]]
"$first_python" - <<'PY'
import importlib.metadata

assert importlib.metadata.version("open-web-codex-provider-sdk") == "0.1.0"
assert importlib.metadata.version("open-web-codex-copilot-sdk") == "0.1.0"
PY

marker_before="$(<"$environment_root/source-fingerprint")"
second_python="$("$bootstrap" --environment-root "$environment_root" 2>"$fixture_root/reuse.log")"
[[ "$second_python" == "$first_python" ]]
[[ ! -s "$fixture_root/reuse.log" ]]
[[ "$(<"$environment_root/source-fingerprint")" == "$marker_before" ]]

failure_environment="$fixture_root/failure-environment"
mkdir -p "$fixture_root/empty-index"
set +e
PIP_NO_INDEX=1 PIP_FIND_LINKS="$fixture_root/empty-index" \
  "$bootstrap" --environment-root "$failure_environment" \
  >"$fixture_root/failure.out" 2>"$fixture_root/failure.err"
failure_status=$?
set -e
((failure_status != 0))
[[ ! -e "$failure_environment/source-fingerprint" ]]

symlink_target="$fixture_root/symlink-target"
mkdir -p "$symlink_target"
printf 'keep\n' >"$symlink_target/sentinel"
ln -s "$symlink_target" "$fixture_root/symlink-environment"
if "$bootstrap" --environment-root "$fixture_root/symlink-environment" >/dev/null 2>&1; then
  printf 'symlinked environment unexpectedly succeeded\n' >&2
  exit 1
fi
grep -Fx 'keep' "$symlink_target/sentinel" >/dev/null
[[ ! -e "$symlink_target/source-fingerprint" ]]

python3 - "$environment_root/source-fingerprint" <<'PY'
import sys
from pathlib import Path

Path(sys.argv[1]).unlink()
PY
"$bootstrap" --environment-root "$environment_root" \
  >"$fixture_root/concurrent-one.out" 2>"$fixture_root/concurrent-one.err" &
first_pid=$!
"$bootstrap" --environment-root "$environment_root" \
  >"$fixture_root/concurrent-two.out" 2>"$fixture_root/concurrent-two.err" &
second_pid=$!
wait "$first_pid"
wait "$second_pid"
[[ "$(<"$fixture_root/concurrent-one.out")" == "$first_python" ]]
[[ "$(<"$fixture_root/concurrent-two.out")" == "$first_python" ]]
[[ -s "$environment_root/source-fingerprint" ]]
[[ ! -e "${environment_root}.bootstrap-lock" ]]

printf 'Copilot SDK bootstrap contracts passed\n'
