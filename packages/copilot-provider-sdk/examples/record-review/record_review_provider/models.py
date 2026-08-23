"""Bounded domain and Tool-result contracts for the cookbook provider."""

from __future__ import annotations

from typing import Literal

from open_web_codex_provider import ResourceRef
from pydantic import BaseModel, ConfigDict, Field

REVIEW_SCHEMA = "record_review.v1"
ERROR_SCHEMA = "record_review_error.v1"


class ReviewRecord(BaseModel):
    """One immutable provider-owned review result."""

    model_config = ConfigDict(extra="forbid", populate_by_name=True)

    schema_version: Literal["record_review.v1"] = Field(
        default=REVIEW_SCHEMA, alias="schemaVersion"
    )
    status: Literal["accepted", "needs-review"]
    record_id: str = Field(min_length=1, max_length=32, pattern=r"^[a-z][a-z0-9-]*$")
    score: int = Field(ge=0, le=100)
    threshold: int = Field(ge=0, le=100)


class WorkspaceFileRef(BaseModel):
    """Safe user-visible reference without a server-absolute path."""

    model_config = ConfigDict(extra="forbid")

    type: Literal["workspace_file"] = "workspace_file"
    relative_path: str = Field(min_length=1, max_length=256)
    byte_size: int = Field(ge=1, le=4096)


class ReviewPublishResult(BaseModel):
    """Bounded successful publish result returned to the model."""

    model_config = ConfigDict(extra="forbid")

    summary: str = Field(min_length=1, max_length=160)
    review: ReviewRecord
    resource_ref: ResourceRef
    workspace_file: WorkspaceFileRef


class ReviewLoadResult(BaseModel):
    """Bounded successful exact-Resource load result."""

    model_config = ConfigDict(extra="forbid")

    summary: str = Field(min_length=1, max_length=160)
    review: ReviewRecord


PublicErrorCode = Literal[
    "record_id_invalid",
    "score_invalid",
    "threshold_invalid",
    "workspace_scope_invalid",
    "workspace_scope_mismatch",
    "workspace_file_invalid",
    "resource_server_mismatch",
    "resource_uri_mismatch",
    "resource_schema_mismatch",
    "resource_load_invalid",
    "resource_payload_invalid",
    "provider_failure",
]


class ReviewError(BaseModel):
    """Small public failure envelope; internal exception text never crosses MCP."""

    model_config = ConfigDict(extra="forbid", populate_by_name=True)

    schema_version: Literal["record_review_error.v1"] = Field(
        default=ERROR_SCHEMA, alias="schemaVersion"
    )
    status: Literal["error"] = "error"
    code: PublicErrorCode
    retryable: Literal[False] = False
