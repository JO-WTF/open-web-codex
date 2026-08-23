#!/usr/bin/env bash

set -euo pipefail
umask 077

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
web_root="$repo_root/apps/web"

database_url="${DATABASE_URL:-}"
database_url_file=""
server_bin="${OPEN_WEB_CODEX_SERVER_BIN:-$web_root/target/debug/open-web-codex-server}"
postgres_bin="${OPEN_WEB_CODEX_POSTGRES_BIN:-}"
confirmed="0"

usage() {
  cat <<'EOF'
Usage: ./scripts/rebuild-development-database.sh --confirm-development-only [options]

Destroys and recreates the selected development database. It preserves only the
identity closure required to restore Provider credentials, the default model
selection, and the active maps credential: organizations, users, memberships,
profiles, profile_secrets, platform configuration, and platform configuration
secrets. Secret ciphertext is never printed or decrypted.

Options:
  --confirm-development-only  Required destructive-operation acknowledgement.
  --database-url URL          PostgreSQL URL (or DATABASE_URL).
  --database-url-file PATH    File containing the PostgreSQL URL.
  --server-bin PATH           Built open-web-codex server used for --migrate-only.
  --postgres-bin PATH         Directory containing PostgreSQL client tools.
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

probe_psql=""
if [[ -n "$postgres_bin" ]]; then
  probe_psql="$postgres_bin/psql"
elif command -v psql >/dev/null 2>&1; then
  probe_psql="$(command -v psql)"
else
  printf 'error: psql is required to determine the development database server version\n' >&2
  exit 2
fi
[[ -x "$probe_psql" ]] || {
  printf 'error: PostgreSQL client executable is missing: %s\n' "$probe_psql" >&2
  exit 2
}
server_major="$("$probe_psql" "$database_url" -Atq -v ON_ERROR_STOP=1 -c 'SHOW server_version' | sed -E 's/^([0-9]+)\..*/\1/')"
[[ "$server_major" =~ ^[0-9]+$ ]] || {
  printf 'error: could not determine PostgreSQL server major version\n' >&2
  exit 2
}

if [[ -z "$postgres_bin" ]]; then
  candidate_bins=()
  if command -v brew >/dev/null 2>&1; then
    if brew_prefix="$(brew --prefix "postgresql@$server_major" 2>/dev/null)"; then
      candidate_bins+=("$brew_prefix/bin")
    fi
    if brew_prefix="$(brew --prefix postgresql 2>/dev/null)"; then
      candidate_bins+=("$brew_prefix/bin")
    fi
  fi
  if command -v pg_config >/dev/null 2>&1; then
    candidate_bins+=("$(pg_config --bindir)")
  fi
  candidate_bins+=("$(dirname "$probe_psql")")
  postgres_bin=""
  for candidate_bin in "${candidate_bins[@]}"; do
    [[ -x "$candidate_bin/pg_dump" ]] || continue
    client_major="$("$candidate_bin/pg_dump" --version | sed -E 's/.* ([0-9]+)\..*/\1/')"
    if [[ "$client_major" == "$server_major" ]]; then
      postgres_bin="$candidate_bin"
      break
    fi
  done
fi

[[ -n "$postgres_bin" ]] || {
  printf 'error: no PostgreSQL %s client tools are available; install a matching client or provide --postgres-bin\n' "$server_major" >&2
  exit 2
}

for executable in pg_dump pg_restore dropdb createdb psql; do
  [[ -x "$postgres_bin/$executable" ]] || {
    printf 'error: PostgreSQL client executable is missing: %s\n' "$postgres_bin/$executable" >&2
    exit 2
  }
done
client_major="$("$postgres_bin/pg_dump" --version | sed -E 's/.* ([0-9]+)\..*/\1/')"
[[ "$client_major" == "$server_major" ]] || {
  printf 'error: pg_dump major version %s does not match PostgreSQL server major version %s; provide matching --postgres-bin\n' \
    "$client_major" "$server_major" >&2
  exit 2
}
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
backup_file="$backup_dir/configuration.dump"
has_preservable_configuration="0"
preservation_digest_before=""
critical_configuration_digest_before=""
trap 'rm -rf "$backup_dir"' EXIT

preservation_schema_state="$("$postgres_bin/psql" "$database_url" -Atq -v ON_ERROR_STOP=1 -c "
WITH expected(name) AS (
    VALUES ('organizations'), ('users'), ('memberships'), ('profiles'),
           ('profile_secrets'),
           ('platform_configuration'), ('platform_configuration_secrets')
), availability AS (
    SELECT COUNT(*) FILTER (WHERE to_regclass('public.' || name) IS NOT NULL) AS found,
           COUNT(*) AS expected
    FROM expected
)
SELECT CASE
    WHEN found = expected THEN 'complete'
    WHEN found = 0 THEN 'empty'
    ELSE 'incomplete'
END
FROM availability")"

preservation_digest() {
  "$postgres_bin/psql" "$database_url" -Atq -v ON_ERROR_STOP=1 -c "
WITH preserved(value) AS (
    SELECT 'organizations:' || to_jsonb(entry)::text FROM organizations AS entry
    UNION ALL SELECT 'users:' || to_jsonb(entry)::text FROM users AS entry
    UNION ALL SELECT 'memberships:' || to_jsonb(entry)::text FROM memberships AS entry
    UNION ALL SELECT 'profiles:' || to_jsonb(entry)::text FROM profiles AS entry
    UNION ALL SELECT 'profile_secrets:' || to_jsonb(entry)::text FROM profile_secrets AS entry
    UNION ALL SELECT 'platform_configuration:' || to_jsonb(entry)::text FROM platform_configuration AS entry
    UNION ALL SELECT 'platform_configuration_secrets:' || to_jsonb(entry)::text FROM platform_configuration_secrets AS entry
)
SELECT md5(COALESCE(string_agg(value, E'\\n' ORDER BY value), '')) FROM preserved"
}

critical_configuration_digest() {
  "$postgres_bin/psql" "$database_url" -Atq -v ON_ERROR_STOP=1 -c "
WITH preserved(value) AS (
    SELECT 'profile_secrets:' || to_jsonb(entry)::text FROM profile_secrets AS entry
    UNION ALL
        SELECT 'platform_configuration:' || to_jsonb(entry)::text
        FROM platform_configuration AS entry
        WHERE scope_kind = 'global' AND scope_id = 'global'
          AND config_key IN ('models.default_selection', 'maps.mapbox_public_access_token')
    UNION ALL
        SELECT 'platform_configuration_secrets:' || to_jsonb(entry)::text
        FROM platform_configuration_secrets AS entry
        WHERE scope_kind = 'global' AND scope_id = 'global'
          AND config_key IN ('maps.active_credential', 'maps.mapbox_public_access_token')
)
SELECT md5(COALESCE(string_agg(value, E'\\n' ORDER BY value), '')) FROM preserved"
}

case "$preservation_schema_state" in
  complete)
    has_preservable_configuration="1"
    preservation_digest_before="$(preservation_digest)"
    critical_configuration_digest_before="$(critical_configuration_digest)"
    printf 'Exporting encrypted Provider credentials, default model selection, and maps configuration...\n'
    "$postgres_bin/pg_dump" "$database_url" \
      --format=custom --data-only --no-owner --no-privileges \
      --table=organizations --table=users --table=memberships --table=profiles \
      --table=profile_secrets \
      --table=platform_configuration --table=platform_configuration_secrets \
      --file="$backup_file"
    ;;
  empty)
    printf 'No persisted Provider credentials/default model selection/maps configuration exists; starting with an empty development database.\n'
    ;;
  incomplete)
    printf 'error: refusing to rebuild because Provider credential/default model selection or Maps persistence is incomplete; migrate or repair it before destructive rebuild.\n' >&2
    exit 1
    ;;
  *)
    printf 'error: could not determine Provider credential/default model selection/maps preservation state.\n' >&2
    exit 1
    ;;
