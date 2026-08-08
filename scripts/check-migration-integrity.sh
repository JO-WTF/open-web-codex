#!/usr/bin/env bash

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
migrations_dir="$repo_root/apps/web/migrations"
manifest="$migrations_dir/SHA256SUMS"

if [[ ! -f "$manifest" ]]; then
  printf 'error: migration manifest is missing: %s\n' "$manifest" >&2
  exit 1
fi

temporary_manifest="$(mktemp "${TMPDIR:-/tmp}/open-web-codex-migrations.XXXXXX")"
trap 'rm -f "$temporary_manifest"' EXIT

while IFS= read -r migration; do
  checksum="$(shasum -a 256 "$migration" | awk '{print $1}')"
  printf '%s  %s\n' "$checksum" "${migration#"$migrations_dir/"}" >>"$temporary_manifest"
done < <(find "$migrations_dir" -maxdepth 1 -type f -name '*.sql' -print | sort)

if ! diff -u "$manifest" "$temporary_manifest"; then
  printf '\nerror: migration contents or inventory changed. Update SHA256SUMS in the same reviewed change.\n' >&2
  exit 1
fi

printf 'migration integrity verified (%s files)\n' "$(wc -l <"$manifest" | tr -d ' ')"
