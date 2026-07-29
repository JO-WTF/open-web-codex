#!/usr/bin/env bash

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"

workspaces=(
  "$repo_root/apps/web"
  "$repo_root/codex/codex-rs"
)
target_dirs=()
preserved_profiles=(release)
candidate_profiles=(ci-test dev dev-small)
custom_workspaces=0
action="enforce"
dry_run=0
strict="${OPEN_WEB_CODEX_TARGET_GC_STRICT:-1}"
high_water_gb="${OPEN_WEB_CODEX_TARGET_LIMIT_GB:-24}"
low_water_gb="${OPEN_WEB_CODEX_TARGET_LOW_WATER_GB:-16}"
high_water_kb=""
low_water_kb=""

usage() {
  cat <<'EOF'
Usage: ./scripts/cargo-target-gc.sh [options]

Enforce a combined high/low-water policy across the Web and Codex Cargo
targets. Release artifacts are always preserved. Cleanup uses only
`cargo clean --profile` and stops after reaching the low-water mark.

Options:
  --status                    Report target/profile sizes without cleaning.
  --dry-run                   Show Cargo cleanup operations without deleting.
  --high-water-gb COUNT       Start cleanup above this size (default: 24).
  --low-water-gb COUNT        Clean down to this size (default: 16).
  --high-water-kb COUNT       KiB override, primarily for deterministic tests.
  --low-water-kb COUNT        KiB override, primarily for deterministic tests.
  --preserve-profile NAME     Preserve an additional active profile.
  --workspace PATH            Replace defaults with one or more workspaces.
  -h, --help                  Show this help.

Environment:
  OPEN_WEB_CODEX_TARGET_LIMIT_GB       High-water mark; 0 disables cleanup.
  OPEN_WEB_CODEX_TARGET_LOW_WATER_GB   Low-water target after cleanup.
  OPEN_WEB_CODEX_TARGET_GC_STRICT      1 fails if the high-water mark remains
                                       exceeded; 0 emits a warning.
  CARGO_TARGET_DIR                     Optional Cargo target override; relative
                                       paths resolve from each workspace.
EOF
}

fail() {
  printf 'error: %s\n' "$*" >&2
  exit 2
}

while (($# > 0)); do
  case "$1" in
    --status) action="status" ;;
    --dry-run) dry_run=1 ;;
    --high-water-gb|--low-water-gb|--high-water-kb|--low-water-kb|--preserve-profile|--workspace)
      (($# >= 2)) || fail "$1 requires a value"
      case "$1" in
        --high-water-gb) high_water_gb="$2" ;;
        --low-water-gb) low_water_gb="$2" ;;
        --high-water-kb) high_water_kb="$2" ;;
        --low-water-kb) low_water_kb="$2" ;;
        --preserve-profile) preserved_profiles+=("$2") ;;
        --workspace)
          if [[ "$custom_workspaces" == "0" ]]; then
            workspaces=()
            custom_workspaces=1
          fi
          workspaces+=("$2")
          ;;
      esac
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *) fail "unknown option: $1" ;;
  esac
  shift
done

case "$strict" in 0|1) ;; *) fail "OPEN_WEB_CODEX_TARGET_GC_STRICT must be 0 or 1" ;; esac
[[ "$high_water_gb" =~ ^[0-9]+$ ]] || fail "high-water GiB must be a non-negative integer"
[[ "$low_water_gb" =~ ^[0-9]+$ ]] || fail "low-water GiB must be a non-negative integer"
[[ -z "$high_water_kb" || "$high_water_kb" =~ ^[0-9]+$ ]] \
  || fail "high-water KiB must be a non-negative integer"
[[ -z "$low_water_kb" || "$low_water_kb" =~ ^[0-9]+$ ]] \
  || fail "low-water KiB must be a non-negative integer"
((${#workspaces[@]} > 0)) || fail "at least one workspace is required"

if [[ -z "$high_water_kb" ]]; then
  high_water_kb=$((high_water_gb * 1024 * 1024))
fi
if [[ -z "$low_water_kb" ]]; then
  low_water_kb=$((low_water_gb * 1024 * 1024))
fi
if ((high_water_kb > 0 && low_water_kb >= high_water_kb)); then
  fail "low-water mark must be lower than the high-water mark"
fi

profile_output_dir() {
  case "$1" in
    dev|test) printf 'debug\n' ;;
    release|bench) printf 'release\n' ;;
    *) printf '%s\n' "$1" ;;
  esac
}

contains_value() {
  local wanted="$1" value
  shift
  for value in "$@"; do
    [[ "$value" == "$wanted" ]] && return 0
  done
  return 1
}

