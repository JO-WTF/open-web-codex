#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

./scripts/setup-codex-remotes.sh >/dev/null
git fetch --quiet codex-upstream main

upstream="$(git rev-parse codex-upstream/main)"
integrated="$(jq -r '.integratedUpstreamCommit' .sync/codex-upstream.json)"
local_tree="HEAD:codex"

python3 - "$integrated" "$upstream" "$local_tree" <<'PY'
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


integrated = tree_entries(sys.argv[1])
upstream = tree_entries(sys.argv[2])
local = tree_entries(sys.argv[3])
changes = []
classes = {
    "upstream-only": [],
    "local-only": [],
    "diverged": [],
}

for path in sorted(set(integrated) | set(upstream) | set(local)):
    if path not in upstream:
        status = "A"
    elif path not in local:
        status = "D"
    elif upstream[path] != local[path]:
        status = "M"
    else:
        continue
    changes.append((status, path))
    if local.get(path) == integrated.get(path):
        classes["upstream-only"].append(path)
    elif upstream.get(path) == integrated.get(path):
        classes["local-only"].append(path)
    else:
        classes["diverged"].append(path)

counts = {status: sum(1 for current, _ in changes if current == status) for status in "AMD"}
print(f"Integrated upstream: {sys.argv[1]}")
print(f"Official upstream: {sys.argv[2]}")
print(f"Local Codex tree: {sys.argv[3]}")
print(
    "Differences: "
    f"{len(changes)} "
    f"(added locally: {counts['A']}, modified: {counts['M']}, missing locally: {counts['D']})"
)
print(
    "Classification: "
    f"upstream-only={len(classes['upstream-only'])}, "
    f"local-only={len(classes['local-only'])}, "
    f"diverged={len(classes['diverged'])}"
)
for status, path in changes:
    if path in classes["upstream-only"]:
        classification = "upstream-only"
    elif path in classes["local-only"]:
        classification = "local-only"
    else:
        classification = "diverged"
    print(f"{classification}\t{status}\t{path}")
PY
