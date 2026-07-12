#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

if [[ "${1:-}" != "--apply" ]]; then
  ./scripts/codex-upstream-status.sh
  printf '\nNo changes made. Re-run with --apply to create a sync branch.\n'
  exit 0
fi

if [[ -n "$(git status --porcelain)" ]]; then
  printf 'The worktree must be clean before an upstream sync.\n' >&2
  exit 1
fi

./scripts/setup-codex-remotes.sh >/dev/null
git fetch codex-upstream main

metadata=.sync/codex-upstream.json
integrated="$(jq -r '.integratedUpstreamCommit' "$metadata")"
upstream="$(git rev-parse codex-upstream/main)"

if [[ "$integrated" == "$upstream" ]]; then
  printf 'Codex is already synchronized with %s.\n' "$upstream"
  exit 0
fi

short="$(git rev-parse --short=12 "$upstream")"
branch="codex/sync-upstream-$short"
git switch -c "$branch"

printf 'Merging openai/codex main into codex/ on %s.\n' "$branch"
printf 'If conflicts occur, preserve upstream structure and reapply custom provider seams.\n'
git subtree pull \
  --prefix=codex \
  codex-upstream main \
  --squash \
  -m "Sync openai/codex through $short"

tmp="$(mktemp)"
jq \
  --arg integrated "$upstream" \
  --arg observed "$upstream" \
  --arg observed_at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  '.integratedUpstreamCommit = $integrated
   | .observedUpstreamCommit = $observed
   | .observedAt = $observed_at' \
  "$metadata" > "$tmp"
mv "$tmp" "$metadata"
git add "$metadata"
git commit -m "Record Codex upstream $short"

printf 'Sync branch ready: %s\n' "$branch"
printf 'Run the Codex validation matrix before merging it into main.\n'
