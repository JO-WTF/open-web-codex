"""Registry of platform-owned Python distributions available to Tool runtimes."""

from __future__ import annotations

import importlib.metadata
import importlib.util
import re
import stat
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class PlatformPackageSpec:
    id: str
    distribution: str
    version_prefix: str
    import_packages: tuple[str, ...]


@dataclass(frozen=True)
class ResolvedPlatformPackage:
    spec: PlatformPackageSpec
    version: str
    package_roots: tuple[Path, ...]


_PACKAGES = {
    "open-web-codex-provider-sdk": PlatformPackageSpec(
        id="open-web-codex-provider-sdk",
        distribution="open-web-codex-provider-sdk",
        version_prefix="0.1.",
        import_packages=("open_web_codex_provider",),
    ),
}


def platform_package(package_id: str) -> PlatformPackageSpec:
    """Resolve one declared platform package without name or path inference."""

    try:
        return _PACKAGES[package_id]
    except KeyError as error:
        raise ValueError("platform_package_unregistered") from error


def resolve_installed_platform_package(package_id: str) -> ResolvedPlatformPackage:
    """Resolve one registry entry from the SDK process's installed distributions."""

    spec = platform_package(package_id)
    try:
        version = importlib.metadata.version(spec.distribution)
    except importlib.metadata.PackageNotFoundError as error:
        raise ValueError("platform_package_unavailable") from error
    if not version.startswith(spec.version_prefix) or re.fullmatch(r"[0-9A-Za-z_.+-]+", version) is None:
        raise ValueError("platform_package_version_invalid")
    roots: list[Path] = []
    for import_package in spec.import_packages:
        module = importlib.util.find_spec(import_package)
        locations = tuple(module.submodule_search_locations or ()) if module is not None else ()
        if len(locations) != 1:
            raise ValueError("platform_package_unavailable")
        root = Path(locations[0])
        try:
            if stat.S_ISLNK(root.lstat().st_mode):
                raise ValueError("platform_package_unsafe")
            root = root.resolve(strict=True)
        except OSError as error:
            raise ValueError("platform_package_unavailable") from error
        if not root.is_dir():
            raise ValueError("platform_package_unavailable")
        roots.append(root)
    return ResolvedPlatformPackage(spec, version, tuple(roots))
