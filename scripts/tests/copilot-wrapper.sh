#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
fixture_root="$(mktemp -d "${TMPDIR:-/tmp}/open-web-codex-copilot-wrapper.XXXXXX")"
fixture_root="$(cd "$fixture_root" && pwd)"
fixture_repo="$fixture_root/repo"
capture="$fixture_root/arguments"
bootstrap_marker="$fixture_root/bootstrap-called"

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
  "$fixture_repo/codex/codex-rs/target/dev-small"
cp "$repo_root/scripts/copilot.sh" "$fixture_repo/scripts/copilot.sh"

fake_python="$fixture_root/fake-python"
cat >"$fake_python" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$@" >"${COPILOT_WRAPPER_CAPTURE:?}"
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

cat >"$fixture_repo/codex/codex-rs/target/dev-small/codex" <<'SH'
#!/usr/bin/env bash
exit 0
SH
chmod +x "$fixture_repo/codex/codex-rs/target/dev-small/codex"

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
