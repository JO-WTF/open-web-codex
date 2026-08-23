#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
launcher="$repo_root/scripts/run-local.sh"
fixture_root="$(mktemp -d "${TMPDIR:-/tmp}/open-web-codex-run-local-rebuild.XXXXXX")"

cleanup() {
  rm -rf -- "$fixture_root"
}
trap cleanup EXIT

bash -n "$launcher"
! grep -F -- '--refresh-local' "$launcher" >/dev/null
grep -F -- '--rebuild-development-database' "$launcher" >/dev/null
! grep -F -- '--codex-bin PATH' "$launcher" >/dev/null
if "$launcher" --codex-bin /tmp/codex >/dev/null 2>&1; then
  printf 'external Codex binary option unexpectedly succeeded\n' >&2
  exit 1
fi
if CODEX_BIN=/tmp/codex "$launcher" >/dev/null 2>&1; then
  printf 'external Codex binary environment unexpectedly succeeded\n' >&2
  exit 1
fi

if "$launcher" --refresh-local >/dev/null 2>&1; then
  printf 'removed refresh option unexpectedly succeeded\n' >&2
  exit 1
fi

for rejected in \
  "$launcher --rebuild-development-database --status" \
  "$launcher --rebuild-development-database --stop" \
  "env OPEN_WEB_CODEX_DATA_DIR=$fixture_root $launcher --rebuild-development-database" \
  "env DATABASE_URL=postgresql://invalid@127.0.0.1:5432/invalid $launcher --background --rebuild-development-database"
do
  if zsh -c "$rejected" >/dev/null 2>&1; then
    printf 'unsafe rebuild invocation unexpectedly succeeded: %s\n' "$rejected" >&2
    exit 1
  fi
done

printf 'run-local rebuild option contract passed\n'
