#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

./scripts/setup-codex-remotes.sh >/dev/null
git fetch --quiet codex-upstream main

upstream="$(git rev-parse codex-upstream/main)"
local_tree="HEAD:codex"

python3 - "$upstream" "$local_tree" <<'PY'
import subprocess
import sys


def tree_entries(treeish: str) -> dict[str, tuple[str, str, str]]:
    output = subprocess.check_output(["git", "ls-tree", "-r", "-z", treeish])
    entries = {}
    for record in output.split(b"\0"):
        if not record:
            continue
        metadata, path = record.split(b"\t", 1)
        mode, kind, object_id = metadata.decode().split(" ", 2)
        entries[path.decode()] = (mode, kind, object_id)
    return entries


upstream = tree_entries(sys.argv[1])
local = tree_entries(sys.argv[2])
changes = []

for path in sorted(set(upstream) | set(local)):
    if path not in upstream:
        status = "A"
    elif path not in local:
        status = "D"
    elif upstream[path] != local[path]:
        status = "M"
    else:
        continue
    changes.append((status, path))

counts = {status: sum(1 for current, _ in changes if current == status) for status in "AMD"}
print(f"Official upstream: {sys.argv[1]}")
print(f"Local Codex tree: {sys.argv[2]}")
print(
    "Differences: "
    f"{len(changes)} "
    f"(added locally: {counts['A']}, modified: {counts['M']}, missing locally: {counts['D']})"
)
for status, path in changes:
    print(f"{status}\t{path}")
PY
