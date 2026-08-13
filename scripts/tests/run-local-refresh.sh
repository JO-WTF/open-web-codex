#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
launcher="$repo_root/scripts/run-local.sh"
fixture_root="$(mktemp -d "${TMPDIR:-/tmp}/open-web-codex-run-local-refresh.XXXXXX")"
database_name="open_web_codex_refresh_test_$(printf '%s' "$$-$RANDOM" | tr -cd '0-9')"
database_user="${USER:-postgres}"
real_dropdb="$(command -v dropdb)"
real_createdb="$(command -v createdb)"

cleanup() {
  dropdb --if-exists --host 127.0.0.1 --port 5432 \
    --username "$database_user" "$database_name" >/dev/null 2>&1 || true
  rm -rf -- "$fixture_root"
}
trap cleanup EXIT

bash -n "$launcher"
grep -F -- '--refresh-local' "$launcher" >/dev/null
! grep -F -- '--codex-bin PATH' "$launcher" >/dev/null
if "$launcher" --codex-bin /tmp/codex >/dev/null 2>&1; then
  printf 'external Codex binary option unexpectedly succeeded\n' >&2
  exit 1
fi
if CODEX_BIN=/tmp/codex "$launcher" >/dev/null 2>&1; then
  printf 'external Codex binary environment unexpectedly succeeded\n' >&2
  exit 1
fi

for rejected in \
  "env OPEN_WEB_CODEX_DATA_DIR=$fixture_root $launcher --refresh-local" \
  "env DATABASE_URL=postgresql://$database_user@127.0.0.1:5432/$database_name $launcher --refresh-local" \
  "$launcher --refresh-local --database-url postgresql://$database_user@127.0.0.1:5432/$database_name" \
  "$launcher --refresh-local --database-url-file $fixture_root/database-url" \
  "$launcher --refresh-local --no-build" \
  "$launcher --refresh-local --fake" \
  "$launcher --refresh-local --background"
do
  if zsh -c "$rejected" >/dev/null 2>&1; then
    printf 'unsafe refresh invocation unexpectedly succeeded: %s\n' "$rejected" >&2
    exit 1
  fi
done

test_repo="$fixture_root/repo"
test_data="$test_repo/.local/open-web-codex"
shim_bin="$fixture_root/bin"
mkdir -p \
  "$test_repo/scripts" \
  "$shim_bin" \
  "$test_data/tool-environments/warehouse-network-copilot" \
  "$test_data/tool-environments/meeting-action-review" \
  "$test_data/profiles/default" \
  "$test_data/runner"
cp "$launcher" "$test_repo/scripts/run-local.sh"
cp "$repo_root/scripts/cargo-build-cache.sh" "$test_repo/scripts/cargo-build-cache.sh"
printf 'stale\n' >"$test_data/tool-environments/warehouse-network-copilot/stale"
printf 'stale\n' >"$test_data/tool-environments/meeting-action-review/stale"
printf 'preserve\n' >"$test_data/profiles/default/preserve"
printf 'preserve\n' >"$test_data/runner/preserve"

cat >"$shim_bin/dropdb" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
[[ "$*" == *"--host 127.0.0.1"* && "$*" == *"--port 5432"* && "$*" == *"open_web_codex"* ]]
"${RUN_LOCAL_TEST_REAL_DROPDB:?}" --if-exists --host 127.0.0.1 --port 5432 \
  --username "${RUN_LOCAL_TEST_DATABASE_USER:?}" "${RUN_LOCAL_TEST_DATABASE:?}"
EOF
cat >"$shim_bin/createdb" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
[[ "$*" == *"--host 127.0.0.1"* && "$*" == *"--port 5432"* && "$*" == *"open_web_codex"* ]]
"${RUN_LOCAL_TEST_REAL_CREATEDB:?}" --host 127.0.0.1 --port 5432 \
  --username "${RUN_LOCAL_TEST_DATABASE_USER:?}" "${RUN_LOCAL_TEST_DATABASE:?}"
EOF
chmod +x "$shim_bin/dropdb" "$shim_bin/createdb"

printf 'postgresql://%s@127.0.0.1:5432/open_web_codex\n' "$database_user" \
  >"$test_data/database-url"
if PATH="$shim_bin:$PATH" "$test_repo/scripts/run-local.sh" --refresh-local >/dev/null 2>&1
then
  printf 'default database URL file unexpectedly passed refresh authority\n' >&2
  exit 1
fi
rm -f -- "$test_data/database-url"

createdb --host 127.0.0.1 --port 5432 --username "$database_user" "$database_name"
psql --host 127.0.0.1 --port 5432 --username "$database_user" \
  --dbname "$database_name" --set ON_ERROR_STOP=1 \
  --command 'CREATE TABLE stale_development_state(id integer);' >/dev/null

OPEN_WEB_CODEX_RUN_LOCAL_TEST=refresh-local \
RUN_LOCAL_TEST_REAL_DROPDB="$real_dropdb" \
RUN_LOCAL_TEST_REAL_CREATEDB="$real_createdb" \
RUN_LOCAL_TEST_DATABASE="$database_name" \
RUN_LOCAL_TEST_DATABASE_USER="$database_user" \
PATH="$shim_bin:$PATH" \
  "$test_repo/scripts/run-local.sh" --refresh-local >/dev/null

[[ ! -e "$test_data/tool-environments/warehouse-network-copilot" ]]
[[ ! -e "$test_data/tool-environments/meeting-action-review" ]]
grep -Fx 'preserve' "$test_data/profiles/default/preserve" >/dev/null
grep -Fx 'preserve' "$test_data/runner/preserve" >/dev/null
[[ "$(psql --host 127.0.0.1 --port 5432 --username "$database_user" \
  --dbname "$database_name" --tuples-only --no-align \
  --command "SELECT to_regclass('public.stale_development_state') IS NULL;")" == "t" ]]

printf 'run-local explicit refresh contract passed\n'
