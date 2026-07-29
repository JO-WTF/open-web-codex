#!/usr/bin/env bash

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"
temporary_root="$(mktemp -d)"
trap 'find "$temporary_root" -depth -delete' EXIT

# shellcheck source=scripts/cargo-build-cache.sh
source "$repo_root/scripts/cargo-build-cache.sh"
[[ "$(cargo_build_cache_size_bytes 8G)" == "8589934592" ]]
[[ "$(cargo_build_cache_size_bytes 512MiB)" == "536870912" ]]
if cargo_build_cache_size_bytes invalid >/dev/null 2>&1; then
  printf 'invalid sccache size was accepted\n' >&2
  exit 1
fi
if cargo_build_cache_size_bytes 0 >/dev/null 2>&1; then
  printf 'zero sccache size was accepted\n' >&2
  exit 1
fi

cache_lock_root="$temporary_root/cache-lock"
critical_section="$cache_lock_root/critical-section"
mkdir -p "$cache_lock_root"

run_cache_lock_probe() {
  bash -c '
    set -euo pipefail
    source "$1"
    export SCCACHE_DIR="$2"
    cargo_build_cache_acquire_server_lock
    mkdir "$3"
    sleep 0.1
    rmdir "$3"
    cargo_build_cache_release_server_lock
  ' _ "$repo_root/scripts/cargo-build-cache.sh" "$cache_lock_root" "$critical_section"
}

run_cache_lock_probe &
first_lock_probe=$!
run_cache_lock_probe &
second_lock_probe=$!
wait "$first_lock_probe"
wait "$second_lock_probe"

create_workspace() {
  local workspace="$1" name="$2"
  mkdir -p "$workspace/src"
  printf 'pub fn value() -> u8 { 1 }\n' >"$workspace/src/lib.rs"
  cat >"$workspace/Cargo.toml" <<EOF
[package]
name = "$name"
version = "0.1.0"
edition = "2024"

[profile.dev-small]
inherits = "dev"
debug = "none"

[profile.ci-test]
inherits = "test"
debug = "limited"
EOF
}

allocate_kb() {
  local path="$1" size_kb="$2"
  mkdir -p "$(dirname "$path")"
  dd if=/dev/zero of="$path" bs=1024 count="$size_kb" status=none
}

web_workspace="$temporary_root/web"
runtime_workspace="$temporary_root/runtime"
create_workspace "$web_workspace" "retention-web"
create_workspace "$runtime_workspace" "retention-runtime"

for workspace in "$web_workspace" "$runtime_workspace"; do
  allocate_kb "$workspace/target/ci-test/payload" 48
  allocate_kb "$workspace/target/dev-small/payload" 24
  allocate_kb "$workspace/target/release/payload" 8
done

"$repo_root/scripts/cargo-target-gc.sh" \
  --workspace "$web_workspace" \
  --workspace "$runtime_workspace" \
  --high-water-kb 100 \
  --low-water-kb 24

[[ ! -d "$web_workspace/target/ci-test" ]]
[[ ! -d "$runtime_workspace/target/ci-test" ]]
[[ ! -d "$web_workspace/target/dev-small" ]]
[[ ! -d "$runtime_workspace/target/dev-small" ]]
[[ -f "$web_workspace/target/release/payload" ]]
[[ -f "$runtime_workspace/target/release/payload" ]]

allocate_kb "$web_workspace/target/ci-test/payload" 48
allocate_kb "$runtime_workspace/target/ci-test/payload" 48

"$repo_root/scripts/cargo-target-gc.sh" \
  --workspace "$web_workspace" \
  --workspace "$runtime_workspace" \
  --high-water-kb 80 \
  --low-water-kb 24 \
  --dry-run

[[ -f "$web_workspace/target/ci-test/payload" ]]
[[ -f "$runtime_workspace/target/ci-test/payload" ]]

relative_web_workspace="$temporary_root/relative-web"
relative_runtime_workspace="$temporary_root/relative-runtime"
create_workspace "$relative_web_workspace" "retention-relative-web"
create_workspace "$relative_runtime_workspace" "retention-relative-runtime"
allocate_kb "$relative_web_workspace/custom-target/ci-test/payload" 48
allocate_kb "$relative_runtime_workspace/custom-target/ci-test/payload" 48

CARGO_TARGET_DIR=custom-target "$repo_root/scripts/cargo-target-gc.sh" \
  --workspace "$relative_web_workspace" \
  --workspace "$relative_runtime_workspace" \
  --high-water-kb 80 \
  --low-water-kb 16

[[ ! -d "$relative_web_workspace/custom-target/ci-test" ]]
[[ ! -d "$relative_runtime_workspace/custom-target/ci-test" ]]

strict_web_workspace="$temporary_root/strict-web"
strict_runtime_workspace="$temporary_root/strict-runtime"
create_workspace "$strict_web_workspace" "retention-strict-web"
create_workspace "$strict_runtime_workspace" "retention-strict-runtime"
allocate_kb "$strict_web_workspace/target/release/payload" 64
allocate_kb "$strict_runtime_workspace/target/release/payload" 64

if "$repo_root/scripts/cargo-target-gc.sh" \
  --workspace "$strict_web_workspace" \
  --workspace "$strict_runtime_workspace" \
  --high-water-kb 80 \
  --low-water-kb 24 >/dev/null 2>&1
then
  printf 'retention unexpectedly succeeded above high water with release-only data\n' >&2
  exit 1
fi
[[ -f "$strict_web_workspace/target/release/payload" ]]
[[ -f "$strict_runtime_workspace/target/release/payload" ]]

if "$repo_root/scripts/cargo-target-gc.sh" \
  --workspace "$strict_web_workspace" \
  --high-water-kb 80 \
  --low-water-kb 80 >/dev/null 2>&1
then
  printf 'invalid high/low-water configuration was accepted\n' >&2
  exit 1
fi

printf 'cargo build retention tests: OK\n'
