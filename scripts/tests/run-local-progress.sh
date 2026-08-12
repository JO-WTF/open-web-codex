#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
fixture_root="$(mktemp -d "${TMPDIR:-/tmp}/open-web-codex-run-local-progress.XXXXXX")"
capture_file="$fixture_root/capture"
fake_command="$fixture_root/capture-env"

cleanup() {
  rm -rf "$fixture_root"
}
trap cleanup EXIT

cat >"$fake_command" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
{
  printf 'when=%s\n' "${CARGO_TERM_PROGRESS_WHEN-<unset>}"
  printf 'width=%s\n' "${CARGO_TERM_PROGRESS_WIDTH-<unset>}"
} >"${OPEN_WEB_CODEX_RUN_LOCAL_TEST_CAPTURE:?}"
EOF
chmod +x "$fake_command"

run_progress() {
  local stream="$1"
  OPEN_WEB_CODEX_RUN_LOCAL_TEST=progress \
    OPEN_WEB_CODEX_RUN_LOCAL_TEST_STREAM="$stream" \
    OPEN_WEB_CODEX_RUN_LOCAL_TEST_COMMAND="$fake_command" \
    OPEN_WEB_CODEX_RUN_LOCAL_TEST_CAPTURE="$capture_file" \
    "$repo_root/scripts/run-local.sh" >/dev/null
}

assert_capture() {
  local expected_when="$1" expected_width="$2"
  grep -Fx "when=$expected_when" "$capture_file" >/dev/null
  grep -Fx "width=$expected_width" "$capture_file" >/dev/null
}

(
  unset CARGO_TERM_PROGRESS_WHEN CARGO_TERM_PROGRESS_WIDTH
  run_progress 1
)
assert_capture always 120

(
  unset CARGO_TERM_PROGRESS_WHEN
  CARGO_TERM_PROGRESS_WIDTH=256 run_progress 1
)
assert_capture always 256

for invalid_width in 0 -1 invalid ''; do
  (
    unset CARGO_TERM_PROGRESS_WHEN
    CARGO_TERM_PROGRESS_WIDTH="$invalid_width" run_progress 1
  )
  assert_capture always 120
done

(
  CARGO_TERM_PROGRESS_WHEN=sentinel \
    CARGO_TERM_PROGRESS_WIDTH=sentinel \
    run_progress 0
)
assert_capture sentinel sentinel

printf 'run-local progress tests passed\n'
