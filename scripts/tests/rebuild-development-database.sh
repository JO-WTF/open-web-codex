#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
rebuild="$repo_root/scripts/rebuild-development-database.sh"
fixture_root="$(mktemp -d "${TMPDIR:-/tmp}/open-web-codex-rebuild-development-database.XXXXXX")"
fixture_bin="$fixture_root/bin"
record_file="$fixture_root/record"

cleanup() {
  rm -rf -- "$fixture_root"
}
trap cleanup EXIT

bash -n "$rebuild"
mkdir -p "$fixture_bin"

cat >"$fixture_bin/pg_config" <<EOF
#!/usr/bin/env bash
set -euo pipefail
[[ "\${1:-}" == "--bindir" ]]
printf '%s\\n' '$fixture_bin'
EOF

cat >"$fixture_bin/psql" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf 'psql %s\n' "$*" >>"${REBUILD_TEST_RECORD:?}"
case "$*" in
  *"SHOW server_version"*) printf '17.0\n' ;;
  *"SELECT current_database()"*) printf 'rebuild_fixture\n' ;;
  *"WITH expected(name)"*) printf '%s\n' "${REBUILD_TEST_SCHEMA_STATE:-complete}" ;;
  *"WITH preserved(value)"*) printf 'configuration-digest\n' ;;
esac
if [[ "$*" != *"--command"* && "$*" != *"-c"* ]]; then
  cat >>"${REBUILD_TEST_RECORD:?}"
fi
EOF

for executable in pg_dump pg_restore dropdb createdb; do
  cat >"$fixture_bin/$executable" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s %s\n' "$(basename "$0")" "$*" >>"${REBUILD_TEST_RECORD:?}"
if [[ "${1:-}" == "--version" ]]; then
  printf '%s\n' "$(basename "$0") (PostgreSQL) 17.0"
fi
EOF
done

cat >"$fixture_bin/open-web-codex-server" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == "--help" ]]; then
  printf '%s\n' '--migrate-only'
fi
printf 'server %s\n' "$*" >>"${REBUILD_TEST_RECORD:?}"
EOF
chmod +x "$fixture_bin"/*

REBUILD_TEST_RECORD="$record_file" \
PATH="$fixture_bin:$PATH" \
DATABASE_URL='postgresql://fixture@localhost/rebuild_fixture' \
OPEN_WEB_CODEX_SERVER_BIN="$fixture_bin/open-web-codex-server" \
OPEN_WEB_CODEX_POSTGRES_BIN="$fixture_bin" \
  "$rebuild" --confirm-development-only >/dev/null

grep -F 'psql postgresql://fixture@localhost/rebuild_fixture' "$record_file" >/dev/null
grep -F 'dropdb --force --maintenance-db=postgresql://fixture@localhost/postgres rebuild_fixture' "$record_file" >/dev/null
grep -F 'createdb --maintenance-db=postgresql://fixture@localhost/postgres rebuild_fixture' "$record_file" >/dev/null
grep -F 'server --database-url postgresql://fixture@localhost/rebuild_fixture --migrate-only' "$record_file" >/dev/null
grep -F -- '--table=profile_secrets' "$record_file" >/dev/null
! grep -F -- '--table=profile_provider_definitions' "$record_file" >/dev/null
grep -F -- '--table=platform_configuration_secrets' "$record_file" >/dev/null
grep -F 'pg_restore --dbname=postgresql://fixture@localhost/rebuild_fixture --data-only --exit-on-error' "$record_file" >/dev/null
grep -F "'maps.active_credential'" "$record_file" >/dev/null
grep -F "'maps.mapbox_public_access_token'" "$record_file" >/dev/null

: >"$record_file"
if REBUILD_TEST_RECORD="$record_file" \
  REBUILD_TEST_SCHEMA_STATE=incomplete \
  PATH="$fixture_bin:$PATH" \
  DATABASE_URL='postgresql://fixture@localhost/rebuild_fixture' \
  OPEN_WEB_CODEX_SERVER_BIN="$fixture_bin/open-web-codex-server" \
  OPEN_WEB_CODEX_POSTGRES_BIN="$fixture_bin" \
  "$rebuild" --confirm-development-only >/dev/null 2>&1
then
  printf 'incomplete Provider/maps schema unexpectedly permitted a rebuild\n' >&2
  exit 1
fi
! grep -F 'dropdb ' "$record_file" >/dev/null

printf 'rebuild development database preservation contract passed\n'
