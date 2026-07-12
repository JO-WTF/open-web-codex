#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

./scripts/setup-codex-remotes.sh >/dev/null
git fetch --quiet codex-fork open-codex
git fetch --quiet codex-upstream main

metadata=.sync/codex-upstream.json
integrated="$(jq -r '.integratedUpstreamCommit' "$metadata")"
custom="$(jq -r '.customSourceCommit' "$metadata")"
upstream="$(git rev-parse codex-upstream/main)"

if ! git merge-base --is-ancestor "$integrated" "$upstream"; then
  printf 'Recorded upstream base %s is not an ancestor of %s.\n' "$integrated" "$upstream" >&2
  exit 1
fi

upstream_commits="$(git rev-list --count "$integrated..$upstream")"
custom_commits="$(git rev-list --count "$integrated..$custom")"

printf 'Codex subtree: codex/\n'
printf 'Integrated upstream: %s\n' "$integrated"
printf 'Current upstream:    %s\n' "$upstream"
printf 'Official commits pending integration: %s\n' "$upstream_commits"
printf 'Custom commits above the recorded base: %s\n' "$custom_commits"

if [[ "$integrated" == "$upstream" ]]; then
  printf 'Status: synchronized\n'
else
  printf 'Status: update available\n'
fi
