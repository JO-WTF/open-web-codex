#!/usr/bin/env bash

set -euo pipefail
umask 077

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
web_root="$repo_root/apps/web"

database_url="${DATABASE_URL:-}"
database_url_file=""
server_bin="${OPEN_WEB_CODEX_SERVER_BIN:-$web_root/target/debug/open-web-codex-server}"
postgres_bin="${OPEN_WEB_CODEX_POSTGRES_BIN:-/opt/homebrew/opt/postgresql@18/bin}"
confirmed="0"

usage() {
  cat <<'EOF'
Usage: ./scripts/rebuild-development-database.sh --confirm-development-only [options]

Destroys and recreates the selected development database. It preserves only the
identity closure required to restore encrypted Provider configuration:
organizations, users, memberships, profiles, profile_secrets and
profile_provider_definitions. Secret ciphertext is never printed or decrypted.

Options:
  --confirm-development-only  Required destructive-operation acknowledgement.
  --database-url URL          PostgreSQL URL (or DATABASE_URL).
  --database-url-file PATH    File containing the PostgreSQL URL.
  --server-bin PATH           Built open-web-codex server used for --migrate-only.
  --postgres-bin PATH         Directory containing PostgreSQL 18 client tools.
  -h, --help                  Show this help.
EOF
}

while (($# > 0)); do
  case "$1" in
    --confirm-development-only) confirmed="1" ;;
    --database-url)
      (($# >= 2)) || { printf 'error: --database-url requires a value\n' >&2; exit 2; }
      database_url="$2"
      shift
      ;;
    --database-url-file)
      (($# >= 2)) || { printf 'error: --database-url-file requires a value\n' >&2; exit 2; }
      database_url_file="$2"
      shift
      ;;
    --server-bin)
      (($# >= 2)) || { printf 'error: --server-bin requires a value\n' >&2; exit 2; }
      server_bin="$2"
      shift
      ;;
    --postgres-bin)
      (($# >= 2)) || { printf 'error: --postgres-bin requires a value\n' >&2; exit 2; }
      postgres_bin="$2"
      shift
      ;;
    -h|--help) usage; exit 0 ;;
    *) printf 'error: unknown option %s\n' "$1" >&2; usage >&2; exit 2 ;;
  esac
  shift
done

if [[ "$confirmed" != "1" ]]; then
  printf 'error: --confirm-development-only is required\n' >&2
  exit 2
fi

if [[ -n "$database_url_file" ]]; then
  [[ -f "$database_url_file" ]] || { printf 'error: database URL file is missing\n' >&2; exit 2; }
  database_url="$(<"$database_url_file")"
fi
[[ -n "$database_url" ]] || { printf 'error: provide --database-url, --database-url-file, or DATABASE_URL\n' >&2; exit 2; }

for executable in pg_dump pg_restore dropdb createdb psql; do
  [[ -x "$postgres_bin/$executable" ]] || {
    printf 'error: PostgreSQL client executable is missing: %s\n' "$postgres_bin/$executable" >&2
    exit 2
  }
done
[[ -x "$server_bin" ]] || {
  printf 'error: built server is missing: %s\n' "$server_bin" >&2
  printf 'build it first with ./scripts/run-local.sh, then rerun this command.\n' >&2
  exit 2
}
if ! "$server_bin" --help | grep -Fq -- '--migrate-only'; then
  printf 'error: built server does not support --migrate-only: %s\n' "$server_bin" >&2
  printf 'build the current server before running a destructive rebuild.\n' >&2
  exit 2
fi

database_name="$($postgres_bin/psql "$database_url" -Atq -v ON_ERROR_STOP=1 -c 'SELECT current_database()')"
[[ "$database_name" != "postgres" ]] || {
  printf 'error: refusing to rebuild the maintenance database\n' >&2
  exit 2
}
maintenance_url="$(printf '%s' "$database_url" | sed -E 's#(/)[^/?]+([?].*)?$#\1postgres\2#')"
if [[ "$maintenance_url" == "$database_url" ]]; then
  printf 'error: unable to derive a maintenance connection from the database URL\n' >&2
  exit 2
fi

backup_dir="$(mktemp -d "${TMPDIR:-/tmp}/open-web-codex-provider-backup.XXXXXX")"
backup_file="$backup_dir/providers.dump"
has_provider_schema="0"
trap 'rm -rf "$backup_dir"' EXIT

if [[ "$("$postgres_bin/psql" "$database_url" -Atq -v ON_ERROR_STOP=1 -c "SELECT to_regclass('public.profile_provider_definitions') IS NOT NULL AND to_regclass('public.profile_secrets') IS NOT NULL")" == "t" ]]; then
  has_provider_schema="1"
  printf 'Exporting encrypted Provider configuration closure...\n'
  "$postgres_bin/pg_dump" "$database_url" \
    --format=custom --data-only --no-owner --no-privileges \
    --table=organizations --table=users --table=memberships --table=profiles \
    --table=profile_secrets --table=profile_provider_definitions \
    --file="$backup_file"
else
  printf 'No prior Provider schema exists; starting with an empty development database.\n'
fi

printf 'Recreating development database...\n'
"$postgres_bin/dropdb" --force --maintenance-db="$maintenance_url" "$database_name"
"$postgres_bin/createdb" --maintenance-db="$maintenance_url" "$database_name"

printf 'Applying current migrations...\n'
"$server_bin" --database-url "$database_url" --migrate-only

if [[ "$has_provider_schema" == "1" ]]; then
  printf 'Restoring encrypted Provider configuration closure...\n'
  "$postgres_bin/pg_restore" --dbname="$database_url" --data-only --exit-on-error \
    --no-owner --no-privileges "$backup_file"
fi

"$script_dir/check-migration-integrity.sh"
printf 'Development database rebuild completed.\n'
