from __future__ import annotations

from pathlib import Path
from types import SimpleNamespace

import pytest

from meeting_action_review import server


NOTES = """# Launch readiness

- [ ] Prepare launch checklist | owner: Mei | due: 2026-09-01
- [ ] Confirm budget | owner: | due:
"""


def test_inline_review_is_deterministic_and_bounded() -> None:
    review = server.review_action_items(NOTES)

    assert review == {
        "schema": "meeting_action_review.v1",
        "document": "inline",
        "summary": {
            "action_item_count": 2,
            "complete_count": 1,
            "incomplete_count": 1,
        },
        "action_items": [
            {
                "description": "Prepare launch checklist",
                "owner": "Mei",
                "due_date": "2026-09-01",
                "missing_fields": [],
            },
            {
                "description": "Confirm budget",
                "owner": "",
                "due_date": "",
                "missing_fields": ["owner", "due_date"],
            },
        ],
    }


def test_workspace_review_rejects_symlink_and_reads_regular_markdown(tmp_path: Path) -> None:
    notes = tmp_path / "meeting.md"
    notes.write_text(NOTES, encoding="utf-8")
    assert server._review(server._read_workspace_markdown(tmp_path, "meeting.md"), "meeting.md")[
        "summary"
    ]["action_item_count"] == 2

    outside = tmp_path / "outside.md"
    outside.write_text(NOTES, encoding="utf-8")
    (tmp_path / "linked.md").symlink_to(outside)
    with pytest.raises(ValueError, match="workspace_file_invalid"):
        server._read_workspace_markdown(tmp_path, "linked.md")


def test_publish_uses_fixed_artifact_envelope_and_create_new_workspace_write(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    (tmp_path / "meeting.md").write_text(NOTES, encoding="utf-8")
    (tmp_path / "reports").mkdir()

    class FakeProvider:
        def require_workspace(self, _ctx: object) -> Path:
            return tmp_path

        def create_workspace_file(
            self,
            _ctx: object,
            relative_path: str,
            content: bytes,
            *,
            max_bytes: int,
        ) -> SimpleNamespace:
            assert len(content) <= max_bytes
            target = tmp_path / relative_path
            with target.open("xb") as handle:
                handle.write(content)
            return SimpleNamespace(relative_path=relative_path, byte_size=len(content))

    monkeypatch.setattr(server, "_provider", lambda: FakeProvider())
    result = server.publish_action_review("meeting.md", "reports/action-review.md", object())

    assert result.structuredContent == {
        "summary": "Reviewed 2 meeting action items.",
        "artifact": {
            "schema": "meeting_action_review_markdown.v1",
            "displayName": "Meeting action review",
            "mimeType": "text/markdown",
            "workspaceRelativePath": "reports/action-review.md",
            "byteSize": (tmp_path / "reports/action-review.md").stat().st_size,
        },
    }
    report = (tmp_path / "reports/action-review.md").read_text(encoding="utf-8")
    assert report.startswith("# Meeting action review\n\n<!-- meeting_action_review_markdown.v1 -->")
