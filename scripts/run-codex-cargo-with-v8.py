#!/usr/bin/env python3
"""Run an existing Codex Cargo command with the official V8 environment."""

import os
import sys
from importlib import import_module
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
CODEX_REPO_ROOT = (REPO_ROOT / "codex").resolve()


def configure_codex_repo_root() -> None:
    """Bind upstream package helpers to this checkout before importing them."""
    configured = os.environ.get("CODEX_REPO_ROOT")
    if configured is None:
        os.environ["CODEX_REPO_ROOT"] = str(CODEX_REPO_ROOT)
        return

    try:
        configured_root = Path(configured).expanduser().resolve(strict=True)
    except (OSError, RuntimeError) as error:
        raise RuntimeError(
            "CODEX_REPO_ROOT must point to this checkout's codex directory"
        ) from error
    if configured_root != CODEX_REPO_ROOT:
        raise RuntimeError(
            "CODEX_REPO_ROOT must point to this checkout's codex directory"
        )


configure_codex_repo_root()

CODEX_SCRIPTS = CODEX_REPO_ROOT / "scripts"
if str(CODEX_SCRIPTS) not in sys.path:
    sys.path.insert(0, str(CODEX_SCRIPTS))

from codex_package.targets import TARGET_SPECS, TargetSpec, default_target  # noqa: E402
from codex_package.v8 import V8_ARTIFACT_PROFILE  # noqa: E402
from codex_package.v8 import RustyV8ArtifactPair  # noqa: E402
from codex_package.v8 import has_checksum  # noqa: E402
from codex_package.v8 import load_checksums  # noqa: E402
from codex_package.v8 import resolve_codex_v8_cargo_env  # noqa: E402
from codex_package.v8 import resolved_v8_crate_version  # noqa: E402


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


def codex_v8_cache_root(environ: dict[str, str]) -> Path:
    """Return the durable, user-overridable cache for verified V8 artifacts."""
    configured = environ.get("OPEN_WEB_CODEX_V8_CACHE_DIR")
    if configured:
        return Path(configured).expanduser()

    if sys.platform == "darwin":
        return Path.home() / "Library" / "Caches" / "open-web-codex" / "codex-v8"
    return (
        Path(environ.get("XDG_CACHE_HOME", Path.home() / ".cache"))
        / "open-web-codex"
        / "codex-v8"
    )


def cached_codex_v8_artifacts(
    spec: TargetSpec, *, cache_root: Path
) -> RustyV8ArtifactPair | None:
    """Return an already checksum-verified pair without touching the network."""
    version = resolved_v8_crate_version()
    target = spec.target
    cache_dir = cache_root / f"rusty-v8-{version}-{target}"
    if spec.is_windows:
        archive_name = f"rusty_v8_{V8_ARTIFACT_PROFILE}_{target}.lib.gz"
    else:
        archive_name = f"librusty_v8_{V8_ARTIFACT_PROFILE}_{target}.a.gz"
    binding_name = f"src_binding_{V8_ARTIFACT_PROFILE}_{target}.rs"
    checksums = cache_dir / f"rusty_v8_{V8_ARTIFACT_PROFILE}_{target}.sha256"
    archive = cache_dir / archive_name
    binding = cache_dir / binding_name

    try:
        expected = load_checksums(checksums, {archive_name, binding_name})
    except (OSError, RuntimeError):
        return None
    if not has_checksum(archive, expected[archive_name]):
        return None
    if not has_checksum(binding, expected[binding_name]):
        return None
    return RustyV8ArtifactPair(archive=archive, binding=binding)


def resolve_cached_v8_cargo_env(
    spec: TargetSpec, *, environ: dict[str, str]
) -> dict[str, str]:
    """Use the durable verified cache before the official one-time resolver."""
    if environ.get("V8_FROM_SOURCE") in {"true", "1", "yes"}:
        return {}

    archive_override = environ.get("RUSTY_V8_ARCHIVE")
    binding_override = environ.get("RUSTY_V8_SRC_BINDING_PATH")
    if archive_override and binding_override:
        return {}
    if archive_override or binding_override:
        raise RuntimeError(
            "Cargo package builds need RUSTY_V8_ARCHIVE and "
            "RUSTY_V8_SRC_BINDING_PATH set together."
        )

    cache_root = codex_v8_cache_root(environ)
    artifacts = cached_codex_v8_artifacts(spec, cache_root=cache_root)
    if artifacts is None:
        return resolve_codex_v8_cargo_env(
            spec,
            environ=environ,
            cache_root=cache_root,
        )
    return {
        "RUSTY_V8_ARCHIVE": str(artifacts.archive),
        "RUSTY_V8_SRC_BINDING_PATH": str(artifacts.binding),
    }


def cargo_environment() -> dict[str, str]:
    """Merge the official Codex V8 overrides into the current environment."""
    configure_native_certificate_store()
    target = default_target()
    spec = TARGET_SPECS[target]
    environment = dict(os.environ)
    environment.update(resolve_cached_v8_cargo_env(spec, environ=environment))
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
