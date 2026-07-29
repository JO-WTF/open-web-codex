#!/usr/bin/env bash

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"

# shellcheck source=scripts/cargo-build-cache.sh
source "$script_dir/cargo-build-cache.sh"
cargo_build_cache_configure "$repo_root"
cargo_build_cache_describe

test_status=0
gc_status=0
test_args=("$@")
if (($# == 0)); then
  test_args=(--workspace)
fi

"$script_dir/cargo-target-gc.sh" --preserve-profile dev-small
(cd "$repo_root/apps/web" && cargo test --locked --profile ci-test "${test_args[@]}") \
  || test_status=$?
"$script_dir/cargo-target-gc.sh" --preserve-profile dev-small || gc_status=$?

if ((test_status != 0)); then
  exit "$test_status"
fi
exit "$gc_status"
