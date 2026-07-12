#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

ensure_remote() {
  local name="$1"
  local url="$2"
  if git remote get-url "$name" >/dev/null 2>&1; then
    git remote set-url "$name" "$url"
  else
    git remote add "$name" "$url"
  fi
}

ensure_remote codex-upstream https://github.com/openai/codex.git
ensure_remote codex-fork https://github.com/JO-WTF/codex.git

printf 'Configured codex-upstream and codex-fork remotes.\n'
