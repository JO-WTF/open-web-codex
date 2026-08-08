"""Bounded MCP result envelopes for Network Case tools."""

from __future__ import annotations

import json
from dataclasses import dataclass
from typing import Literal

from mcp.types import CallToolResult, ResourceLink, TextContent
from pydantic import BaseModel, ConfigDict, Field

from .case_types import CaseSummary, FacetSummary, ToolOperationSummary


class ToolResultContract(BaseModel):
    model_config = ConfigDict(extra="forbid")


class SafeIssue(ToolResultContract):
    code: str = Field(min_length=1, max_length=128)
    severity: Literal["info", "warning", "error"]
    business_message: str = Field(min_length=1, max_length=400)


class EvidenceSummary(ToolResultContract):
    kind: Literal["source", "field", "mapping_candidate", "metric", "parameter", "artifact"]
    evidence_id: str = Field(min_length=1, max_length=128)
    label: str = Field(min_length=1, max_length=256)
    value: str | int | float | bool | None = None


class NetworkCaseToolEnvelope(ToolResultContract):
    """Domain payload carried by the common bounded platform Tool envelope."""

    schema_version: Literal["platform-tool-result.v1"] = "platform-tool-result.v1"
    case: CaseSummary
    operation: ToolOperationSummary | None = None
    facet_updates: list[FacetSummary] = Field(default_factory=list)
    issues: list[SafeIssue] = Field(default_factory=list)
    issues_truncated: bool = False
    evidence: list[EvidenceSummary] = Field(default_factory=list, max_length=100)
    next_action: str | None = Field(default=None, max_length=128)
    published_artifacts: list[dict[str, object]] = Field(default_factory=list)
    inline_visualization: dict[str, object] | None = None


@dataclass(frozen=True)
class NetworkPlannerLimits:
    tool_argument_max_bytes: int = 16 * 1024
    tool_result_max_bytes: int = 16 * 1024
    issue_summary_limit: int = 20


class ToolEnvelopeBuilder:
    def __init__(self, limits: NetworkPlannerLimits | None = None):
        self.limits = limits or NetworkPlannerLimits()

    def build(
        self,
        *,
        case: CaseSummary,
        summary: str,
        operation: ToolOperationSummary | None = None,
        facet_updates: list[FacetSummary] | None = None,
        issues: list[SafeIssue] | None = None,
        evidence: list[EvidenceSummary] | None = None,
        next_action: str | None = None,
        published_artifacts: list[dict[str, object]] | None = None,
        extra_content: list[ResourceLink] | None = None,
        inline_visualization: dict[str, object] | None = None,
    ) -> CallToolResult:
        bounded_issues = (issues or [])[: self.limits.issue_summary_limit]
        envelope = NetworkCaseToolEnvelope(
            case=case,
            operation=operation,
            facet_updates=facet_updates or [],
            issues=bounded_issues,
            issues_truncated=len(issues or []) > len(bounded_issues),
            evidence=evidence or [],
            next_action=next_action,
            published_artifacts=published_artifacts or [],
            inline_visualization=inline_visualization,
        )
        structured = envelope.model_dump(mode="json", by_alias=True)
        raw = json.dumps(structured, ensure_ascii=True, separators=(",", ":")).encode()
        if len(raw) > self.limits.tool_result_max_bytes:
            raise ValueError("tool_result_too_large")
        return CallToolResult(
            content=[TextContent(type="text", text=summary[:1_000]), *(extra_content or [])],
            structuredContent=structured,
        )
