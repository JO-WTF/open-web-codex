from __future__ import annotations

import os
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from threading import Barrier

import pytest
from open_web_codex_provider import (
    WorkspaceFileError,
    create_workspace_file,
)


def test_create_workspace_file_is_atomic_create_new(tmp_path: Path) -> None:
    output = tmp_path / "outputs"
    output.mkdir()

    created = create_workspace_file(tmp_path, "outputs/report.txt", b"ready")

    assert created.relative_path == "outputs/report.txt"
    assert created.byte_size == 5
    assert (output / "report.txt").read_bytes() == b"ready"
    assert list(output.glob(".open-web-codex-*.tmp")) == []
    with pytest.raises(WorkspaceFileError, match="workspace_file_exists"):
        create_workspace_file(tmp_path, "outputs/report.txt", b"replacement")
    assert (output / "report.txt").read_bytes() == b"ready"


@pytest.mark.parametrize(
    "relative_path",
    ("", ".", "..", "/absolute", "outputs/../escape", "outputs//file", "a\\b"),
)
def test_create_workspace_file_rejects_invalid_relative_paths(
    tmp_path: Path, relative_path: str
) -> None:
    with pytest.raises(WorkspaceFileError, match="workspace_path_invalid"):
        create_workspace_file(tmp_path, relative_path, b"content")


def test_create_workspace_file_rejects_symlink_components_and_targets(
    tmp_path: Path,
) -> None:
    outside = tmp_path / "outside"
    outside.mkdir()
    (tmp_path / "linked").symlink_to(outside, target_is_directory=True)
    (tmp_path / "target-link").symlink_to(outside / "target")

    with pytest.raises(WorkspaceFileError, match="workspace_symlink_rejected"):
        create_workspace_file(tmp_path, "linked/file.txt", b"content")
    with pytest.raises(WorkspaceFileError, match="workspace_symlink_rejected"):
        create_workspace_file(tmp_path, "target-link", b"content")
    assert list(outside.iterdir()) == []


def test_create_workspace_file_requires_existing_directory_parent(tmp_path: Path) -> None:
    with pytest.raises(WorkspaceFileError, match="workspace_parent_missing"):
        create_workspace_file(tmp_path, "missing/file.txt", b"content")
    (tmp_path / "plain-file").write_text("not a directory", encoding="utf-8")
    with pytest.raises(WorkspaceFileError, match="workspace_parent_not_directory"):
        create_workspace_file(tmp_path, "plain-file/output.txt", b"content")


def test_create_workspace_file_enforces_exact_size_bound(tmp_path: Path) -> None:
    assert create_workspace_file(tmp_path, "exact.bin", b"1234", max_bytes=4).byte_size == 4
    with pytest.raises(WorkspaceFileError, match="workspace_file_too_large"):
        create_workspace_file(tmp_path, "large.bin", b"12345", max_bytes=4)
    assert not (tmp_path / "large.bin").exists()


def test_create_workspace_file_cleans_temporary_after_publish_failure(
    tmp_path: Path, monkeypatch
) -> None:
    def fail_link(*_args, **_kwargs):
        raise OSError("simulated publish failure")

    monkeypatch.setattr(os, "link", fail_link)

    with pytest.raises(WorkspaceFileError, match="workspace_write_failed"):
        create_workspace_file(tmp_path, "result.bin", b"content")
    assert not (tmp_path / "result.bin").exists()
    assert list(tmp_path.glob(".open-web-codex-*.tmp")) == []


def test_create_workspace_file_has_one_winner_under_concurrency(tmp_path: Path) -> None:
    barrier = Barrier(2)
    payloads = (b"first", b"second")

    def create(payload: bytes):
        barrier.wait()
        try:
            return create_workspace_file(tmp_path, "winner.bin", payload)
        except WorkspaceFileError as error:
            return error

    with ThreadPoolExecutor(max_workers=2) as executor:
        results = list(executor.map(create, payloads))

    assert sum(not isinstance(result, WorkspaceFileError) for result in results) == 1
    failures = [result for result in results if isinstance(result, WorkspaceFileError)]
    assert [error.code for error in failures] == ["workspace_file_exists"]
    assert (tmp_path / "winner.bin").read_bytes() in payloads
    assert list(tmp_path.glob(".open-web-codex-*.tmp")) == []