resolve_target_dir() {
  local workspace="$1"
  if [[ -z "${CARGO_TARGET_DIR:-}" ]]; then
    printf '%s/target\n' "$workspace"
  elif [[ "$CARGO_TARGET_DIR" == /* ]]; then
    printf '%s\n' "$CARGO_TARGET_DIR"
  else
    printf '%s/%s\n' "$workspace" "$CARGO_TARGET_DIR"
  fi
}

for workspace in "${workspaces[@]}"; do
  [[ -f "$workspace/Cargo.toml" ]] || fail "Cargo workspace is missing: $workspace"
  target_dir="$(resolve_target_dir "$workspace")"
  if ((${#target_dirs[@]} == 0)); then
    target_dirs+=("$target_dir")
  elif ! contains_value "$target_dir" "${target_dirs[@]}"; then
    target_dirs+=("$target_dir")
  fi
done

directory_size_kb() {
  local path="$1"
  if [[ -d "$path" ]]; then
    du -sk "$path" | awk '{print $1}'
  else
    printf '0\n'
  fi
}

total_target_size_kb() {
  local total=0 target_dir
  for target_dir in "${target_dirs[@]}"; do
    total=$((total + $(directory_size_kb "$target_dir")))
  done
  printf '%s\n' "$total"
}

combined_profile_size_kb() {
  local profile_dir total=0 target_dir
  profile_dir="$(profile_output_dir "$1")"
  for target_dir in "${target_dirs[@]}"; do
    total=$((total + $(directory_size_kb "$target_dir/$profile_dir")))
  done
  printf '%s\n' "$total"
}

format_size_kb() {
  local size_kb="$1"
  if ((size_kb < 1024)); then
    printf '%s KiB' "$size_kb"
  elif ((size_kb % (1024 * 1024) == 0)); then
    printf '%s GiB' "$((size_kb / 1024 / 1024))"
  else
    printf '%s MiB' "$((size_kb / 1024))"
  fi
}

print_status() {
  local profile size_kb
  printf 'Cargo targets: %s (high %s, low %s)\n' \
    "$(format_size_kb "$(total_target_size_kb)")" \
    "$(format_size_kb "$high_water_kb")" \
    "$(format_size_kb "$low_water_kb")"
  for profile in ci-test dev dev-small release; do
    size_kb="$(combined_profile_size_kb "$profile")"
    printf '  %-9s %s\n' "$profile" "$(format_size_kb "$size_kb")"
  done
}

profile_is_preserved() {
  contains_value "$1" "${preserved_profiles[@]}"
}

clean_profile() {
  local profile="$1" profile_dir workspace target_dir
  local -a command
  profile_dir="$(profile_output_dir "$profile")"

  if profile_is_preserved "$profile"; then
    printf 'Preserving active Cargo profile: %s\n' "$profile"
    return 0
  fi
  if (( $(combined_profile_size_kb "$profile") == 0 )); then
    return 0
  fi

  printf 'Cleaning Cargo profile: %s\n' "$profile"
  for workspace in "${workspaces[@]}"; do
    target_dir="$(resolve_target_dir "$workspace")"
    [[ -d "$target_dir/$profile_dir" ]] || continue
    command=(
      cargo clean
      --manifest-path "$workspace/Cargo.toml"
      --target-dir "$target_dir"
      --profile "$profile"
    )
    if [[ "$dry_run" == "1" ]]; then
      command+=(--dry-run --verbose)
    fi
    "${command[@]}"
  done
}

if [[ "$action" == "status" ]]; then
  print_status
  exit 0
fi

if ((high_water_kb == 0)); then
  printf 'Cargo target retention is disabled.\n'
  exit 0
fi

before_kb="$(total_target_size_kb)"
if ((before_kb <= high_water_kb)); then
  print_status
  exit 0
fi

printf 'Cargo targets exceeded the high-water mark: %s > %s\n' \
  "$(format_size_kb "$before_kb")" "$(format_size_kb "$high_water_kb")"

for profile in "${candidate_profiles[@]}"; do
  clean_profile "$profile"
  if [[ "$dry_run" == "0" ]] && (( $(total_target_size_kb) <= low_water_kb )); then
    break
  fi
done

after_kb="$(total_target_size_kb)"
if [[ "$dry_run" == "1" ]]; then
  printf 'Dry run complete; no Cargo artifacts were deleted.\n'
  exit 0
fi

print_status
if ((after_kb > high_water_kb)); then
  message="Cargo targets remain above the high-water mark after all eligible profiles were cleaned"
  if [[ "$strict" == "1" ]]; then
    printf 'error: %s\n' "$message" >&2
    exit 1
  fi
  printf 'warning: %s\n' "$message" >&2
elif ((after_kb > low_water_kb)); then
  printf 'warning: Cargo targets are below the high-water mark but could not reach the low-water mark\n' >&2
fi
