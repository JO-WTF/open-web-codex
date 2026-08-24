#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
fixture_root="$(mktemp -d "${TMPDIR:-/tmp}/open-web-codex-test-codex.XXXXXX")"
fixture_root="$(cd "$fixture_root" && pwd)"
fixture_repo="$fixture_root/repo"
capture_dir="$fixture_root/capture"

cleanup() {
  rm -rf -- "$fixture_root"
}
trap cleanup EXIT

mkdir -p "$fixture_repo/scripts" "$fixture_repo/codex/codex-rs" "$capture_dir"
cp "$repo_root/scripts/test-codex.sh" "$fixture_repo/scripts/test-codex.sh"
cp "$repo_root/scripts/cargo-build-cache.sh" "$fixture_repo/scripts/cargo-build-cache.sh"
touch "$fixture_repo/scripts/run-codex-cargo-with-v8.py"
chmod +x "$fixture_repo/scripts/test-codex.sh"

fake_python="$fixture_root/fake-python"
cat >"$fake_python" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

sequence_file="$TEST_CODEX_CAPTURE_DIR/sequence"
environment_file="$TEST_CODEX_CAPTURE_DIR/environment"
printf '%s\n' "$*" >>"$sequence_file"

if [[ " $* " == *" cargo build "* ]]; then
  if [[ -n "${CARGO_TARGET_DIR:-}" && "$CARGO_TARGET_DIR" == /* ]]; then
    target_dir="$CARGO_TARGET_DIR"
  else
    target_dir="$TEST_CODEX_FIXTURE_REPO/codex/codex-rs/${CARGO_TARGET_DIR:-target}"
  fi
  helper_dir="$target_dir/ci-test"
  mkdir -p "$helper_dir"
  for helper in codex codex-code-mode-host test_stdio_server; do
    if [[ "$helper" == "${TEST_CODEX_OMIT_HELPER:-}" ]]; then
      continue
    fi
    printf '#!/usr/bin/env bash\nexit 0\n' >"$helper_dir/$helper"
    chmod +x "$helper_dir/$helper"
  done
  exit 0
fi

if [[ " $* " == *" just test "* ]]; then
  env -0 >"$environment_file"
  exit 0
fi

printf 'unexpected fake Python invocation: %s\n' "$*" >&2
exit 1
SH
chmod +x "$fake_python"

run_test_codex() {
  local target_dir="$1"
  shift
  rm -f "$capture_dir/sequence" "$capture_dir/environment"
  env \
    OPEN_WEB_CODEX_SCCACHE_MODE=off \
    TEST_CODEX_CAPTURE_DIR="$capture_dir" \
    TEST_CODEX_FIXTURE_REPO="$fixture_repo" \
    CARGO_TARGET_DIR="$target_dir" \
    CARGO_BIN_EXE_codex=/stale/codex \
    CARGO_BIN_EXE_codex_code_mode_host=/stale/codex-code-mode-host \
    CARGO_BIN_EXE_test_stdio_server=/stale/test_stdio_server \
    CARGO_BIN_EXE_codex-code-mode-host=/stale/dashed-code-mode-host \
    CODEX_BIN=/stale/codex \
    PYTHON="$fake_python" \
    "$fixture_repo/scripts/test-codex.sh" "$@"
}

assert_helper_environment() {
  local helper_dir="$1"
  python3 - "$capture_dir/environment" "$helper_dir" <<'PY'
from pathlib import Path
import sys

values = {}
for item in Path(sys.argv[1]).read_bytes().split(b"\0"):
    if item:
        key, value = item.split(b"=", 1)
        values[key.decode()] = value.decode()

helper_dir = sys.argv[2]

def assert_value(key: str, expected: str) -> None:
    actual = values.get(key)
    assert actual == expected, f"{key} was not the current-checkout helper"

assert_value("CARGO_BIN_EXE_codex", f"{helper_dir}/codex")
assert_value("CARGO_BIN_EXE_codex_code_mode_host", f"{helper_dir}/codex-code-mode-host")
assert_value("CARGO_BIN_EXE_test_stdio_server", f"{helper_dir}/test_stdio_server")
assert_value("CODEX_BIN", f"{helper_dir}/codex")
assert "CARGO_BIN_EXE_codex-code-mode-host" not in values, "dashed stale helper remained"
assert values["PATH"].split(":", 1)[0] == helper_dir, "current helper directory did not lead PATH"
PY
}

bash -n "$fixture_repo/scripts/test-codex.sh"

run_test_codex custom-target -p codex-core code_mode
helper_dir="$fixture_repo/codex/codex-rs/custom-target/ci-test"
assert_helper_environment "$helper_dir"
grep -F -- 'cargo build --locked --profile ci-test -p codex-cli --bin codex -p codex-code-mode-host --bin codex-code-mode-host -p codex-rmcp-client --bin test_stdio_server' \
  "$capture_dir/sequence" >/dev/null
grep -F -- 'just test --cargo-profile ci-test -p codex-core code_mode' "$capture_dir/sequence" >/dev/null

run_test_codex custom-target -p codex-rmcp-client stdio_message_limits
python3 - "$capture_dir/environment" "$helper_dir" <<'PY'
from pathlib import Path
import sys

values = {}
for item in Path(sys.argv[1]).read_bytes().split(b"\0"):
    if item:
        key, value = item.split(b"=", 1)
        values[key.decode()] = value.decode()

helper_dir = sys.argv[2]
assert values["CARGO_BIN_EXE_test_stdio_server"] == f"{helper_dir}/test_stdio_server"
assert "CARGO_BIN_EXE_codex" not in values
assert "CARGO_BIN_EXE_codex_code_mode_host" not in values
assert "CARGO_BIN_EXE_codex-code-mode-host" not in values
assert "CODEX_BIN" not in values
PY
grep -F -- 'cargo build --locked --profile ci-test -p codex-rmcp-client --bin test_stdio_server' \
  "$capture_dir/sequence" >/dev/null
if grep -F -- '-p codex-cli --bin codex' "$capture_dir/sequence" >/dev/null; then
  printf 'rmcp-client test unexpectedly built the Codex CLI helper\n' >&2
  exit 1
fi

run_test_codex custom-target -p codex-app-server-protocol schema_fixtures
[[ ! -e "$capture_dir/environment" ]]
[[ "$(wc -l <"$capture_dir/sequence")" == "1" ]]
grep -F -- 'just test --cargo-profile ci-test -p codex-app-server-protocol schema_fixtures' \
  "$capture_dir/sequence" >/dev/null

rm -f "$capture_dir/sequence" "$capture_dir/environment"
set +e
TEST_CODEX_OMIT_HELPER=codex-code-mode-host \
  OPEN_WEB_CODEX_SCCACHE_MODE=off \
  TEST_CODEX_CAPTURE_DIR="$capture_dir" \
  TEST_CODEX_FIXTURE_REPO="$fixture_repo" \
  CARGO_TARGET_DIR=missing-helper-target \
  PYTHON="$fake_python" \
  "$fixture_repo/scripts/test-codex.sh" -p codex-app-server code_mode \
  >"$capture_dir/missing.out" 2>"$capture_dir/missing.err"
missing_status=$?
set -e
((missing_status != 0))
grep -F -- 'current checkout test helper is missing or not executable' "$capture_dir/missing.err" >/dev/null
[[ "$(wc -l <"$capture_dir/sequence")" == "1" ]]

printf 'test-codex helper contract passed\n'
