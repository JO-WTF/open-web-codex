#!/usr/bin/env bash

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
provider_root="$repo_root/packages/copilot-provider-sdk"
example_root="$provider_root/examples/record-review"

command -v uv >/dev/null 2>&1 || {
  printf 'provider-example-smoke: uv is required\n' >&2
  exit 1
}

uv_base=(
  uv run --isolated --no-project
  --with-editable "$provider_root"
  --with pytest
)

(
  cd "$provider_root"
  "${uv_base[@]}" python -m pytest tests -q
)
(
  cd "$example_root"
  uv run --isolated --project "$example_root" --with pytest \
    python -m pytest tests/test_record_review_provider.py -q
  uv run --isolated --project "$example_root" \
    python tests/stdio_smoke.py
)
