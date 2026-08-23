#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
fixture_root="$(mktemp -d "${TMPDIR:-/tmp}/open-web-codex-copilot-wrapper.XXXXXX")"
fixture_root="$(cd "$fixture_root" && pwd)"
fixture_repo="$fixture_root/repo"
capture="$fixture_root/arguments"
bootstrap_marker="$fixture_root/bootstrap-called"
check_marker="$fixture_root/check-called"
restart_capture="$fixture_root/restart-arguments"

cleanup() {
  python3 - "$fixture_root" <<'PY'
import shutil
import sys
from pathlib import Path

shutil.rmtree(Path(sys.argv[1]), ignore_errors=True)
PY
}
trap cleanup EXIT

mkdir -p \
  "$fixture_repo/scripts" \
  "$fixture_repo/tools" \
  "$fixture_repo/copilots/sample-package" \
  "$fixture_repo/copilots/outer/nested" \
  "$fixture_repo/codex/codex-rs/target/dev-small"
cp "$repo_root/scripts/copilot.sh" "$fixture_repo/scripts/copilot.sh"
printf 'schema_version = 1\nid = "sample-package"\n' \
  >"$fixture_repo/copilots/sample-package/copilot.toml"
printf 'schema_version = 1\nid = "nested-package"\n' \
  >"$fixture_repo/copilots/outer/nested/copilot.toml"

fake_python="$fixture_root/fake-python"
cat >"$fake_python" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$@" >"${COPILOT_WRAPPER_CAPTURE:?}"
if [[ " $* " == *" copilot_sdk check "* ]]; then
  if [[ -z "${COPILOT_CHECK_MARKER:-}" ]]; then
    printf '{"ok":true,"forwarded":true}\n'
    exit 0
  fi
  printf 'called\n' >"${COPILOT_CHECK_MARKER:-/dev/null}"
  case "${COPILOT_FAKE_CHECK_MODE:-success}" in
    success)
      printf '{"ok":true,"state":"check_passed","copilot":{"id":"sample-package","compositionDescriptorSha256":"%064d"},"phases":[],"tests":{"passed":2}}\n' 0
      ;;
    failure)
      printf '{"ok":false,"state":"check_failed","failedPhase":"test","error":{"code":"TestFailed"}}\n'
      exit 2
      ;;
    invalid)
      printf 'not-json\n'
      ;;
    *)
      exit 90
      ;;
  esac
  exit 0
fi
printf '{"ok":true,"forwarded":true}\n'
SH
chmod +x "$fake_python"

cat >"$fixture_repo/scripts/copilot-sdk-bootstrap.sh" <<SH
#!/usr/bin/env bash
set -euo pipefail
printf 'called\n' >'$bootstrap_marker'
printf '%s\n' '$fake_python'
SH
chmod +x "$fixture_repo/scripts/copilot-sdk-bootstrap.sh"

cat >"$fixture_repo/scripts/run-local.sh" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$@" >>"${COPILOT_RESTART_CAPTURE:?}"
if [[ "${COPILOT_FAKE_RESTART_FAIL:-0}" == "1" ]]; then
  printf 'typed stale build failure\n' >&2
  exit 1
fi
descriptor="${COPILOT_FAKE_REPOSITORY:?}/.local/open-web-codex/tool-environments/sample-package/copilot-sdk/prepared-tools.v1.json"
mkdir -p "$(dirname "$descriptor")"
printf '{"schemaVersion":1,"compositionDescriptorSha256":"%s"}\n' \
  "${COPILOT_FAKE_PREPARED_HASH:-0000000000000000000000000000000000000000000000000000000000000000}" \
  >"$descriptor"
printf 'launcher health gate passed\n'
SH
chmod +x "$fixture_repo/scripts/run-local.sh"

cat >"$fixture_repo/codex/codex-rs/target/dev-small/codex" <<'SH'
#!/usr/bin/env bash
exit 0
SH
chmod +x "$fixture_repo/codex/codex-rs/target/dev-small/codex"

wrapper_env=(
  COPILOT_WRAPPER_CAPTURE="$capture"
  COPILOT_CHECK_MARKER="$check_marker"
  COPILOT_RESTART_CAPTURE="$restart_capture"
  COPILOT_FAKE_REPOSITORY="$fixture_repo"
)

COPILOT_WRAPPER_CAPTURE="$capture" "$fixture_repo/scripts/copilot.sh" --help >/dev/null
[[ ! -e "$bootstrap_marker" ]]

