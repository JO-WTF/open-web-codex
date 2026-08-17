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
  *"SELECT current_database()"*) printf 'rebuild_fixture\n' ;;
  *"to_regclass('public.profile_provider_definitions')"*) printf 'f\n' ;;
esac
EOF

for executable in pg_dump pg_restore dropdb createdb; do
  cat >"$fixture_bin/$executable" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s %s\n' "$(basename "$0")" "$*" >>"${REBUILD_TEST_RECORD:?}"
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
  "$rebuild" --confirm-development-only >/dev/null

grep -F 'psql postgresql://fixture@localhost/rebuild_fixture' "$record_file" >/dev/null
grep -F 'dropdb --force --maintenance-db=postgresql://fixture@localhost/postgres rebuild_fixture' "$record_file" >/dev/null
grep -F 'createdb --maintenance-db=postgresql://fixture@localhost/postgres rebuild_fixture' "$record_file" >/dev/null
grep -F 'server --database-url postgresql://fixture@localhost/rebuild_fixture --migrate-only' "$record_file" >/dev/null

printf 'rebuild development database client discovery contract passed\n'
