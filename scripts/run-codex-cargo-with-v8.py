#!/usr/bin/env python3
"""Run an existing Codex Cargo command with the official V8 environment."""

import os
import sys
from importlib import import_module
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
CODEX_SCRIPTS = REPO_ROOT / "codex" / "scripts"
if str(CODEX_SCRIPTS) not in sys.path:
    sys.path.insert(0, str(CODEX_SCRIPTS))

from codex_package.targets import TARGET_SPECS, default_target  # noqa: E402
from codex_package.v8 import resolve_codex_v8_cargo_env  # noqa: E402


def configure_native_certificate_store() -> None:
    """Use macOS Keychain trust for the official V8 artifact download."""
    if sys.platform != "darwin":
        return

    for module_name in ("truststore", "pip._vendor.truststore"):
        try:
            truststore = import_module(module_name)
        except ImportError:
            continue
        truststore.inject_into_ssl()
        return


def cargo_environment() -> dict[str, str]:
    """Merge the official Codex V8 overrides into the current environment."""
    configure_native_certificate_store()
    target = default_target()
    spec = TARGET_SPECS[target]
    environment = dict(os.environ)
    environment.update(resolve_codex_v8_cargo_env(spec, environ=environment))
    return environment


def main(argv: list[str] | None = None) -> int:
    cargo_argv = list(sys.argv[1:] if argv is None else argv)
    if not cargo_argv:
        print("usage: run-codex-cargo-with-v8.py CARGO [ARGS...]", file=sys.stderr)
        return 2

    try:
        environment = cargo_environment()
    except Exception as error:  # noqa: BLE001 - preserve a bounded CLI error
        print(f"Unable to prepare Codex Cargo V8 environment: {error}", file=sys.stderr)
        return 1

    os.execvpe(cargo_argv[0], cargo_argv, environment)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