stdout="$(
  COPILOT_WRAPPER_CAPTURE="$capture" \
    "$fixture_repo/scripts/copilot.sh" validate source --json
)"
python3 -c 'import json, sys; assert json.loads(sys.argv[1])["ok"]' "$stdout"
grep -Fx -- '-m' "$capture" >/dev/null
grep -Fx -- 'copilot_sdk' "$capture" >/dev/null
grep -Fx -- '--tool-registry-root' "$capture" >/dev/null
grep -Fx -- "$fixture_repo/tools" "$capture" >/dev/null

stdout="$(
  COPILOT_WRAPPER_CAPTURE="$capture" \
    "$fixture_repo/scripts/copilot.sh" check source --json
)"
python3 -c 'import json, sys; assert json.loads(sys.argv[1])["forwarded"]' "$stdout"
grep -Fx -- '--codex-bin' "$capture" >/dev/null
grep -Fx -- "$fixture_repo/codex/codex-rs/target/dev-small/codex" "$capture" >/dev/null
grep -Fx -- '--build-store-root' "$capture" >/dev/null
grep -Fx -- "$fixture_repo/.local/open-web-codex/tool-builds" "$capture" >/dev/null

set +e
override_output="$(
  COPILOT_WRAPPER_CAPTURE="$capture" \
    "$fixture_repo/scripts/copilot.sh" validate source \
      --tool-registry-root /untrusted --json 2>"$fixture_root/override.err"
)"
override_status=$?
set -e
[[ "$override_status" == "2" ]]
python3 -c 'import json, sys; assert json.loads(sys.argv[1])["error"]["code"] == "fixed_option"' \
  "$override_output"
[[ ! -s "$fixture_root/override.err" ]]

for forbidden_output in \
  "$fixture_repo/.local/open-web-codex/tool-environments" \
  "$fixture_repo/.local/open-web-codex/tool-environments/package-id"
do
  set +e
  forbidden_json="$(
    COPILOT_WRAPPER_CAPTURE="$capture" \
      "$fixture_repo/scripts/copilot.sh" check source \
        --tool-env "$forbidden_output" --json 2>"$fixture_root/forbidden.err"
  )"
  forbidden_status=$?
  set -e
  [[ "$forbidden_status" == "2" ]]
  python3 -c \
    'import json, sys; assert json.loads(sys.argv[1])["error"]["code"] == "platform_output_forbidden"' \
    "$forbidden_json"
  [[ ! -s "$fixture_root/forbidden.err" ]]
done

rm -f "$check_marker" "$restart_capture"
sync_stdout="$(
  cd "$fixture_repo"
  env "${wrapper_env[@]}" \
    ./scripts/copilot.sh sync copilots/sample-package \
      --workspace "$fixture_repo" --timeout-seconds 12 --json
)"
python3 -c '
import json, sys
payload = json.loads(sys.argv[1])
assert payload == {
    "ok": True,
    "state": "synced",
    "copilot": {
        "id": "sample-package",
        "compositionDescriptorSha256": "0" * 64,
    },
    "restart": {"mode": "no-build", "healthy": True},
}
' "$sync_stdout"
[[ -f "$check_marker" ]]
grep -Fx -- 'check' "$capture" >/dev/null
canonical_sample_package="$(cd "$fixture_repo/copilots/sample-package" && pwd -P)"
grep -Fx -- "$canonical_sample_package" "$capture" >/dev/null
grep -Fx -- '--workspace' "$capture" >/dev/null
grep -Fx -- "$fixture_repo" "$capture" >/dev/null
grep -Fx -- '--timeout-seconds' "$capture" >/dev/null
grep -Fx -- '12' "$capture" >/dev/null
grep -Fx -- '--restart' "$restart_capture" >/dev/null
grep -Fx -- '--no-build' "$restart_capture" >/dev/null
[[ "$(grep -Fc -- '--restart' "$restart_capture")" == "1" ]]

rm -f "$restart_capture"
build_stdout="$(
  cd "$fixture_repo"
  env "${wrapper_env[@]}" \
    ./scripts/copilot.sh sync copilots/sample-package --build --json
)"
python3 -c 'import json, sys; payload = json.loads(sys.argv[1]); assert payload["restart"] == {"mode": "build", "healthy": True}' \
  "$build_stdout"
grep -Fx -- '--restart' "$restart_capture" >/dev/null
if grep -Fx -- '--no-build' "$restart_capture" >/dev/null; then
  printf 'sync --build unexpectedly requested --no-build\n' >&2
  exit 1
fi

