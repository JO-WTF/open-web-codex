#!/usr/bin/env python3
"""Executable contract smoke for the repository Copilot developer docs."""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import tempfile
from collections.abc import Iterable, Sequence
from pathlib import Path
from urllib.parse import unquote, urlsplit

import tomllib


class DocsSmokeError(RuntimeError):
    """One bounded developer-documentation contract failure."""


LINK_PATTERN = re.compile(r"!?\[[^\]]*\]\(([^)]+)\)")
HEADING_PATTERN = re.compile(r"^(#{1,6})\s+(.+?)\s*$")
FENCE_PATTERN = re.compile(r"^\s*(`{3,}|~{3,})")

EXTRA_MARKDOWN_FILES = (
    "docs/README.md",
    "docs/tutorials/README.md",
    "docs/tutorials/copilot-developer-quickstart.md",
    "docs/tutorials/warehouse-copilot-developer-tutorial.md",
    "docs/tutorials/hello-agent-quickstart.md",
    "packages/copilot-sdk/README.md",
    "packages/copilot-provider-sdk/README.md",
    "packages/copilot-provider-sdk/examples/record-review/README.md",
)


def _outside_fences(text: str) -> Iterable[tuple[int, str]]:
    active_fence: str | None = None
    for line_number, line in enumerate(text.splitlines(), 1):
        match = FENCE_PATTERN.match(line)
        if match:
            marker = match.group(1)
            if active_fence is None:
                active_fence = marker[0]
            elif marker[0] == active_fence:
                active_fence = None
            continue
        if active_fence is None:
            yield line_number, line


def github_anchors(path: Path) -> set[str]:
    """Return GitHub-style heading anchors, including duplicate suffixes."""

    anchors: set[str] = set()
    counts: dict[str, int] = {}
    for _line_number, line in _outside_fences(path.read_text(encoding="utf-8")):
        match = HEADING_PATTERN.match(line)
        if match is None:
            continue
        heading = match.group(2).strip().rstrip("#").strip().lower()
        heading = re.sub(r"<[^>]+>", "", heading)
        heading = re.sub(r"!?\[([^\]]+)\]\([^)]+\)", r"\1", heading)
        heading = heading.replace("`", "")
        heading = re.sub(r"[^\w\- ]", "", heading, flags=re.UNICODE)
        base = re.sub(r"\s+", "-", heading).strip("-")
        if not base:
            continue
        duplicate = counts.get(base, 0)
        counts[base] = duplicate + 1
        anchors.add(base if duplicate == 0 else f"{base}-{duplicate}")
    return anchors


def _link_target(raw_target: str) -> str:
    target = raw_target.strip()
    if target.startswith("<") and target.endswith(">"):
        return target[1:-1]
    # None of the governed docs use Markdown link titles. Rejecting a quoted
    # suffix would make a future accidental space fail visibly instead of
    # resolving a different local path.
    return target


def check_markdown_links(paths: Sequence[Path]) -> int:
    """Validate every local link and local heading anchor in the governed docs."""

    failures: list[str] = []
    checked = 0
    anchor_cache: dict[Path, set[str]] = {}
    for source in paths:
        if not source.is_file():
            failures.append(f"missing governed Markdown file: {source}")
            continue
        for line_number, line in _outside_fences(source.read_text(encoding="utf-8")):
            for match in LINK_PATTERN.finditer(line):
                raw_target = _link_target(match.group(1))
                parsed = urlsplit(raw_target)
                if parsed.scheme or parsed.netloc:
                    continue
                checked += 1
                relative = unquote(parsed.path)
                destination = (
                    source if not relative else (source.parent / relative).resolve()
                )
                if not destination.exists():
                    failures.append(
                        f"{source}:{line_number}: missing link target {raw_target!r}"
                    )
                    continue
                if parsed.fragment and destination.is_file():
                    fragment = unquote(parsed.fragment).lower()
                    anchors = anchor_cache.setdefault(
                        destination, github_anchors(destination)
                    )
                    if fragment not in anchors:
                        failures.append(
                            f"{source}:{line_number}: missing anchor #{parsed.fragment} "
                            f"in {destination}"
                        )
    if failures:
        raise DocsSmokeError("\n".join(failures))
    return checked


