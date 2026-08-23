#!/usr/bin/env bash

set -euo pipefail
umask 077

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
environment_root="${OPEN_WEB_CODEX_COPILOT_SDK_ENVIRONMENT_ROOT:-$repo_root/.local/open-web-codex/sdk-environments/copilot}"
python_command="${PYTHON:-python3}"

usage() {
  cat <<'EOF'
Usage: ./scripts/copilot-sdk-bootstrap.sh [--environment-root PATH]

Create or reuse the isolated editable Copilot SDK environment and print its
Python executable. The environment is keyed by the absolute SDK source paths
and both SDK pyproject.toml files.
EOF
}

error() {
  printf 'copilot-sdk-bootstrap: %s\n' "$*" >&2
}

while (($# > 0)); do
  case "$1" in
    --environment-root)
      (($# >= 2)) || { error "$1 requires a value"; exit 2; }
      environment_root="$2"
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      error "unknown option: $1"
      usage >&2
      exit 2
      ;;
  esac
  shift
done

[[ "$environment_root" == /* ]] || {
  error "environment root must be an absolute path"
  exit 2
}
command -v "$python_command" >/dev/null 2>&1 || {
  error "Python executable was not found: $python_command"
  exit 1
}

provider_source="$repo_root/packages/copilot-provider-sdk"
copilot_source="$repo_root/packages/copilot-sdk"
provider_project="$provider_source/pyproject.toml"
copilot_project="$copilot_source/pyproject.toml"
for source_path in \
  "$provider_source" \
  "$copilot_source" \
  "$provider_project" \
  "$copilot_project"
do
  [[ ! -L "$source_path" ]] || {
    error "SDK source paths must not be symlinks: $source_path"
    exit 1
  }
  [[ -e "$source_path" ]] || {
    error "SDK source path is missing: $source_path"
    exit 1
  }
done

source_fingerprint="$(
  "$python_command" - "$provider_project" "$copilot_project" <<'PY'
import hashlib
import sys
from pathlib import Path

digest = hashlib.sha256()
for raw in sys.argv[1:]:
    source = Path(raw).resolve(strict=True)
    encoded = str(source).encode("utf-8")
    contents = source.read_bytes()
    digest.update(len(encoded).to_bytes(8, "big"))
    digest.update(encoded)
    digest.update(len(contents).to_bytes(8, "big"))
    digest.update(contents)
print(digest.hexdigest())
PY
)"

environment_parent="$(dirname "$environment_root")"
mkdir -p "$environment_parent"
[[ ! -L "$environment_root" ]] || {
  error "environment root must not be a symlink"
  exit 1
}

lock_root="${environment_root}.bootstrap-lock"
lock_acquired="0"
release_lock() {
  local recorded_pid=""
  if [[ "$lock_acquired" == "1" && -d "$lock_root" && ! -L "$lock_root" ]]; then
    if [[ -r "$lock_root/pid" && ! -L "$lock_root/pid" ]]; then
      IFS= read -r recorded_pid <"$lock_root/pid" || true
    fi
    if [[ "$recorded_pid" == "$$" ]]; then
      rm -f -- "$lock_root/pid"
      rmdir "$lock_root" 2>/dev/null || true
    fi
  fi
}
trap release_lock EXIT
trap 'release_lock; exit 130' INT TERM HUP

for attempt in $(seq 1 600); do
  if mkdir "$lock_root" 2>/dev/null; then
    lock_acquired="1"
    printf '%s\n' "$$" >"$lock_root/pid"
    break
  fi
  [[ ! -L "$lock_root" ]] || {
    error "bootstrap lock must not be a symlink"
    exit 1
  }
  recorded_pid=""
  if [[ -r "$lock_root/pid" && ! -L "$lock_root/pid" ]]; then
    IFS= read -r recorded_pid <"$lock_root/pid" || true
  fi
  if [[ "$recorded_pid" =~ ^[1-9][0-9]*$ ]] && ! kill -0 "$recorded_pid" 2>/dev/null; then
    stale_lock="${lock_root}.stale.$$.$attempt"
    if mv "$lock_root" "$stale_lock" 2>/dev/null; then
      [[ ! -L "$stale_lock/pid" ]] || {
        error "stale bootstrap lock contains a symlink"
        exit 1
      }
      rm -f -- "$stale_lock/pid"
      rmdir "$stale_lock" 2>/dev/null || {
        error "stale bootstrap lock contains unexpected entries"
        exit 1
      }
      continue
    fi
  fi
  if ((attempt == 600)); then
    error "timed out waiting for the Copilot SDK bootstrap lock"
    exit 1
  fi
  sleep 0.1
done

[[ "$lock_acquired" == "1" ]] || {
  error "failed to acquire the Copilot SDK bootstrap lock"
  exit 1
}
[[ ! -L "$environment_root" ]] || {
  error "environment root became a symlink while waiting for the lock"
  exit 1
}
if [[ -e "$environment_root" && ! -d "$environment_root" ]]; then
  error "environment root must be a directory"
  exit 1
fi

marker="$environment_root/source-fingerprint"
[[ ! -L "$marker" ]] || {
  error "source fingerprint marker must not be a symlink"
  exit 1
}
if [[ -d "$environment_root" && ! -e "$environment_root/pyvenv.cfg" ]]; then
  first_entry="$(find "$environment_root" -mindepth 1 -maxdepth 1 -print -quit)"
  [[ -z "$first_entry" ]] || {
    error "existing environment root is not an owned Python virtual environment"
    exit 1
  }
fi

venv_python="$environment_root/bin/python"
if [[ ! -x "$venv_python" ]]; then
  "$python_command" -m venv --copies "$environment_root"
fi
[[ -x "$venv_python" ]] || {
  error "virtual environment Python is missing after creation"
  exit 1
}

metadata_is_current() {
  "$venv_python" - "$provider_source" "$copilot_source" <<'PY' >/dev/null 2>&1
import importlib
import importlib.metadata
import json
import sys
import tomllib
from pathlib import Path
from urllib.parse import unquote, urlparse
from urllib.request import url2pathname

expected = (
    ("open-web-codex-provider-sdk", "open_web_codex_provider", Path(sys.argv[1]).resolve()),
    ("open-web-codex-copilot-sdk", "copilot_sdk", Path(sys.argv[2]).resolve()),
)
for distribution_name, import_name, source in expected:
    with (source / "pyproject.toml").open("rb") as handle:
        declared_version = tomllib.load(handle)["project"]["version"]
    distribution = importlib.metadata.distribution(distribution_name)
    if distribution.version != declared_version:
        raise SystemExit(1)
    direct_url_text = distribution.read_text("direct_url.json")
    if direct_url_text is None:
        raise SystemExit(1)
    direct_url = json.loads(direct_url_text)
    parsed = urlparse(direct_url.get("url", ""))
    installed_source = Path(url2pathname(unquote(parsed.path))).resolve()
    if parsed.scheme != "file" or installed_source != source:
        raise SystemExit(1)
    if direct_url.get("dir_info", {}).get("editable") is not True:
        raise SystemExit(1)
    importlib.import_module(import_name)
PY
}

installed_fingerprint=""
if [[ -r "$marker" ]]; then
  IFS= read -r installed_fingerprint <"$marker" || true
fi
if [[ "$installed_fingerprint" != "$source_fingerprint" ]] || ! metadata_is_current; then
  "$venv_python" -m pip install --disable-pip-version-check --upgrade \
    --editable "$provider_source" \
    --editable "$copilot_source" >&2
  metadata_is_current || {
    error "editable SDK metadata verification failed"
    exit 1
  }
  marker_temporary="$environment_root/.source-fingerprint.$$"
  [[ ! -L "$marker_temporary" ]] || {
    error "temporary source fingerprint marker must not be a symlink"
    exit 1
  }
  printf '%s\n' "$source_fingerprint" >"$marker_temporary"
  chmod 600 "$marker_temporary"
  mv -f -- "$marker_temporary" "$marker"
fi

printf '%s\n' "$venv_python"
