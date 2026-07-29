#!/usr/bin/env zsh
set -euo pipefail

script_dir="${0:A:h}"

export E2E_LIFECYCLE_PROBE=1
exec "${script_dir}/smoke-enterprise-supervisor-copilot.sh" "$@"
