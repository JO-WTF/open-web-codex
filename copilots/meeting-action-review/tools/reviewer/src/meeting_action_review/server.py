"""Deterministic MCP tools for meeting action-item review."""

from __future__ import annotations

import os
import stat
import unicodedata
from pathlib import Path, PurePosixPath
from typing import Annotated, Any

from mcp.server.fastmcp import Context, FastMCP
from mcp.types import CallToolResult, TextContent, ToolAnnotations
from open_web_codex_provider import McpResourceRuntime, bind_runtime
from pydantic import Field

SCHEMA = "meeting_action_review.v1"
REPORT_SCHEMA = "meeting_action_review_markdown.v1"
REPORT_MARKER = "<!-- meeting_action_review_markdown.v1 -->"
DISPLAY_NAME = "Meeting action review"
MAX_NOTES_BYTES = 256 * 1024
MAX_ACTION_ITEMS = 32

READ_ONLY = ToolAnnotations(
    readOnlyHint=True,
    destructiveHint=False,
    idempotentHint=True,
    openWorldHint=False,
)
CREATE_NEW = ToolAnnotations(
    readOnlyHint=False,
    destructiveHint=False,
    idempotentHint=False,
    openWorldHint=False,
)

mcp = FastMCP(
    "Meeting Action Review",
    instructions=(
        "Review explicit Markdown checklist action items. Never infer missing owners, due dates, "
        "or completion. Workspace tools accept only authorized Workspace-relative Markdown paths."
    ),
    json_response=True,
)

_startup_workspace = Path.cwd().resolve()
_profile_home = Path(os.environ.get("CODEX_HOME", _startup_workspace / ".codex")).resolve()
_runtime: McpResourceRuntime | None = None


def _provider() -> McpResourceRuntime:
    global _runtime
    if _runtime is None:
        _runtime = bind_runtime(
            _startup_workspace,
            _profile_home,
            "meeting_action_review",
            "meeting-action-review://resources/",
        )
    return _runtime


def _bounded_text(value: str, field: str, maximum: int) -> str:
    if not isinstance(value, str) or not value.strip() or len(value) > maximum:
        raise ValueError(f"{field}_invalid")
    if any(
        unicodedata.category(character) == "Cc" and character not in "\n\r\t"
        for character in value
    ):
        raise ValueError(f"{field}_invalid")
    return value


def _review(notes: str, document: str) -> dict[str, Any]:
    _bounded_text(notes, "notes", MAX_NOTES_BYTES)
    items: list[dict[str, Any]] = []
    for line in notes.splitlines():
        stripped = line.strip()
        if not stripped.startswith("- [ ] "):
            continue
        if len(items) >= MAX_ACTION_ITEMS:
            raise ValueError("action_item_limit_exceeded")
        fields = [field.strip() for field in stripped[6:].split("|")]
        description = _bounded_text(fields[0], "description", 500)
        metadata: dict[str, str] = {}
        for field in fields[1:]:
            if ":" not in field:
                raise ValueError("action_item_metadata_invalid")
            key, value = field.split(":", 1)
            key = key.strip()
            if key not in ("owner", "due") or key in metadata:
                raise ValueError("action_item_metadata_invalid")
            metadata[key] = value.strip()
        owner = metadata.get("owner", "")
        due_date = metadata.get("due", "")
        if len(owner) > 120 or len(due_date) > 32:
            raise ValueError("action_item_metadata_invalid")
        missing = [name for name, value in (("owner", owner), ("due_date", due_date)) if not value]
        items.append(
            {
                "description": description,
                "owner": owner,
                "due_date": due_date,
                "missing_fields": missing,
            }
        )
    if not items:
        raise ValueError("action_items_missing")
    complete_count = sum(not item["missing_fields"] for item in items)
    return {
        "schema": SCHEMA,
        "document": document,
        "summary": {
            "action_item_count": len(items),
            "complete_count": complete_count,
            "incomplete_count": len(items) - complete_count,
        },
        "action_items": items,
    }


def _relative_parts(relative_path: str) -> tuple[str, ...]:
    if not isinstance(relative_path, str) or not relative_path or len(relative_path) > 1024:
        raise ValueError("workspace_path_invalid")
    if "\\" in relative_path or "\x00" in relative_path:
        raise ValueError("workspace_path_invalid")
    path = PurePosixPath(relative_path)
    if path.is_absolute() or path.suffix.lower() != ".md":
        raise ValueError("workspace_path_invalid")
    parts = path.parts
    if not parts or any(part in ("", ".", "..") for part in parts):
        raise ValueError("workspace_path_invalid")
    return parts


