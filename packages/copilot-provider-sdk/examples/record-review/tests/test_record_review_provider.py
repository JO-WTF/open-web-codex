from __future__ import annotations

import json
from pathlib import Path
from types import SimpleNamespace

from open_web_codex_provider import McpResourceRuntime, ResourceRef

from record_review_provider.service import load_review, publish_review

SERVER_NAME = "record_review"
URI_PREFIX = "record-review://resources/"


def _context(workspace: Path) -> SimpleNamespace:
    return SimpleNamespace(
        request_context=SimpleNamespace(
            meta=SimpleNamespace(
                model_extra={"codex/sandbox-state-meta": {"sandboxCwd": workspace.as_uri()}}
            )
        )
    )


def _runtime(tmp_path: Path) -> tuple[McpResourceRuntime, Path]:
    workspace = tmp_path / "workspace"
    profile = tmp_path / "profile"
    workspace.mkdir()
    profile.mkdir()
    return McpResourceRuntime(workspace, profile, SERVER_NAME, URI_PREFIX), workspace


def test_publish_creates_new_workspace_file_and_loadable_resource(tmp_path: Path) -> None:
    runtime, workspace = _runtime(tmp_path)

    result = publish_review(runtime, _context(workspace), "sample", 92, 80)

    assert result.isError is not True
    structured = result.structuredContent
    assert structured["review"] == {
        "schemaVersion": "record_review.v1",
        "status": "accepted",
        "record_id": "sample",
        "score": 92,
        "threshold": 80,
    }
    assert structured["workspace_file"]["relative_path"] == ("outputs/record-review/sample.json")
    saved = json.loads((workspace / "outputs/record-review/sample.json").read_text())
    assert saved == structured["review"]

    ref = ResourceRef.model_validate(structured["resource_ref"])
    loaded = load_review(runtime, ref)
    assert loaded.isError is not True
    assert loaded.structuredContent["review"] == structured["review"]


def test_duplicate_workspace_output_is_a_bounded_typed_error(tmp_path: Path) -> None:
    runtime, workspace = _runtime(tmp_path)
    context = _context(workspace)
    assert publish_review(runtime, context, "sample", 92, 80).isError is not True

    duplicate = publish_review(runtime, context, "sample", 63, 80)

    assert duplicate.isError is True
    assert duplicate.structuredContent == {
        "schemaVersion": "record_review_error.v1",
        "status": "error",
        "code": "workspace_file_invalid",
        "retryable": False,
    }
    assert str(workspace) not in json.dumps(duplicate.structuredContent)


def test_wrong_server_resource_ref_fails_closed(tmp_path: Path) -> None:
    runtime, _workspace = _runtime(tmp_path)
    wrong = ResourceRef(
        server="other_provider",
        uri="record-review://resources/record_review.v1-deadbeef",
        resource_schema="record_review.v1",
    )

    result = load_review(runtime, wrong)

    assert result.isError is True
    assert result.structuredContent["code"] == "resource_server_mismatch"


def test_invalid_input_returns_allowlisted_code(tmp_path: Path) -> None:
    runtime, workspace = _runtime(tmp_path)

    result = publish_review(runtime, _context(workspace), "../sample", 92, 80)

    assert result.isError is True
    assert result.structuredContent["code"] == "record_id_invalid"
    assert not (workspace / "outputs").exists()