rm -f "$check_marker" "$restart_capture"
set +e
failed_check_json="$(
  cd "$fixture_repo"
  env "${wrapper_env[@]}" COPILOT_FAKE_CHECK_MODE=failure \
    ./scripts/copilot.sh sync copilots/sample-package --json 2>"$fixture_root/check-failed.err"
)"
failed_check_status=$?
set -e
[[ "$failed_check_status" == "2" ]]
python3 -c '
import json, sys
payload = json.loads(sys.argv[1])
assert payload["error"]["code"] == "check_failed"
assert payload["check"]["failedPhase"] == "test"
' "$failed_check_json"
[[ -f "$check_marker" ]]
[[ ! -e "$restart_capture" ]]

rm -f "$restart_capture"
set +e
restart_failed_json="$(
  cd "$fixture_repo"
  env "${wrapper_env[@]}" COPILOT_FAKE_RESTART_FAIL=1 \
    ./scripts/copilot.sh sync copilots/sample-package --json 2>"$fixture_root/restart-failed.err"
)"
restart_failed_status=$?
set -e
[[ "$restart_failed_status" == "2" ]]
python3 -c 'import json, sys; assert json.loads(sys.argv[1])["error"]["code"] == "restart_failed"' \
  "$restart_failed_json"
[[ "$(grep -Fc -- '--restart' "$restart_capture")" == "1" ]]
grep -Fx -- '--no-build' "$restart_capture" >/dev/null

rm -f "$restart_capture"
set +e
hash_mismatch_json="$(
  cd "$fixture_repo"
  env "${wrapper_env[@]}" \
    COPILOT_FAKE_PREPARED_HASH=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa \
    ./scripts/copilot.sh sync copilots/sample-package --json 2>"$fixture_root/hash-mismatch.err"
)"
hash_mismatch_status=$?
set -e
[[ "$hash_mismatch_status" == "2" ]]
python3 -c 'import json, sys; assert json.loads(sys.argv[1])["error"]["code"] == "composition_hash_mismatch"' \
  "$hash_mismatch_json"
[[ "$(grep -Fc -- '--restart' "$restart_capture")" == "1" ]]

mkdir -p "$fixture_root/external-package"
printf 'schema_version = 1\nid = "external-package"\n' \
  >"$fixture_root/external-package/copilot.toml"
ln -s "$fixture_repo/copilots/sample-package" "$fixture_repo/copilots/symlink-package"
ln -s "$fixture_root/external-package/copilot.toml" \
  "$fixture_repo/copilots/sample-package/source-link.toml"
for invalid_source in \
  "$fixture_root/external-package" \
  "$fixture_repo/copilots/outer/nested" \
  "$fixture_repo/copilots/symlink-package" \
  "$fixture_repo/copilots/sample-package"
do
  rm -f "$check_marker" "$restart_capture"
  set +e
  invalid_source_json="$(
    cd "$fixture_repo"
    env "${wrapper_env[@]}" \
      ./scripts/copilot.sh sync "$invalid_source" --json 2>"$fixture_root/invalid-source.err"
  )"
  invalid_source_status=$?
  set -e
  [[ "$invalid_source_status" == "2" ]]
  python3 -c 'import json, sys; assert json.loads(sys.argv[1])["error"]["code"] == "invalid_source"' \
    "$invalid_source_json"
  [[ ! -e "$check_marker" ]]
  [[ ! -e "$restart_capture" ]]
done
rm -f "$fixture_repo/copilots/sample-package/source-link.toml"

set +e
case_json="$(
  cd "$fixture_repo"
  env "${wrapper_env[@]}" \
    ./scripts/copilot.sh sync copilots/sample-package --case one --json 2>"$fixture_root/case.err"
)"
case_status=$?
set -e
[[ "$case_status" == "2" ]]
python3 -c 'import json, sys; assert json.loads(sys.argv[1])["error"]["code"] == "invalid_argument"' \
  "$case_json"

python3 - "$fixture_repo/codex/codex-rs/target/dev-small/codex" <<'PY'
import sys
from pathlib import Path

Path(sys.argv[1]).unlink()
PY
set +e
missing_output="$(
  COPILOT_WRAPPER_CAPTURE="$capture" \
    "$fixture_repo/scripts/copilot.sh" check source --json 2>"$fixture_root/missing.err"
)"
missing_status=$?
set -e
[[ "$missing_status" == "2" ]]
python3 -c 'import json, sys; assert json.loads(sys.argv[1])["error"]["code"] == "codex_binary_missing"' \
  "$missing_output"
[[ ! -s "$fixture_root/missing.err" ]]

printf 'Copilot wrapper contracts passed\n'