esac

printf 'Recreating development database...\n'
"$postgres_bin/dropdb" --force --maintenance-db="$maintenance_url" "$database_name"
"$postgres_bin/createdb" --maintenance-db="$maintenance_url" "$database_name"

printf 'Applying current migrations...\n'
"$server_bin" --database-url "$database_url" --migrate-only

if [[ "$has_preservable_configuration" == "1" ]]; then
  printf 'Restoring encrypted Provider credentials, default model selection, and maps configuration...\n'
  "$postgres_bin/pg_restore" --dbname="$database_url" --data-only --exit-on-error \
    --no-owner --no-privileges "$backup_file"
  if [[ "$(preservation_digest)" != "$preservation_digest_before" ]]; then
    printf 'error: restored Provider credentials/default model selection or Maps persistence does not match the encrypted backup.\n' >&2
    exit 1
  fi
  "$postgres_bin/psql" "$database_url" -v ON_ERROR_STOP=1 <<'SQL'
DELETE FROM platform_configuration
WHERE NOT (
    scope_kind = 'global'
    AND scope_id = 'global'
    AND config_key IN ('models.default_selection', 'maps.mapbox_public_access_token')
);

DELETE FROM platform_configuration_secrets
WHERE NOT (
    scope_kind = 'global'
    AND scope_id = 'global'
    AND config_key IN ('maps.active_credential', 'maps.mapbox_public_access_token')
);
SQL
  if [[ "$(critical_configuration_digest)" != "$critical_configuration_digest_before" ]]; then
    printf 'error: rebuilt database did not retain Provider credentials/default model selection or Maps credentials.\n' >&2
    exit 1
  fi
fi

"$script_dir/check-migration-integrity.sh"
printf 'Development database rebuild completed.\n'
