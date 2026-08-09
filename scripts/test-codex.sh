#!/usr/bin/env bash

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"

# shellcheck source=scripts/cargo-build-cache.sh
source "$script_dir/cargo-build-cache.sh"
cargo_build_cache_configure "$repo_root"
cargo_build_cache_describe

(cd "$repo_root/codex" && just test --cargo-profile ci-test "$@")
