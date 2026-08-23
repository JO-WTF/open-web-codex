"""Domain service composed from Provider SDK primitives."""

from __future__ import annotations

import json
import re

from mcp.server.fastmcp import Context
from mcp.types import CallToolResult, TextContent
from open_web_codex_provider import (
    McpResourceRuntime,
    ProviderContractError,
    ResourceRef,
    WorkspaceFileError,
    ensure_workspace_directory,
)
from pydantic import ValidationError

from .models import (
    REVIEW_SCHEMA,
    PublicErrorCode,
    ReviewError,
    ReviewLoadResult,
    ReviewPublishResult,
    ReviewRecord,
    WorkspaceFileRef,
)

MAX_REVIEW_FILE_BYTES = 4096
RECORD_ID = re.compile(r"^[a-z][a-z0-9-]{0,31}$")
PUBLIC_PROVIDER_CODES: set[str] = {
    "workspace_scope_invalid",
    "workspace_scope_mismatch",
    "workspace_file_invalid",
    "resource_server_mismatch",
    "resource_uri_mismatch",
    "resource_schema_mismatch",
    "resource_load_invalid",
    "resource_payload_invalid",
}


class ReviewInputError(ValueError):
    """One allowlisted domain validation failure."""

    def __init__(self, code: PublicErrorCode):
        self.code = code
        super().__init__(code)


def _review(record_id: str, score: int, threshold: int) -> ReviewRecord:
    normalized_id = record_id.strip()
    if RECORD_ID.fullmatch(normalized_id) is None:
        raise ReviewInputError("record_id_invalid")
    if isinstance(score, bool) or not isinstance(score, int) or not 0 <= score <= 100:
        raise ReviewInputError("score_invalid")
    if isinstance(threshold, bool) or not isinstance(threshold, int) or not 0 <= threshold <= 100:
        raise ReviewInputError("threshold_invalid")
    return ReviewRecord(
        status="accepted" if score >= threshold else "needs-review",
        record_id=normalized_id,
        score=score,
        threshold=threshold,
    )


def _error_code(error: Exception) -> PublicErrorCode:
    if isinstance(error, ReviewInputError):
        return error.code
    if isinstance(error, ProviderContractError) and error.code in PUBLIC_PROVIDER_CODES:
        return error.code  # type: ignore[return-value]
    if isinstance(error, WorkspaceFileError):
        return "workspace_file_invalid"
    if isinstance(error, ValidationError):
        return "resource_payload_invalid"
    return "provider_failure"


def _error_result(error: Exception) -> CallToolResult:
    failure = ReviewError(code=_error_code(error))
    payload = failure.model_dump(mode="json", by_alias=True)
    return CallToolResult(
        isError=True,
        # MCP 1.27 clients do not retain structuredContent for isError=true.
        # Keep the same bounded typed envelope as canonical JSON content so
        # the failure remains machine-readable across the real transport.
        content=[
            TextContent(
                type="text",
                text=json.dumps(
                    payload,
                    ensure_ascii=False,
                    sort_keys=True,
                    separators=(",", ":"),
                ),
            )
        ],
        structuredContent=payload,
    )


def publish_review(
    runtime: McpResourceRuntime,
    ctx: Context,
    record_id: str,
    score: int,
    threshold: int = 80,
) -> CallToolResult:
    """Create one Workspace file and publish the same exact typed Resource."""

    try:
        review = _review(record_id, score, threshold)
        workspace = runtime.require_workspace(ctx)
        ensure_workspace_directory(workspace, "outputs/record-review")
        summary = f"Reviewed {review.record_id}: {review.status}."
        published = runtime.publish(REVIEW_SCHEMA, review, summary)
        created = runtime.create_workspace_model(
            ctx,
            f"outputs/record-review/{review.record_id}.json",
            review,
            max_bytes=MAX_REVIEW_FILE_BYTES,
        )
        resource = ResourceRef.model_validate(published.structuredContent["resource_ref"])
        result = ReviewPublishResult(
            summary=summary,
            review=review,
            resource_ref=resource,
            workspace_file=WorkspaceFileRef(
                relative_path=created.relative_path,
                byte_size=created.byte_size,
            ),
        )
        published.structuredContent = result.model_dump(mode="json", by_alias=True)
        return published
    except (ProviderContractError, ReviewInputError, ValidationError, WorkspaceFileError) as error:
        return _error_result(error)
    except (OSError, RuntimeError, TypeError, ValueError) as error:
        return _error_result(error)


def load_review(runtime: McpResourceRuntime, resource_ref: ResourceRef) -> CallToolResult:
    """Load one exact typed Resource without scanning or guessing a latest value."""

    try:
        review = runtime.load_model(resource_ref, REVIEW_SCHEMA, ReviewRecord)
        result = ReviewLoadResult(
            summary=f"Loaded exact review for {review.record_id}.",
            review=review,
        )
        return CallToolResult(
            content=[TextContent(type="text", text=result.summary)],
            structuredContent=result.model_dump(mode="json", by_alias=True),
        )
    except (ProviderContractError, ValidationError) as error:
        return _error_result(error)
    except (OSError, RuntimeError, TypeError, ValueError) as error:
        return _error_result(error)