def _read_workspace_markdown(workspace: Path, relative_path: str) -> str:
    parts = _relative_parts(relative_path)
    directory_fd = os.open(workspace, os.O_RDONLY | os.O_DIRECTORY)
    try:
        for component in parts[:-1]:
            child_fd = os.open(
                component,
                os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW,
                dir_fd=directory_fd,
            )
            os.close(directory_fd)
            directory_fd = child_fd
        file_fd = os.open(parts[-1], os.O_RDONLY | os.O_NOFOLLOW, dir_fd=directory_fd)
        try:
            metadata = os.fstat(file_fd)
            if not stat.S_ISREG(metadata.st_mode) or metadata.st_size > MAX_NOTES_BYTES:
                raise ValueError("workspace_file_invalid")
            content = bytearray()
            while len(content) <= MAX_NOTES_BYTES:
                chunk = os.read(file_fd, min(64 * 1024, MAX_NOTES_BYTES + 1 - len(content)))
                if not chunk:
                    break
                content.extend(chunk)
            if len(content) > MAX_NOTES_BYTES:
                raise ValueError("workspace_file_invalid")
        finally:
            os.close(file_fd)
    except OSError as error:
        raise ValueError("workspace_file_invalid") from error
    finally:
        os.close(directory_fd)
    try:
        return bytes(content).decode("utf-8")
    except UnicodeDecodeError as error:
        raise ValueError("workspace_file_invalid") from error


@mcp.tool(structured_output=True, annotations=READ_ONLY)
def review_action_items(
    notes: Annotated[str, Field(min_length=1, max_length=MAX_NOTES_BYTES)],
) -> dict[str, Any]:
    """Review explicit checklist action items supplied as bounded Markdown text."""

    return _review(notes, "inline")


@mcp.tool(structured_output=True, annotations=READ_ONLY)
def review_meeting_notes(
    workspace_relative_path: Annotated[str, Field(min_length=1, max_length=1024)],
    ctx: Context,
) -> dict[str, Any]:
    """Review one authorized Workspace Markdown meeting-notes file."""

    workspace = _provider().require_workspace(ctx)
    notes = _read_workspace_markdown(workspace, workspace_relative_path)
    return _review(notes, workspace_relative_path)


def _report(review: dict[str, Any]) -> str:
    summary = review["summary"]
    lines = [
        "# Meeting action review",
        "",
        REPORT_MARKER,
        "",
        f"Document: {review['document']}",
        "",
        f"Complete actions: {summary['complete_count']} / {summary['action_item_count']}",
        "",
        "## Action items",
        "",
    ]
    for item in review["action_items"]:
        owner = item["owner"] or "Missing"
        due_date = item["due_date"] or "Missing"
        lines.append(f"- {item['description']} — Owner: {owner}; Due: {due_date}")
    return "\n".join(lines) + "\n"


@mcp.tool(structured_output=True, annotations=CREATE_NEW)
def publish_action_review(
    workspace_relative_path: Annotated[str, Field(min_length=1, max_length=1024)],
    output_relative_path: Annotated[str, Field(min_length=1, max_length=1024)],
    ctx: Context,
) -> CallToolResult:
    """Review Workspace meeting notes and create one new Markdown report."""

    workspace = _provider().require_workspace(ctx)
    review = _review(
        _read_workspace_markdown(workspace, workspace_relative_path),
        workspace_relative_path,
    )
    content = _report(review).encode("utf-8")
    created = _provider().create_workspace_file(
        ctx,
        output_relative_path,
        content,
        max_bytes=MAX_NOTES_BYTES,
    )
    summary = f"Reviewed {review['summary']['action_item_count']} meeting action items."
    return CallToolResult(
        content=[TextContent(type="text", text=summary)],
        structuredContent={
            "summary": summary,
            "artifact": {
                "schema": REPORT_SCHEMA,
                "displayName": DISPLAY_NAME,
                "mimeType": "text/markdown",
                "workspaceRelativePath": created.relative_path,
                "byteSize": created.byte_size,
            },
        },
    )


def main() -> None:
    mcp.run()


if __name__ == "__main__":
    main()