def governed_markdown(repo_root: Path) -> list[Path]:
    center = sorted((repo_root / "docs/developers/copilot").rglob("*.md"))
    extras = [repo_root / relative for relative in EXTRA_MARKDOWN_FILES]
    return [*center, *extras]


def _run(
    arguments: Sequence[str],
    *,
    cwd: Path,
    timeout: float = 180.0,
) -> subprocess.CompletedProcess[str]:
    try:
        completed = subprocess.run(
            arguments,
            cwd=cwd,
            text=True,
            capture_output=True,
            timeout=timeout,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        raise DocsSmokeError(
            f"command could not complete: {' '.join(arguments)}"
        ) from error
    if completed.returncode != 0:
        detail = (completed.stderr or completed.stdout).strip()
        raise DocsSmokeError(
            f"command failed ({completed.returncode}): {' '.join(arguments)}\n{detail}"
        )
    return completed


def _assert_contains(label: str, text: str, expected: Sequence[str]) -> None:
    missing = [value for value in expected if value not in text]
    if missing:
        raise DocsSmokeError(f"{label} help is missing: {', '.join(missing)}")


def check_cli_help(repo_root: Path) -> None:
    wrapper = "./scripts/copilot.sh"
    top = _run([wrapper, "--help"], cwd=repo_root).stdout
    _assert_contains(
        "top-level",
        top,
        (
            "init PATH --name ID [--template single-agent|multi-agent]",
            "tool init PATH --name ID",
            "validate SOURCE",
            "check SOURCE",
            "sync copilots/PACKAGE [--workspace ABS] [--timeout-seconds SEC] [--build]",
            "dev SOURCE --workspace PATH",
            "test SOURCE --workspace PATH",
        ),
    )
    commands = {
        ("init", "--help"): ("--name", "--template", "single-agent", "multi-agent"),
        ("tool", "init", "--help"): ("--name", "--json"),
        ("validate", "--help"): ("source_root", "--manifest", "--json"),
        ("check", "--help"): (
            "source_root",
            "--workspace",
            "--manifest",
            "--case",
            "--timeout-seconds",
            "--json",
        ),
        ("dev", "--help"): ("source_root", "--workspace", "--timeout-seconds"),
        ("test", "--help"): ("source_root", "--workspace", "--case"),
    }
    for suffix, expected in commands.items():
        completed = _run([wrapper, *suffix], cwd=repo_root)
        _assert_contains(" ".join(suffix), completed.stdout, expected)


def _json_command(
    arguments: Sequence[str], *, cwd: Path, timeout: float = 180.0
) -> dict:
    completed = _run(arguments, cwd=cwd, timeout=timeout)
    try:
        payload = json.loads(completed.stdout)
    except json.JSONDecodeError as error:
        raise DocsSmokeError(
            f"command did not return JSON: {' '.join(arguments)}"
        ) from error
    if not isinstance(payload, dict) or payload.get("ok") is not True:
        raise DocsSmokeError(f"command did not report ok=true: {' '.join(arguments)}")
    return payload


def _load_toml(path: Path) -> dict:
    with path.open("rb") as stream:
        return tomllib.load(stream)


def check_fresh_packages(repo_root: Path, *, run_runtime: bool) -> str:
    """Exercise the documented fresh init, Tool init, validate, and check path."""

    wrapper = "./scripts/copilot.sh"
    with tempfile.TemporaryDirectory(prefix="copilot-docs-smoke-") as directory:
        root = Path(directory)
        single = root / "single" / "record-review"
        multi = root / "multi" / "record-review"
        tool = root / "tool" / "record-review"

        _json_command(
            [wrapper, "init", str(single), "--name", "record-review", "--json"],
            cwd=repo_root,
        )
        _json_command(
            [
                wrapper,
                "init",
                str(multi),
                "--name",
                "record-review",
                "--template",
                "multi-agent",
                "--json",
            ],
            cwd=repo_root,
        )
        _json_command(
            [wrapper, "tool", "init", str(tool), "--name", "record-review", "--json"],
            cwd=repo_root,
        )

        single_manifest = _load_toml(single / "copilot.toml")
        multi_manifest = _load_toml(multi / "copilot.toml")
        tool_manifest = _load_toml(tool / "tool.toml")
        tool_runtime = _load_toml(tool / "runtime.toml")
        if (
            single_manifest.get("id") != "record-review"
            or single_manifest.get("root", {}).get("agent") != "record-review-root"
            or len(single_manifest.get("skills", [])) != 1
            or len(single_manifest.get("agents", [])) != 1
            or len(single_manifest.get("tests", [])) != 1
        ):
            raise DocsSmokeError(
                "fresh single-agent manifest does not match the documented shape"
            )
        if (
            multi_manifest.get("id") != "record-review"
            or "agent" in multi_manifest.get("root", {})
            or len(multi_manifest.get("skills", [])) != 2
            or len(multi_manifest.get("agents", [])) != 1
            or len(multi_manifest.get("tests", [])) != 1
        ):
            raise DocsSmokeError(
                "fresh multi-agent manifest does not match the documented shape"
            )
        if (
            tool_manifest.get("id") != "record-review"
            or tool_manifest.get("runtime") != "runtime.toml"
            or tool_runtime.get("servers", [{}])[0].get("id") != "record_review"
        ):
            raise DocsSmokeError(
                "fresh shared Tool manifest does not match the documented shape"
            )

        for package in (single, multi):
            payload = _json_command(
                [wrapper, "validate", str(package), "--json"], cwd=repo_root
            )
            if payload.get("copilot", {}).get("id") != "record-review":
                raise DocsSmokeError(
                    "fresh package validation returned the wrong package ID"
                )

        if not run_runtime:
            return "skipped"
        payload = _json_command(
            [
                wrapper,
                "check",
                str(single),
                "--workspace",
                str(repo_root),
                "--timeout-seconds",
                "90",
                "--json",
            ],
            cwd=repo_root,
            timeout=300.0,
        )
        phase_states = {
            phase.get("name"): phase.get("state") for phase in payload.get("phases", [])
        }
        if phase_states != {
            "validate": "passed",
            "prepare": "passed",
            "dev": "passed",
            "test": "passed",
        }:
            raise DocsSmokeError(
                f"fresh quick check returned unexpected phases: {phase_states}"
            )
        return "passed"


def parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Validate Copilot developer docs and their executable quick path."
    )
    runtime = parser.add_mutually_exclusive_group()
    runtime.add_argument(
        "--skip-runtime",
        action="store_true",
        help="skip the real app-server check while retaining links/help/init/validate gates",
    )
    runtime.add_argument(
        "--require-runtime",
        action="store_true",
        help="fail unless the checkout Runtime exists and the real app-server check passes",
    )
    return parser.parse_args(argv)


def main(argv: Sequence[str] | None = None) -> int:
    args = parse_args(argv)
    repo_root = Path(__file__).resolve().parents[1]
    codex_binary = repo_root / "codex/codex-rs/target/dev-small/codex"
    if args.require_runtime and not codex_binary.is_file():
        raise DocsSmokeError(
            "--require-runtime needs codex/codex-rs/target/dev-small/codex"
        )
    run_runtime = not args.skip_runtime and codex_binary.is_file()

    link_count = check_markdown_links(governed_markdown(repo_root))
    check_cli_help(repo_root)
    runtime_state = check_fresh_packages(repo_root, run_runtime=run_runtime)
    print(
        "Copilot docs smoke passed: "
        f"{link_count} internal links; CLI help; fresh single/multi/Tool; "
        f"quick check={runtime_state}."
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except DocsSmokeError as error:
        print(f"Copilot docs smoke failed: {error}")
        raise SystemExit(1) from error
