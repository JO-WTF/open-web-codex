"""FastMCP entry point for supply-chain network planning."""

from __future__ import annotations

import argparse
import asyncio
import base64
import hashlib
import json
import os
import time
import urllib.error
import urllib.parse
import urllib.request
from datetime import UTC, datetime
from decimal import Decimal
from pathlib import Path
from typing import Annotated, Literal
from uuid import UUID

from mcp.server.fastmcp import Context, FastMCP
from mcp.server.stdio import stdio_server
from mcp.types import (
    CallToolResult,
    EmbeddedResource,
    ResourceLink,
    TextContent,
    TextResourceContents,
    ToolAnnotations,
)
from pydantic import Field, ValidationError

from .analysis_service import NetworkAnalysisService
from .case_models import ArtifactRef, NetworkCase, NormalizedNetworkInput
from .case_repository import CaseRepository, CaseRepositoryError
from .case_tools import EvidenceSummary, SafeIssue, ToolEnvelopeBuilder
from .case_types import AnalysisKind, CaseState
from .core import (
    compare_scenarios,
    create_route_matrix,
    create_snapshot,
    evaluate_current_assignment,
    evaluate_optimized_network,
    solve_location_candidates,
)
from .decision_core import (
    build_risk_register,
)
from .decision_core import (
    evaluate_financial_case as calculate_financial_case,
)
from .map_service import (
    NetworkComparisonMapBundle,
    NetworkMapService,
    build_network_comparison_map_bundle,
    build_network_distribution_geojson,
    build_network_distribution_map_card_handoff,
)
from .mapping_service import CaseMappingService
from .matrix import build_cost_matrix as _build_composable_cost_matrix
from .matrix import build_provided_route_matrix as _build_provided_route_matrix
from .matrix import build_route_matrix_with_reuse
from .matrix import plan_route_matrix as _plan_composable_route_matrix
from .matrix import register_navigation_route_matrix as _register_composable_navigation_matrix
from .matrix import validate_route_matrix as _validate_route_matrix_model
from .matrix_models import CostCalculationPolicy, CostMatrix, RouteMatrixRow
from .matrix_models import RouteMatrix as ComposableRouteMatrix
from .matrix_service import CaseMatrixService
from .mcp_contracts import MapResourceRef, ResourceRef
from .mcp_resources import McpResourceContractError, McpResourceRuntime, bind_runtime
from .models import (
    ComparisonToolResult,
    CurrentCoverageResult,
    CurrentCoverageToolResult,
    FacilityLocationSolution,
    FacilityLocationToolResult,
    FinancialEvaluation,
    FinancialEvaluationToolResult,
    NetworkBaselineResourceToolResult,
    NetworkFinalArtifactDescriptor,
    NetworkFinalArtifactToolResult,
    NetworkInput,
    NetworkMapRenderToolResult,
    NetworkMapToolResult,
    NetworkPlanningReportToolResult,
    NetworkScenarioResult,
    NetworkSnapshot,
    NetworkSnapshotPreparationToolResult,
    PlanningDataset,
    PreparedNetworkResource,
    ResourceToolResult,
    RiskItem,
    RiskRegister,
    RiskRegisterToolResult,
    RouteEntry,
    ScenarioComparison,
    UncoveredCitySummary,
    ValidationResult,
)
from .models import RouteMatrix as LegacyRouteMatrix
from .network_data import SourceInventoryService
from .network_models import (
    DemandCityRecord,
    NormalizedInputBatch,
    WarehouseRecord,
)
from .normalization import NormalizationService
from .optimization_models import (
    AssignmentComparison,
    AssignmentResult,
    BaselineResult,
    CostSummary,
    CoverageMetricSummary,
    PMedianSolution,
    ScenarioResult,
    ScenarioSpec,
    ServiceConstrainedSolution,
    ServiceCoverageConstraint,
    ServiceMetric,
)
from .readiness import ReadinessEvaluator
from .report_service import (
    NetworkPlanningReportBundle,
    NetworkReportService,
    build_network_planning_report_bundle,
)
from .requirements import RequirementRequest, RequirementService
from .resource_store import (
    PublishedResource,
    ResourceStore,
)
from .resource_store import (
    resource_ref as _resource_ref,
)
from .scenario_service import FacilityLocationService, NetworkScenarioService
from .solver import (
    SolverUnavailable,
    compare_assignments,
    coverage_metrics,
    enumerate_p_median,
    service_metrics,
    solve_assignment,
    solve_current_assignment,
    summarize_assignment_cost,
)
from .workspace_files import MAX_WORKSPACE_FILE_BYTES
from .workspace_intake import read_json_document

MAX_SOURCE_BYTES = 20 * 1024 * 1024
MAX_PROFILE_GOAL_CHARS = 8_000
SANDBOX_STATE_META_CAPABILITY = "codex/sandbox-state-meta"
MCP_SERVER_NAME = "supply_chain"
RESOURCE_URI_PREFIX = "supply-chain://resources/"

CONTENT_ADDRESSED_RESOURCE_TOOL = ToolAnnotations(
    readOnlyHint=False,
    destructiveHint=False,
    idempotentHint=True,
    openWorldHint=False,
)
BOUNDED_LOCAL_COMPUTE_TOOL = ToolAnnotations(
    readOnlyHint=False,
    destructiveHint=False,
    idempotentHint=False,
    openWorldHint=False,
)
FINAL_WORKSPACE_DELIVERY_TOOL = ToolAnnotations(
    readOnlyHint=False,
    destructiveHint=False,
    idempotentHint=False,
    openWorldHint=True,
)


def resource_ref(published: PublishedResource) -> ResourceRef:
    return _resource_ref(published, MCP_SERVER_NAME)


def _bounded_id_summary(values: list[str], *, limit: int = 4) -> str:
    unique = list(dict.fromkeys(values))
    shown: list[str] = []
    rendered_chars = 0
    for value in unique[:limit]:
        bounded = value if len(value) <= 64 else f"{value[:61]}..."
        separator_chars = 2 if shown else 0
        if shown and rendered_chars + separator_chars + len(bounded) > 160:
            break
        shown.append(bounded)
        rendered_chars += separator_chars + len(bounded)
    if not shown:
        return "none"
    remaining = len(unique) - len(shown)
    suffix = f", +{remaining} more" if remaining else ""
    return f"{', '.join(shown)}{suffix}"


def _service_metric_summary(metrics: list[ServiceMetric], *, limit: int = 8) -> str:
    shown = [
        f"{metric.target_hours:g}h demand-weighted={metric.coverage_rate:.1%}"
        for metric in metrics[:limit]
    ]
    if not shown:
        return "none"
    remaining = len(metrics) - len(shown)
    suffix = f", +{remaining} more" if remaining else ""
    return f"{', '.join(shown)}{suffix}"


def _baseline_coverage_projection(
    assignment: AssignmentResult,
    demand_cities: list[DemandCityRecord],
    warehouses: list[WarehouseRecord],
    targets: list[float],
) -> tuple[list[CoverageMetricSummary], float, list[UncoveredCitySummary]]:
    coverage = coverage_metrics(assignment, targets)
    detail_target = max(targets)
    demand_by_id = {item.city_id: item for item in demand_cities}
    warehouse_by_id = {item.warehouse_id: item for item in warehouses}
    uncovered: list[UncoveredCitySummary] = []
    for row in sorted(assignment.rows, key=lambda item: item.demand_city_id):
        if row.duration_hours is not None and row.duration_hours <= detail_target:
            continue
        demand = demand_by_id[row.demand_city_id]
        warehouse = warehouse_by_id.get(row.warehouse_id) if row.warehouse_id else None
        uncovered.append(
            UncoveredCitySummary(
                demand_city_id=row.demand_city_id,
                demand_city_name=demand.city_name,
                warehouse_id=row.warehouse_id,
                warehouse_name=warehouse.warehouse_name if warehouse else None,
                duration_hours=row.duration_hours,
                demand_quantity=row.demand_quantity,
            )
        )
    return coverage, detail_target, uncovered


def _coverage_metric_summary(
    metrics: list[CoverageMetricSummary], *, limit: int = 8
) -> str:
    shown = [
        f"{metric.target_hours:g}h city-count={metric.city_coverage_rate:.1%} "
        f"({metric.covered_city_count}/{metric.total_city_count}), "
        f"demand-weighted={metric.demand_weighted_coverage_rate:.1%}"
        for metric in metrics[:limit]
    ]
    if not shown:
        return "none"
    remaining = len(metrics) - len(shown)
    suffix = f", +{remaining} more" if remaining else ""
    return f"{'; '.join(shown)}{suffix}"


def _cost_metric_summary(cost: CostSummary | None) -> str:
    if cost is None:
        return "unavailable"
    state = "complete" if cost.complete else "incomplete"
    return f"{cost.total:.2f} {cost.currency} ({state})"


mcp = FastMCP(
    "Supply Chain Network Planner",
    instructions=(
        "仓网工具以平台发布的 Resource 引用作为输入和输出，不把原始数据、矩阵行或"
        "内部 Work State 标识复制到 Agent 消息。球面距离必须明确提供绕路系数和"
        "平均速度；导航必须先报告路线数量和费用风险并获得用户许可。没有当前覆盖"
        "关系时，结果必须标记为现有仓优化基线，不能称为当前实际方案。"
    ),
    json_response=True,
)

_workspace_root = Path.cwd().resolve()
_data_root = Path(os.environ.get("SUPPLY_CHAIN_DATA_ROOT", _workspace_root)).resolve()
_profile_state_root = Path(os.environ.get("CODEX_HOME", _workspace_root / ".codex")).resolve()
_mcp_resource_runtime: McpResourceRuntime | None = None
_case_store: CaseRepository | None = None
_CONTRACT_PATH = (
    Path(__file__).resolve().parents[1] / "contracts" / "warehouse-network-planning-1.0.0.json"
)


def _store() -> ResourceStore:
    return _runtime().store


def _runtime() -> McpResourceRuntime:
    global _mcp_resource_runtime
    if _mcp_resource_runtime is None:
        _mcp_resource_runtime = bind_runtime(
            _workspace_root,
            _profile_state_root,
            MCP_SERVER_NAME,
            RESOURCE_URI_PREFIX,
        )
    return _mcp_resource_runtime


def _cases() -> CaseRepository:
    global _case_store
    if _case_store is None:
        _case_store = CaseRepository.from_profile(_profile_state_root)
    return _case_store


def _workspace(ctx: Context) -> Path:
    return _runtime().require_workspace(ctx)


def _case_error_result(error: CaseRepositoryError) -> CallToolResult:
    return CallToolResult(
        isError=True,
        content=[TextContent(type="text", text=str(error))],
        structuredContent={
            "schemaVersion": "network-case-tool-error.v1",
            "code": error.code,
            "message": str(error),
        },
    )


def _artifact_ref(published: PublishedResource, server_name: str = MCP_SERVER_NAME) -> ArtifactRef:
    if server_name != MCP_SERVER_NAME:
        raise ValueError("unsupported_artifact_server")
    store = _store()
    content_sha256 = hashlib.sha256(store.read(published.resource_id).encode("utf-8")).hexdigest()
    return ArtifactRef(
        server_name=server_name,
        resource_schema=published.schema,
        resource_name=published.resource_id,
        content_sha256=content_sha256,
    )


def _load_artifact(ref: ArtifactRef) -> dict[str, object]:
    if ref.server_name != MCP_SERVER_NAME:
        raise ValueError("unsupported_artifact_server")
    raw = _store().read(ref.resource_name)
    digest = hashlib.sha256(raw.encode("utf-8")).hexdigest()
    if digest != ref.content_sha256:
        raise ValueError("artifact_content_hash_mismatch")
    payload = json.loads(raw)
    if (
        not isinstance(payload, dict)
        or payload.get("schemaVersion", payload.get("schema_version")) != ref.resource_schema
    ):
        raise ValueError("artifact_schema_mismatch")
    return payload


def _case_from_input_ref(ref: ArtifactRef | ResourceRef) -> NetworkCase:
    if isinstance(ref, ResourceRef):
        if ref.resource_schema != "normalized_network_input.v1":
            raise ValueError("network_case_requires_normalized_network_input")
        raw = _store().read(ref.uri.rsplit("/", maxsplit=1)[-1])
        payload = json.loads(raw)
        artifact_ref = ArtifactRef(
            server_name=ref.server,
            resource_schema=ref.resource_schema,
            resource_name=ref.uri.rsplit("/", maxsplit=1)[-1],
            content_sha256=hashlib.sha256(raw.encode("utf-8")).hexdigest(),
        )
    else:
        payload = _load_artifact(ref)
        artifact_ref = ref
    if payload.get("schemaVersion") != "normalized_network_input.v1":
        raise ValueError("network_case_requires_normalized_network_input")
    normalized = NormalizedNetworkInput.model_validate(
        {
            key: value
            for key, value in payload.items()
            if key not in {"schemaVersion", "contract", "taskEvidence"}
        }
    )
    return NetworkCase(
        country_code=normalized.country_code,
        input_ref=artifact_ref,
        demand=normalized.demand,
        warehouses=normalized.existing_warehouses + normalized.candidate_warehouses,
        current_assignments=normalized.current_assignments,
        route_quotes=normalized.route_quotes,
    )


def _resource_name(value: object) -> str:
    """Return the exact opaque Resource name carried by a ResourceRef."""
    if isinstance(value, str):
        return value.rsplit("/", maxsplit=1)[-1]
    if isinstance(value, dict):
        name = value.get("resource_name") or value.get("resourceName")
        if isinstance(name, str) and name:
            return name
        uri = value.get("uri")
        if isinstance(uri, str) and uri:
            return uri.rsplit("/", maxsplit=1)[-1]
    raise ValueError("analysis_authorization_required")


def _require_analysis_gate(
    tool_name: str,
    execution_snapshot_id: str | None,
    planning_dataset_ref: object,
    binding_fingerprint: str | None,
) -> None:
    """Authorize an analysis tool against the current Platform snapshot.

    The MCP process never receives database credentials.  It proves possession
    of the short-lived Profile Host key and sends only opaque snapshot/resource
    references to the platform gate.
    """
    gate_url = os.environ.get("OPEN_WEB_CODEX_ANALYSIS_GATE_URL", "").strip()
    gate_key = os.environ.get("OPEN_WEB_CODEX_ANALYSIS_GATE_KEY", "").strip()
    if not gate_url or not gate_key or not execution_snapshot_id or not binding_fingerprint:
        raise ValueError("analysis_authorization_required")
    try:
        key = base64.urlsafe_b64decode(gate_key + "=" * (-len(gate_key) % 4))
    except ValueError as exc:
        raise ValueError("analysis_authorization_required") from exc
    body = json.dumps(
        {
            "executionSnapshotId": execution_snapshot_id,
            "toolName": tool_name,
            "planningDatasetRef": _resource_name(planning_dataset_ref),
            "bindingFingerprint": binding_fingerprint,
        },
        separators=(",", ":"),
        sort_keys=True,
    ).encode("utf-8")
    timestamp = str(int(time.time()))
    parsed = urllib.parse.urlparse(gate_url)
    path = parsed.path or "/"
    message = "\n".join([timestamp, "POST", path, hashlib.sha256(body).hexdigest()]).encode("utf-8")
    import hmac

    signature = (
        base64.urlsafe_b64encode(hmac.new(key, message, hashlib.sha256).digest())
        .decode("ascii")
        .rstrip("=")
    )
    request = urllib.request.Request(
        gate_url,
        data=body,
        method="POST",
        headers={
            "Content-Type": "application/json",
            "X-Open-Web-Codex-Analysis-Timestamp": timestamp,
            "X-Open-Web-Codex-Analysis-Signature": signature,
        },
    )
    try:
        with urllib.request.urlopen(request, timeout=5) as response:
            payload = json.load(response)
    except (urllib.error.URLError, urllib.error.HTTPError, TimeoutError, OSError) as exc:
        raise ValueError("analysis_authorization_required") from exc
    if payload.get("authorized") is not True:
        raise ValueError("analysis_authorization_required")


def _validate_evidence_ref(ref: ResourceRef) -> None:
    if ref.server != MCP_SERVER_NAME:
        raise ValueError(f"unsupported evidence resource_ref.server {ref.server!r}")
    payload = _store().load_uri(ref.uri)
    if payload.get("schema_version") != ref.resource_schema:
        raise ValueError(
            f"evidence resource_ref schema {ref.resource_schema!r} does not match "
            f"resource schema {payload.get('schema_version')!r}"
        )


@mcp.resource(
    "supply-chain://resources/{resource_id}",
    name="supply_chain_resource",
    title="Supply-chain planning resource",
    mime_type="application/json",
)
def read_supply_chain_resource(resource_id: str) -> str:
    """Read an immutable JSON planning resource created by this MCP server."""
    return _store().read(resource_id)


def _resource_link(
    published: PublishedResource,
    description: str,
) -> ResourceLink:
    return ResourceLink(
        type="resource_link",
        name=published.resource_id,
        title=published.schema,
        uri=published.uri,
        description=description,
        mimeType="application/json",
        size=published.size,
    )


def _resource_call_result(
    published: PublishedResource,
    *,
    summary: str,
    structured: dict[str, object],
) -> CallToolResult:
    content: list[object] = [
        TextContent(type="text", text=summary),
        _resource_link(published, summary),
    ]
    if (
        published.schema
        in {
            "data_requirement_profile.v2",
            "source_profile.v1",
            "mapping_proposal.v1",
            "input_gap.v1",
            "planning-dataset.v2",
            "analysis_readiness_review.v1",
        }
        and published.size > 128 * 1024
    ):
        payload = _store().load_uri(published.uri)
        encoded = json.dumps(
            payload, ensure_ascii=False, sort_keys=True, separators=(",", ":")
        ).encode()
        envelope = {
            "schemaVersion": payload.get("schemaVersion"),
            "contract": payload.get("contract"),
            "taskEvidence": payload.get("taskEvidence"),
            "envelopeSchemaVersion": "intake_evidence_envelope.v1",
            "resourceUri": published.uri,
            "resourceSchema": published.schema,
            "resourceContentSha256": hashlib.sha256(encoded).hexdigest(),
        }
        for key in (
            "taskGoal",
            "problemType",
            "entities",
            "requiredEntities",
            "parameters",
            "outputs",
            "conditionalRequirements",
            "assumptions",
            "exclusions",
            "inputRequest",
            "sources",
            "candidates",
            "gaps",
            "ready",
            "summary",
            "checks",
            "plannedAnalysis",
            "limitations",
            "normalization_status",
            "data_quality",
            "source_summary",
            "normalization_statistics",
            "rejections",
        ):
            if key in payload:
                envelope[key] = payload[key]
        envelope_text = json.dumps(
            envelope, ensure_ascii=False, sort_keys=True, separators=(",", ":")
        )
        if len(envelope_text.encode()) > 128 * 1024:
            for key in (
                "sources",
                "candidates",
                "entities",
                "requiredEntities",
                "checks",
                "limitations",
            ):
                if isinstance(envelope.get(key), list):
                    envelope[key] = envelope[key][:128]
            envelope_text = json.dumps(
                envelope, ensure_ascii=False, sort_keys=True, separators=(",", ":")
            )
        if len(envelope_text.encode()) > 128 * 1024:
            raise ValueError("intake_evidence_envelope_exceeds_platform_limit")
        content.append(
            EmbeddedResource(
                type="resource",
                resource=TextResourceContents(
                    uri=published.uri,
                    mimeType="application/json",
                    text=envelope_text,
                ),
            )
        )
    return CallToolResult(
        content=content,
        structuredContent=structured,
    )


def _load_ref(ref: ResourceRef, model_type):
    if ref.server != MCP_SERVER_NAME:
        raise ValueError(f"resource_ref.server must be {MCP_SERVER_NAME}")
    payload = _store().load(ref)
    value = model_type.model_validate(payload)
    actual_schema = getattr(value, "schema_version", None)
    if actual_schema != ref.resource_schema:
        raise ValueError(
            f"resource_ref schema {ref.resource_schema!r} does not match resource schema "
            f"{actual_schema!r}"
        )
    return value


def _ref_for_resource(resource_name: str) -> ResourceRef:
    """Resolve a server-owned opaque resource name without accepting a URI."""
    if not resource_name or resource_name != Path(resource_name).name:
        raise ValueError("resource_name must be a single opaque Resource identifier")
    payload = _store().load_uri(f"supply-chain://resources/{resource_name}")
    schema = payload.get("schema_version")
    if not isinstance(schema, str) or not schema:
        raise ValueError("Resource is missing schema_version")
    return ResourceRef(
        server=MCP_SERVER_NAME,
        resource_schema=schema,
        uri=f"supply-chain://resources/{resource_name}",
    )


def _load_planning_dataset(ref: ResourceRef) -> PlanningDataset:
    if ref.server != MCP_SERVER_NAME or ref.resource_schema != "planning-dataset.v2":
        raise ValueError("planning_dataset_ref must identify supply_chain planning-dataset.v2")
    payload = _store().load_uri(ref.uri)
    if payload.get("schemaVersion") != "planning-dataset.v2":
        raise ValueError("planning dataset is missing current provenance")
    contract = payload.get("contract")
    if not isinstance(contract, dict) or (contract.get("contractId"), contract.get("version")) != (
        "warehouse-network-planning",
        "1.0.0",
    ):
        raise ValueError("planning dataset does not use the current requirement contract")
    normalization = payload.get("normalization")
    if not isinstance(normalization, dict):
        raise ValueError("planning dataset normalization provenance is missing")
    if (
        not isinstance(normalization.get("profile"), dict)
        or normalization["profile"].get("schemaVersion") != "data_requirement_profile.v2"
    ):
        raise ValueError("planning dataset requirement profile is missing")
    for key in ("mapping", "parameters"):
        confirmation = normalization.get(key)
        if not isinstance(confirmation, dict) or confirmation.get("confirmed") is not True:
            raise ValueError(f"planning dataset {key} confirmation is missing")
    if payload.get("dataClassification") not in {"workspace_data", "synthetic_demo"}:
        raise ValueError("planning dataset classification is missing")
    dataset = PlanningDataset.model_validate(payload)
    if dataset.data_quality.errors:
        raise ValueError(
            "planning dataset has blocking data-quality errors: "
            + "; ".join(dataset.data_quality.errors[:5])
        )
    return dataset


def _load_contract() -> dict[str, object]:
    """Load the versioned domain contract from this capability package.

    The Platform deliberately does not embed or interpret this file.  Network
    Agent owns the contract and publishes the task-specific projection.
    """
    payload = json.loads(_CONTRACT_PATH.read_text(encoding="utf-8"))
    if (
        payload.get("contractId") != "warehouse-network-planning"
        or payload.get("version") != "1.0.0"
    ):
        raise ValueError("warehouse_network_requirement_contract_unavailable")
    return payload


def _intake_envelope(
    schema: str,
    payload: dict[str, object],
    *,
    profile_hash: str | None = None,
    source_hash: str | None = None,
    mapping_hash: str | None = None,
    parameter_hash: str | None = None,
) -> dict[str, object]:
    contract = _load_contract()
    return {
        "schemaVersion": schema,
        "contract": {
            "id": contract["contractId"],
            "version": contract["version"],
            "contentHash": contract["contentSha256"],
        },
        "taskEvidence": {
            "profileHash": profile_hash,
            "sourceSnapshotHash": source_hash,
            "mappingHash": mapping_hash,
            "parameterHash": parameter_hash,
        },
        **payload,
    }


def _canonical_hash(value: object) -> str:
    return hashlib.sha256(
        json.dumps(value, sort_keys=True, separators=(",", ":"), default=str).encode()
    ).hexdigest()


def _normalize_profile_goal(goal: str) -> str:
    normalized = goal.strip()
    if not normalized or len(normalized) > MAX_PROFILE_GOAL_CHARS:
        raise ValueError(
            f"goal must contain 1-{MAX_PROFILE_GOAL_CHARS} characters; "
            "pass a concise business objective, not the complete data Profile"
        )
    return normalized


def publish_data_requirement_profile(
    goal: Annotated[
        str,
        Field(
            min_length=1,
            max_length=MAX_PROFILE_GOAL_CHARS,
            description=(
                "Short business objective and user-specific requirements only. "
                "Do not paste the complete Profile, entity fields, validation rules, "
                "keys, row counts or file format; those come from the reviewed contract."
            ),
        ),
    ],
    requested_outputs: list[str] | None = None,
    analysis_mode: Literal[
        "existing_footprint_coverage", "candidate_warehouse_optimization"
    ] = "candidate_warehouse_optimization",
) -> Annotated[CallToolResult, ResourceToolResult]:
    """Create the complete Profile for the network question.

    ``goal`` is only the business objective. The reviewed contract supplies the
    complete entities, fields, validation rules, keys, row counts, parameters
    and default output definitions. Do not duplicate those details in ``goal``.
    Use ``requested_outputs`` and ``analysis_mode`` for their typed options.
    """
    goal = _normalize_profile_goal(goal)
    contract = _load_contract()
    outputs = requested_outputs or ["report", "map"]
    if not outputs or any(not isinstance(item, str) or not item.strip() for item in outputs):
        raise ValueError("requested_outputs must contain named outputs")
    profile_payload = _intake_envelope(
        "data_requirement_profile.v2",
        {
            "taskGoal": goal,
            "problemType": analysis_mode,
            "entities": contract["requiredEntities"],
            "requiredEntities": contract["requiredEntities"],
            "parameters": contract["businessParameters"],
            "outputs": outputs,
            # The platform uses this bounded marker to distinguish a published
            # requirement from an unmaterialized Resource. It does not create
            # a user prompt; the requirement itself is already complete.
            "inputRequest": {"kind": "data_requirement", "status": "published"},
            "conditionalRequirements": [
                "Route facts are required for quoted/navigation routes; otherwise "
                "coordinates and confirmed estimation parameters are required.",
                "Candidate warehouse optimization requires candidate facilities, "
                "capacity and fixed/opening costs.",
            ],
            "assumptions": [
                "Only files and fields accepted through the confirmed mapping will enter the planning dataset.",
                "No tutorial defaults are applied.",
            ],
            "exclusions": [
                "real-time navigation",
                "live carrier pricing",
                "unbounded source reads",
            ],
        },
    )
    published = _store().publish("data_requirement_profile.v2", profile_payload)
    summary = "Published the complete candidate-warehouse data requirement profile; data preparation can continue automatically."
    structured = ResourceToolResult(
        summary=summary,
        resource_name=published.resource_id,
        resource_ref=resource_ref(published),
    ).model_dump(mode="json")
    return _resource_call_result(published, summary=summary, structured=structured)


def _load_planner_payload(ref: ResourceRef) -> dict[str, object]:
    if ref.server != MCP_SERVER_NAME:
        raise ValueError("profile_ref must identify a supply_chain Resource")
    payload = _store().load(ref)
    if (
        payload.get("schema_version") not in {ref.resource_schema, None}
        and payload.get("schemaVersion") != ref.resource_schema
    ):
        raise ValueError("profile Resource schema does not match its reference")
    return payload


def _load_data_payload(ref: ResourceRef, schema: str) -> dict[str, object]:
    if ref.server != MCP_SERVER_NAME or ref.resource_schema != schema:
        raise ValueError(f"expected supply_chain {schema} Resource")
    return _store().load_uri(ref.uri)


def publish_input_gap(
    profile_ref: ResourceRef,
    source_profile_ref: ResourceRef | None = None,
    mapping_proposal_ref: ResourceRef | None = None,
    parameter_answers: dict[str, object] | None = None,
    mapping_confirmation: dict[str, object] | None = None,
) -> Annotated[CallToolResult, ResourceToolResult]:
    """Publish the current minimal data/confirmation/parameter gap."""
    profile = _load_planner_payload(profile_ref)
    source = (
        _load_data_payload(source_profile_ref, "source_profile.v1")
        if source_profile_ref is not None
        else None
    )
    mapping = (
        _load_data_payload(mapping_proposal_ref, "mapping_proposal.v1")
        if mapping_proposal_ref is not None
        else None
    )
    answers = parameter_answers or {}
    gaps: list[dict[str, object]] = []
    if source is None or not source.get("sources"):
        gaps.append(
            {
                "code": "provide_data",
                "message": "需要 Workspace 中可识别的 Excel、CSV 或 JSON 数据。",
            }
        )
    if source is not None and (
        mapping is None or (mapping_confirmation or {}).get("confirmed") is not True
    ):
        gaps.append(
            {"code": "confirm_mapping", "message": "需要 Data Agent 生成并由用户整体确认字段映射。"}
        )
    required_parameters = [
        item.get("name")
        for item in profile.get("parameters", [])
        if isinstance(item, dict) and item.get("required")
    ]
    missing_parameters = [key for key in required_parameters if key not in answers]
    if missing_parameters:
        gaps.append(
            {
                "code": "answer_parameters",
                "parameters": missing_parameters,
                "message": "请补充带单位、来源和范围的业务参数。",
            }
        )
    request_kind = next(
        (
            kind
            for kind in ("provide_data", "confirm_mapping", "answer_parameters")
            if any(item["code"] == kind for item in gaps)
        ),
        None,
    )
    payload = _intake_envelope(
        "input_gap.v1",
        {
            "gaps": gaps,
            "ready": not gaps,
            "inputRequest": (
                {"kind": request_kind, "prompt": "请补齐以下最小缺口后继续。"}
                if request_kind
                else None
            ),
        },
        profile_hash=_canonical_hash(profile),
        source_hash=_canonical_hash(source) if source is not None else None,
        mapping_hash=_canonical_hash(mapping) if mapping is not None else None,
        parameter_hash=_canonical_hash(answers),
    )
    published = _store().publish("input_gap.v1", payload)
    summary = "Input gap is clear." if gaps else "No blocking input gap remains."
    structured = ResourceToolResult(
        summary=summary, resource_name=published.resource_id, resource_ref=resource_ref(published)
    ).model_dump(mode="json")
    return _resource_call_result(published, summary=summary, structured=structured)


def publish_analysis_readiness_review(
    profile_ref: ResourceRef,
    source_profile_ref: ResourceRef,
    mapping_proposal_ref: ResourceRef,
    planning_dataset_ref: ResourceRef,
    parameter_answers: dict[str, object],
    mapping_confirmation: dict[str, object] | None = None,
) -> Annotated[CallToolResult, ResourceToolResult]:
    """Publish the final checklist only after the strict Dataset validates."""
    profile = _load_planner_payload(profile_ref)
    source = _load_data_payload(source_profile_ref, "source_profile.v1")
    mapping = _load_data_payload(mapping_proposal_ref, "mapping_proposal.v1")
    dataset = _load_planning_dataset(planning_dataset_ref)
    mapping_is_confirmed = mapping.get("confirmed") is True or (
        mapping_confirmation is not None and mapping_confirmation.get("confirmed") is True
    )
    if not mapping_is_confirmed:
        raise ValueError("mapping_confirmation_required")
    if not parameter_answers.get("confirmed"):
        raise ValueError("parameter_confirmation_required")
    if dataset.data_quality.errors:
        raise ValueError("planning_dataset_not_ready")
    review = _intake_envelope(
        "analysis_readiness_review.v1",
        {
            "ready": True,
            "summary": {
                "sourceCount": len(source.get("sources", [])),
                "demandCount": len(dataset.network_input.demand_points),
                "facilityCount": len(dataset.network_input.facilities),
                "routeCount": len(dataset.route_entries),
                "planningPeriod": dataset.network_input.planning_period,
                "currency": dataset.network_input.currency,
            },
            "checks": [
                "planning-dataset.v2 schema and provenance validated",
                "demand, facility, assignment and route relationships validated",
                "mapping and parameters confirmed by the user",
            ],
            "plannedAnalysis": [
                "current coverage",
                "candidate warehouse optimization",
                "scenario comparison",
                "report",
                "map",
            ],
            "limitations": dataset.assumptions,
            "inputRequest": {
                "kind": "confirm_analysis",
                "prompt": "确认后，以上数据、映射和参数将被锁定为本次分析快照。",
            },
        },
        profile_hash=_canonical_hash(profile),
        source_hash=_canonical_hash(source),
        mapping_hash=_canonical_hash(mapping),
        parameter_hash=_canonical_hash(parameter_answers),
    )
    published = _store().publish("analysis_readiness_review.v1", review)
    summary = "Final analysis checklist is ready for explicit user confirmation."
    structured = ResourceToolResult(
        summary=summary, resource_name=published.resource_id, resource_ref=resource_ref(published)
    ).model_dump(mode="json")
    return _resource_call_result(published, summary=summary, structured=structured)


def _load_network_source(source_path: str) -> tuple[NetworkInput, str]:
    candidate = Path(source_path)
    path = candidate.resolve() if candidate.is_absolute() else (_data_root / candidate).resolve()
    if path != _data_root and _data_root not in path.parents:
        raise ValueError("source_path escapes the configured supply-chain data root")
    if path.suffix.lower() != ".json":
        raise ValueError("source_path must point to a JSON file")
    size = path.stat().st_size
    if size > MAX_SOURCE_BYTES:
        raise ValueError(
            f"source file is {size} bytes; maximum supported size is {MAX_SOURCE_BYTES}"
        )
    payload = json.loads(path.read_text(encoding="utf-8"))
    return NetworkInput.model_validate(payload), str(path.relative_to(_data_root))


def prepare_network_snapshot(
    network: NetworkInput | None = None,
    source_path: str | None = None,
) -> Annotated[CallToolResult, ResourceToolResult]:
    """Validate network data and publish an immutable network_snapshot.v1 Resource.

    Provide exactly one input: typed inline `network` data, or a JSON `source_path`
    resolved within the server-configured data root.
    """
    if (network is None) == (source_path is None):
        raise ValueError("provide exactly one of network or source_path")
    source_name = "inline"
    if source_path is not None:
        network, source_name = _load_network_source(source_path)
    assert network is not None
    snapshot = create_snapshot(network, source_name=source_name)
    published = _store().publish(snapshot.schema_version, snapshot)
    summary = (
        f"Prepared snapshot {snapshot.snapshot_id} with {len(snapshot.demand_points)} "
        f"demand points, {len(snapshot.facilities)} facilities, "
        f"{sum(item.demand_units for item in snapshot.demand_points)} demand units, "
        f"currency {snapshot.currency}, and policy "
        f"{snapshot.service_policy.policy_id}."
    )
    structured = ResourceToolResult(
        summary=summary,
        resource_name=published.resource_id,
        resource_ref=resource_ref(published),
    ).model_dump(mode="json")
    return _resource_call_result(published, summary=summary, structured=structured)


def prepare_network_snapshot_from_planning_dataset(
    planning_dataset_ref: ResourceRef,
    execution_snapshot_id: str | None = None,
    binding_fingerprint: str | None = None,
) -> Annotated[CallToolResult, NetworkSnapshotPreparationToolResult]:
    """Project one exact Data Agent planning-dataset.v2 Resource into planner Resources.

    This is the only normal Network Agent entry point for user-provided data.  The
    planner reads the immutable Data Agent Resource, validates its provenance and
    creates a bounded snapshot plus a complete route matrix without copying rows
    through model messages.
    """
    _require_analysis_gate(
        "prepare_network_snapshot_from_planning_dataset",
        execution_snapshot_id,
        planning_dataset_ref.model_dump(mode="json"),
        binding_fingerprint,
    )
    dataset = _load_planning_dataset(planning_dataset_ref)
    snapshot = create_snapshot(
        dataset.network_input,
        source_name=dataset.dataset_id,
    ).model_copy(
        update={
            "source_digest": dataset.source_digest,
            "data_classification": dataset.data_classification,
            "demo_template": dataset.demo_template,
        }
    )
    snapshot_published = _store().publish(snapshot.schema_version, snapshot)
    entries = [RouteEntry.model_validate(route.model_dump()) for route in dataset.route_entries]
    matrix = create_route_matrix(
        snapshot,
        provider=dataset.route_provider,
        method=dataset.route_method,
        entries=entries,
    )
    matrix_published = _store().publish(matrix.schema_version, matrix)
    summary = (
        f"Prepared network snapshot {snapshot.snapshot_id} and route matrix "
        f"{matrix.route_matrix_id} from {dataset.dataset_id}; "
        f"{len(snapshot.demand_points)} demand points, {len(snapshot.facilities)} facilities, "
        f"{len(matrix.entries)} route pairs."
    )
    structured = NetworkSnapshotPreparationToolResult(
        summary=summary,
        snapshot_resource_name=snapshot_published.resource_id,
        snapshot_ref=resource_ref(snapshot_published),
        route_matrix_resource_name=matrix_published.resource_id,
        route_matrix_ref=resource_ref(matrix_published),
    ).model_dump(mode="json")
    return _resource_call_result(
        snapshot_published,
        summary=summary,
        structured=structured,
    )


def register_route_matrix(
    snapshot_ref: ResourceRef,
    provider: str,
    method: Literal["navigation", "quoted", "haversine_estimate"],
    entries: list[RouteEntry],
    planning_dataset_ref: ResourceRef,
    execution_snapshot_id: str | None = None,
    binding_fingerprint: str | None = None,
    require_complete: bool = True,
) -> Annotated[CallToolResult, ResourceToolResult]:
    """Validate and publish route distance/time rows for a prepared snapshot.

    Use `navigation` for provider navigation results, `quoted` for carrier-provided
    lane metrics, and `haversine_estimate` only for explicitly labeled rough estimates.
    Complete matrices are required by default; set `require_complete=false` only for
    explicit diagnostic work, where missing pairs remain visible during evaluation.
    """
    _require_analysis_gate(
        "register_route_matrix",
        execution_snapshot_id,
        planning_dataset_ref.model_dump(mode="json"),
        binding_fingerprint,
    )
    snapshot = _load_ref(snapshot_ref, NetworkSnapshot)
    expected_pairs = {
        (facility.city_id, demand.city_id)
        for facility in snapshot.facilities
        for demand in snapshot.demand_points
    }
    supplied_pairs = {(entry.origin_city_id, entry.destination_city_id) for entry in entries}
    missing_pairs = sorted(expected_pairs - supplied_pairs)
    if require_complete and missing_pairs:
        preview = ", ".join(f"{origin}->{destination}" for origin, destination in missing_pairs[:5])
        suffix = "" if len(missing_pairs) <= 5 else f" and {len(missing_pairs) - 5} more"
        raise ValueError(
            f"route matrix is missing {len(missing_pairs)} required pairs: {preview}{suffix}"
        )
    matrix = create_route_matrix(
        snapshot,
        provider=provider,
        method=method,
        entries=entries,
    )
    published = _store().publish(matrix.schema_version, matrix)
    expected = len(expected_pairs)
    ready = sum(entry.status == "ready" for entry in matrix.entries)
    summary = (
        f"Registered route matrix {matrix.route_matrix_id}: {len(entries)} of {expected} "
        f"city-to-city lane pairs supplied, {ready} ready, method {matrix.method}, "
        f"provider {matrix.provider}."
    )
    structured = ResourceToolResult(
        summary=summary,
        resource_name=published.resource_id,
        resource_ref=resource_ref(published),
    ).model_dump(mode="json")
    return _resource_call_result(published, summary=summary, structured=structured)


def evaluate_current_coverage(
    snapshot_ref: ResourceRef,
    route_matrix_ref: ResourceRef,
    planning_dataset_ref: str,
    execution_snapshot_id: str | None = None,
    binding_fingerprint: str | None = None,
) -> Annotated[CallToolResult, CurrentCoverageToolResult]:
    """Calculate actual current coverage and optimized existing-footprint coverage.

    Actual coverage preserves each demand point's `current_facility_id`. Optimized
    coverage reallocates splittable integer demand within existing facility capacities.
    """
    _require_analysis_gate(
        "evaluate_current_coverage",
        execution_snapshot_id,
        planning_dataset_ref,
        binding_fingerprint,
    )
    snapshot = _load_ref(snapshot_ref, NetworkSnapshot)
    matrix = _load_ref(route_matrix_ref, LegacyRouteMatrix)
    actual = evaluate_current_assignment(snapshot, matrix)
    existing_ids = [
        facility.facility_id for facility in snapshot.facilities if facility.is_existing
    ]
    optimized = evaluate_optimized_network(
        snapshot,
        matrix,
        active_facility_ids=existing_ids,
        scenario_id="optimized_existing_footprint",
    )
    actual_published = _store().publish(actual.schema_version, actual)
    optimized_published = _store().publish(optimized.schema_version, optimized)
    interpretation = (
        "Actual keeps recorded customer-to-warehouse relationships; optimized shows the "
        "best coverage and variable cost available after reallocation on the same footprint."
    )
    aggregate = CurrentCoverageResult(
        snapshot_id=snapshot.snapshot_id,
        route_matrix_id=matrix.route_matrix_id,
        actual_result_resource_name=actual_published.resource_id,
        actual_result_ref=resource_ref(actual_published),
        optimized_result_resource_name=optimized_published.resource_id,
        optimized_result_ref=resource_ref(optimized_published),
        actual_metrics=actual.metrics,
        optimized_metrics=optimized.metrics,
        interpretation=interpretation,
    )
    aggregate_published = _store().publish(aggregate.schema_version, aggregate)
    summary = (
        f"Current actual coverage is {actual.metrics.coverage_ratio:.2%} "
        f"({actual.metrics.covered_demand_units}/{actual.metrics.total_demand_units}); "
        f"optimized coverage on the same existing footprint is "
        f"{optimized.metrics.coverage_ratio:.2%} "
        f"({optimized.metrics.covered_demand_units}/"
        f"{optimized.metrics.total_demand_units})."
    )
    structured = CurrentCoverageToolResult(
        **aggregate.model_dump(),
        summary=summary,
        resource_name=aggregate_published.resource_id,
        resource_ref=resource_ref(aggregate_published),
    ).model_dump(mode="json")
    return _resource_call_result(
        aggregate_published,
        summary=summary,
        structured=structured,
    )


def evaluate_network_scenario(
    snapshot_ref: ResourceRef,
    route_matrix_ref: ResourceRef,
    scenario_id: str,
    active_facility_ids: list[str],
    planning_dataset_ref: ResourceRef,
    execution_snapshot_id: str | None = None,
    binding_fingerprint: str | None = None,
) -> Annotated[CallToolResult, ResourceToolResult]:
    """Optimize coverage and variable cost for an explicit set of active facilities."""
    _require_analysis_gate(
        "evaluate_network_scenario",
        execution_snapshot_id,
        planning_dataset_ref.model_dump(mode="json"),
        binding_fingerprint,
    )
    snapshot = _load_ref(snapshot_ref, NetworkSnapshot)
    matrix = _load_ref(route_matrix_ref, LegacyRouteMatrix)
    result = evaluate_optimized_network(
        snapshot,
        matrix,
        active_facility_ids=active_facility_ids,
        scenario_id=scenario_id,
    )
    published = _store().publish(result.schema_version, result)
    summary = (
        f"Scenario {scenario_id}: coverage {result.metrics.coverage_ratio:.2%}, "
        f"covered demand {result.metrics.covered_demand_units}/"
        f"{result.metrics.total_demand_units}, total cost "
        f"{result.metrics.total_cost} {snapshot.currency}, "
        f"{len(result.active_facility_ids)} active facilities."
    )
    structured = ResourceToolResult(
        summary=summary,
        resource_name=published.resource_id,
        resource_ref=resource_ref(published),
    ).model_dump(mode="json")
    return _resource_call_result(published, summary=summary, structured=structured)


def compare_legacy_network_scenarios(
    baseline_result_ref: ResourceRef,
    candidate_result_ref: ResourceRef,
    planning_dataset_ref: str,
    execution_snapshot_id: str | None = None,
    binding_fingerprint: str | None = None,
) -> Annotated[CallToolResult, ComparisonToolResult]:
    """Compare two scenario results built from the same snapshot, routes, and policy."""
    _require_analysis_gate(
        "compare_legacy_network_scenarios",
        execution_snapshot_id,
        planning_dataset_ref,
        binding_fingerprint,
    )
    baseline = _load_ref(baseline_result_ref, NetworkScenarioResult)
    candidate = _load_ref(candidate_result_ref, NetworkScenarioResult)
    comparison = compare_scenarios(baseline, candidate)
    published = _store().publish(comparison.schema_version, comparison)
    summary = (
        f"Candidate versus baseline: coverage "
        f"{comparison.coverage_ratio_delta:+.2%}, covered demand "
        f"{comparison.covered_demand_units_delta:+d}, total cost "
        f"{comparison.total_cost_delta:+f}."
    )
    structured = ComparisonToolResult(
        **comparison.model_dump(),
        summary=summary,
        resource_name=published.resource_id,
        resource_ref=resource_ref(published),
    ).model_dump(mode="json")
    return _resource_call_result(published, summary=summary, structured=structured)


def solve_facility_location(
    snapshot_ref: ResourceRef,
    route_matrix_ref: ResourceRef,
    target_coverage_ratio: float,
    planning_dataset_ref: str,
    execution_snapshot_id: str | None = None,
    binding_fingerprint: str | None = None,
) -> Annotated[CallToolResult, FacilityLocationToolResult]:
    """Find the fewest candidate warehouses that reach a target coverage ratio.

    The solver enumerates every subset of at most 14 candidate facilities. Within
    each subset it solves capacity-constrained, splittable integer allocation by
    min-cost flow. It minimizes added facility count first, then total cost.
    """
    _require_analysis_gate(
        "solve_facility_location",
        execution_snapshot_id,
        planning_dataset_ref,
        binding_fingerprint,
    )
    snapshot = _load_ref(snapshot_ref, NetworkSnapshot)
    matrix = _load_ref(route_matrix_ref, LegacyRouteMatrix)
    result, selected, evaluated, feasible = solve_location_candidates(
        snapshot,
        matrix,
        target_coverage_ratio=target_coverage_ratio,
    )
    result_published = _store().publish(result.schema_version, result)
    solution = FacilityLocationSolution(
        solution_id=f"solution_{result.result_id.removeprefix('result_')}",
        snapshot_id=snapshot.snapshot_id,
        route_matrix_id=matrix.route_matrix_id,
        target_coverage_ratio=target_coverage_ratio,
        status="optimal" if feasible else "infeasible",
        selected_candidate_facility_ids=selected,
        active_facility_ids=result.active_facility_ids,
        evaluated_subset_count=evaluated,
        result_resource_name=result_published.resource_id,
        result_ref=resource_ref(result_published),
        metrics=result.metrics,
        assumptions=[
            "Demand units are nonnegative integers and may split across facilities.",
            "Existing facilities remain open; only snapshot candidate facilities are selectable.",
            "Coverage requires end-to-end seconds at or below the snapshot policy threshold.",
            "Optimality applies only to the finite candidate set and registered route matrix.",
        ],
    )
    solution_published = _store().publish(solution.schema_version, solution)
    status_text = (
        "Found the exact minimum-facility solution"
        if feasible
        else "Target is infeasible for the supplied candidates; returning the best evaluated plan"
    )
    summary = (
        f"{status_text}: add {len(selected)} facilities {selected}, coverage "
        f"{result.metrics.coverage_ratio:.2%}, total cost "
        f"{result.metrics.total_cost} {snapshot.currency}; evaluated {evaluated} subsets."
    )
    structured = FacilityLocationToolResult(
        **solution.model_dump(),
        summary=summary,
        resource_name=solution_published.resource_id,
        solution_ref=resource_ref(solution_published),
    ).model_dump(mode="json")
    return _resource_call_result(
        solution_published,
        summary=summary,
        structured=structured,
    )


def evaluate_financial_case(
    snapshot_ref: ResourceRef,
    baseline_result_ref: ResourceRef,
    candidate_result_ref: ResourceRef,
    horizon_years: int = 5,
    discount_rate: float = 0.1,
    annual_growth_rate: float = 0.0,
    planning_dataset_ref: str | None = None,
    execution_snapshot_id: str | None = None,
    binding_fingerprint: str | None = None,
) -> Annotated[CallToolResult, FinancialEvaluationToolResult]:
    """Evaluate investment economics for one compatible network option.

    Opening investment comes from candidate facilities newly active in the candidate
    result. Scenario total-cost differences are treated as annual operating savings.
    The result is bounded to the supplied planning horizon and assumptions.
    """
    _require_analysis_gate(
        "evaluate_financial_case",
        execution_snapshot_id,
        planning_dataset_ref,
        binding_fingerprint,
    )
    snapshot = _load_ref(snapshot_ref, NetworkSnapshot)
    baseline = _load_ref(baseline_result_ref, NetworkScenarioResult)
    candidate = _load_ref(candidate_result_ref, NetworkScenarioResult)
    evaluation = calculate_financial_case(
        snapshot,
        baseline,
        candidate,
        snapshot_ref=snapshot_ref,
        baseline_result_ref=baseline_result_ref,
        candidate_result_ref=candidate_result_ref,
        horizon_years=horizon_years,
        discount_rate=discount_rate,
        annual_growth_rate=annual_growth_rate,
    )
    published = _store().publish(evaluation.schema_version, evaluation)
    summary = (
        f"Financial case {evaluation.evaluation_id}: opening investment "
        f"{evaluation.opening_investment} {snapshot.currency}, annual operating savings "
        f"{evaluation.annual_operating_savings} {snapshot.currency}, "
        f"{horizon_years}-year NPV {evaluation.net_present_value} "
        f"{snapshot.currency}, viable={evaluation.financially_viable}."
    )
    structured = FinancialEvaluationToolResult(
        **evaluation.model_dump(),
        summary=summary,
        resource_name=published.resource_id,
        resource_ref=resource_ref(published),
    ).model_dump(mode="json")
    return _resource_call_result(published, summary=summary, structured=structured)


def _scenario_from_name(resource_name: str) -> NetworkScenarioResult:
    return _load_ref(_ref_for_resource(resource_name), NetworkScenarioResult)


def _snapshot_from_name(resource_name: str) -> NetworkSnapshot:
    return _load_ref(_ref_for_resource(resource_name), NetworkSnapshot)


def _feature_point(
    *,
    feature_id: str,
    longitude: float,
    latitude: float,
    properties: dict[str, object],
) -> dict[str, object]:
    return {
        "type": "Feature",
        "id": feature_id,
        "properties": properties,
        "geometry": {"type": "Point", "coordinates": [longitude, latitude]},
    }


def _feature_line(
    *,
    feature_id: str,
    origin: tuple[float, float],
    destination: tuple[float, float],
    properties: dict[str, object],
) -> dict[str, object]:
    return {
        "type": "Feature",
        "id": feature_id,
        "properties": properties,
        "geometry": {
            "type": "LineString",
            "coordinates": [[origin[0], origin[1]], [destination[0], destination[1]]],
        },
    }


def prepare_network_comparison_map(
    snapshot_resource_name: str,
    current_result_resource_name: str,
    candidate_result_resource_name: str,
    comparison_resource_name: str,
    planning_dataset_ref: str,
    execution_snapshot_id: str | None = None,
    binding_fingerprint: str | None = None,
) -> Annotated[CallToolResult, NetworkMapToolResult]:
    """Publish a bounded current-versus-candidate map manifest and GeoJSON.

    The tool only joins planner-owned Resources from one snapshot. It does not
    geocode, route, or recompute business metrics; every feature is derived from
    the validated snapshot and scenario allocations.
    """
    _require_analysis_gate(
        "prepare_network_comparison_map",
        execution_snapshot_id,
        planning_dataset_ref,
        binding_fingerprint,
    )
    snapshot = _snapshot_from_name(snapshot_resource_name)
    current = _scenario_from_name(current_result_resource_name)
    candidate = _scenario_from_name(candidate_result_resource_name)
    comparison = _load_ref(_ref_for_resource(comparison_resource_name), ScenarioComparison)
    if current.snapshot_id != snapshot.snapshot_id or candidate.snapshot_id != snapshot.snapshot_id:
        raise ValueError("map inputs must reference the same network snapshot")
    if (
        comparison.baseline_result_id != current.result_id
        or comparison.candidate_result_id != candidate.result_id
    ):
        raise ValueError("comparison Resource does not match current and candidate results")

    facilities = {item.facility_id: item for item in snapshot.facilities}
    demand_points = {item.demand_id: item for item in snapshot.demand_points}
    features: list[dict[str, object]] = []
    for facility in snapshot.facilities:
        role = "existing" if facility.is_existing else "candidate"
        selected = facility.facility_id in candidate.active_facility_ids
        status = "existing" if facility.is_existing else ("selected" if selected else "unselected")
        features.append(
            _feature_point(
                feature_id=f"facility-{facility.facility_id}",
                longitude=facility.location.longitude,
                latitude=facility.location.latitude,
                properties={
                    "kind": "facility",
                    "facility_id": facility.facility_id,
                    "label": facility.label or facility.facility_id,
                    "status": status,
                    "role": role,
                    "capacity_units": facility.capacity_units,
                },
            )
        )
    demand_units = {item.demand_id: item.demand_units for item in snapshot.demand_points}
    for demand in snapshot.demand_points:
        features.append(
            _feature_point(
                feature_id=f"demand-{demand.demand_id}",
                longitude=demand.location.longitude,
                latitude=demand.location.latitude,
                properties={
                    "kind": "demand",
                    "demand_id": demand.demand_id,
                    "label": demand.demand_id,
                    "region": demand.region,
                    "demand_units": demand_units[demand.demand_id],
                },
            )
        )
    for scenario, prefix, label in (
        (current, "current", "Current allocation"),
        (candidate, "optimized", "Optimized allocation"),
    ):
        for allocation in scenario.allocations:
            if not allocation.facility_id or allocation.units <= 0:
                continue
            facility = facilities.get(allocation.facility_id)
            demand = demand_points.get(allocation.demand_id)
            if facility is None or demand is None:
                raise ValueError("scenario allocation references a missing map point")
            features.append(
                _feature_line(
                    feature_id=f"{prefix}-{allocation.facility_id}-{allocation.demand_id}",
                    origin=(facility.location.longitude, facility.location.latitude),
                    destination=(demand.location.longitude, demand.location.latitude),
                    properties={
                        "kind": "assignment",
                        "scenario": prefix,
                        "label": label,
                        "facility_id": allocation.facility_id,
                        "demand_id": allocation.demand_id,
                        "units": allocation.units,
                        "covered": allocation.covered,
                    },
                )
            )
    geojson = {"type": "FeatureCollection", "features": features}
    geojson_published = _store().publish("geojson.v1", geojson)
    title = "Current and optimized warehouse network"
    map_manifest = {
        "schema_version": "network_comparison_map.v1",
        "title": title,
        "summary": (
            f"Coverage changes from {current.metrics.coverage_ratio:.1%} to "
            f"{candidate.metrics.coverage_ratio:.1%}; cost changes by "
            f"{comparison.total_cost_delta} {snapshot.currency}."
        ),
        "snapshot_resource_name": snapshot_resource_name,
        "current_result_resource_name": current_result_resource_name,
        "candidate_result_resource_name": candidate_result_resource_name,
        "comparison_resource_name": comparison_resource_name,
        "geojson_resource_name": geojson_published.resource_id,
        "geojson_ref": {
            "type": "mcp_resource",
            "server": MCP_SERVER_NAME,
            "uri": geojson_published.uri,
            "resource_schema": "geojson.v1",
        },
        "feature_count": len(features),
        "layers": [
            {
                "id": "network-current-assignments",
                "type": "line",
                "source": "network",
                "paint": {"line-color": "#64748b", "line-width": 1.5, "line-opacity": 0.45},
            },
            {
                "id": "network-optimized-assignments",
                "type": "line",
                "source": "network",
                "filter": ["==", ["get", "scenario"], "optimized"],
                "paint": {"line-color": "#2563eb", "line-width": 2.5, "line-opacity": 0.75},
            },
            {
                "id": "network-facilities",
                "type": "circle",
                "source": "network",
                "filter": ["==", ["get", "kind"], "facility"],
                "paint": {
                    "circle-color": [
                        "match",
                        ["get", "status"],
                        "selected",
                        "#16a34a",
                        "unselected",
                        "#94a3b8",
                        "#f97316",
                    ],
                    "circle-radius": 7,
                    "circle-stroke-color": "#ffffff",
                    "circle-stroke-width": 1,
                },
            },
            {
                "id": "network-demand",
                "type": "circle",
                "source": "network",
                "filter": ["==", ["get", "kind"], "demand"],
                "paint": {
                    "circle-color": "#7c3aed",
                    "circle-radius": [
                        "interpolate",
                        ["linear"],
                        ["get", "demand_units"],
                        0,
                        3,
                        1000,
                        10,
                    ],
                    "circle-opacity": 0.7,
                },
            },
        ],
        "extensions": {
            "legend": {
                "title": "Network comparison",
                "items": [
                    {"label": "Existing facility", "color": "#f97316", "type": "circle"},
                    {"label": "Selected candidate", "color": "#16a34a", "type": "circle"},
                    {"label": "Current allocation", "color": "#64748b", "type": "line"},
                    {"label": "Optimized allocation", "color": "#2563eb", "type": "line"},
                ],
            },
            "hover": {
                "layers": [
                    {
                        "layer": "network-facilities",
                        "title_property": "label",
                        "fields": ["status", "capacity_units"],
                    },
                    {
                        "layer": "network-demand",
                        "title_property": "label",
                        "fields": ["region", "demand_units"],
                    },
                ]
            },
        },
    }
    map_published = _store().publish("network_comparison_map.v1", map_manifest)
    summary = str(map_manifest["summary"])
    structured = NetworkMapToolResult(
        summary=summary,
        map_resource_name=map_published.resource_id,
        map_ref=resource_ref(map_published),
        geojson_resource_name=geojson_published.resource_id,
        geojson_ref=ResourceRef(
            server=MCP_SERVER_NAME,
            resource_schema="geojson.v1",
            uri=geojson_published.uri,
        ),
        feature_count=len(features),
        title=title,
        layers=map_manifest["layers"],
        extensions=map_manifest["extensions"],
    ).model_dump(mode="json")
    return _resource_call_result(map_published, summary=summary, structured=structured)


def prepare_network_map_render(
    map_resource_name: str,
    geojson_resource_name: str,
    planning_dataset_ref: str,
    execution_snapshot_id: str | None = None,
    binding_fingerprint: str | None = None,
) -> Annotated[CallToolResult, NetworkMapRenderToolResult]:
    """Resolve exact planner map and GeoJSON names for Visualization Agent 2.0."""
    _require_analysis_gate(
        "prepare_network_map_render",
        execution_snapshot_id,
        planning_dataset_ref,
        binding_fingerprint,
    )
    map_ref = _ref_for_resource(map_resource_name)
    payload = _store().load(map_ref)
    if payload.get("schema_version") != "network_comparison_map.v1":
        raise ValueError("map_resource_name must identify network_comparison_map.v1")
    if payload.get("geojson_resource_name") != geojson_resource_name:
        raise ValueError("map and GeoJSON Resource names do not match")
    geojson_ref = payload.get("geojson_ref")
    if (
        not isinstance(geojson_ref, dict)
        or geojson_ref.get("uri") != f"supply-chain://resources/{geojson_resource_name}"
    ):
        raise ValueError("network map does not reference the requested GeoJSON Resource")
    geojson = _store().load_uri(f"supply-chain://resources/{geojson_resource_name}")
    if geojson.get("type") != "FeatureCollection" or not isinstance(geojson.get("features"), list):
        raise ValueError("GeoJSON Resource is not a FeatureCollection")
    result = NetworkMapRenderToolResult(
        summary=str(payload.get("summary", "Network comparison map ready")),
        map_manifest_resource_name=map_resource_name,
        geojson_resource_name=geojson_resource_name,
        geojson_ref=ResourceRef(
            server=MCP_SERVER_NAME,
            resource_schema="geojson.v1",
            uri=f"supply-chain://resources/{geojson_resource_name}",
        ),
        title=str(payload.get("title", "Network comparison")),
        layers=payload.get("layers", []),
        extensions=payload.get("extensions", {}),
    )
    return _resource_call_result(
        _store().publish("network_map_render_input.v1", result),
        summary=result.summary,
        structured=result.model_dump(mode="json"),
    )


def prepare_network_planning_report(
    snapshot_resource_name: str,
    current_result_resource_name: str,
    solution_resource_name: str,
    comparison_resource_name: str,
    map_resource_name: str | None = None,
    planning_dataset_ref: str | None = None,
    execution_snapshot_id: str | None = None,
    binding_fingerprint: str | None = None,
) -> Annotated[CallToolResult, NetworkPlanningReportToolResult]:
    """Publish a deterministic report Resource from validated planner evidence."""
    _require_analysis_gate(
        "prepare_network_planning_report",
        execution_snapshot_id,
        planning_dataset_ref,
        binding_fingerprint,
    )
    snapshot = _snapshot_from_name(snapshot_resource_name)
    current = _scenario_from_name(current_result_resource_name)
    solution = _load_ref(_ref_for_resource(solution_resource_name), FacilityLocationSolution)
    comparison = _load_ref(_ref_for_resource(comparison_resource_name), ScenarioComparison)
    if solution.result_resource_name:
        candidate = _scenario_from_name(solution.result_resource_name)
    else:
        raise ValueError("facility solution is missing its candidate result Resource")
    if current.snapshot_id != snapshot.snapshot_id or candidate.snapshot_id != snapshot.snapshot_id:
        raise ValueError("report inputs must reference one snapshot")
    if (
        comparison.baseline_result_id != current.result_id
        or comparison.candidate_result_id != candidate.result_id
    ):
        raise ValueError("report comparison does not match supplied scenarios")
    map_note = map_resource_name or "not prepared"
    title = "Warehouse network planning report"
    classification_line = (
        "- Data classification: Synthetic demo (not observed business data)."
        if snapshot.data_classification == "synthetic_demo"
        else "- Data classification: Workspace data."
    )
    markdown = "\n".join(
        [
            f"# {title}",
            "",
            "## Evidence and scope",
            f"- Planning period: {snapshot.planning_period}",
            f"- Currency: {snapshot.currency}",
            classification_line,
            f"- Source digest: {snapshot.source_digest or 'not supplied'}",
            (
                f"- Demand points: {len(snapshot.demand_points)}; "
                f"facilities: {len(snapshot.facilities)}"
            ),
            (
                f"- Current coverage: {current.metrics.coverage_ratio:.1%}; "
                f"current cost: {current.metrics.total_cost} {snapshot.currency}"
            ),
            "",
            "## Candidate warehouse optimization",
            f"- Status: {solution.status}",
            (
                "- Selected candidates: "
                f"{', '.join(solution.selected_candidate_facility_ids) or 'none'}"
            ),
            (
                f"- Optimized coverage: {candidate.metrics.coverage_ratio:.1%}; "
                f"optimized cost: {candidate.metrics.total_cost} {snapshot.currency}"
            ),
            (
                f"- Coverage change: {comparison.coverage_ratio_delta:+.1%}; "
                f"cost change: {comparison.total_cost_delta:+} {snapshot.currency}"
            ),
            "",
            "## Limitations",
            "- Optimization is exact only over the finite candidate facilities and "
            "supplied route matrix.",
            f"- Comparison map Resource: {map_note}.",
            "- Route quality and assumptions are inherited from the confirmed planning dataset.",
        ]
    )
    digest = hashlib.sha256(markdown.encode("utf-8")).hexdigest()
    payload = {
        "schema_version": "network_planning_report.v1",
        "title": title,
        "status": "ready",
        "markdown": markdown,
        "markdown_sha256": digest,
        "dataClassification": snapshot.data_classification,
        "demoTemplate": snapshot.demo_template,
        "sourceDigest": snapshot.source_digest,
        "input_resources": [
            snapshot_resource_name,
            current_result_resource_name,
            solution_resource_name,
            comparison_resource_name,
            map_note,
        ],
        "generated_at": datetime.now(UTC).isoformat(),
    }
    report_published = _store().publish("network_planning_report.v1", payload)
    report_ref = ResourceRef(
        server=MCP_SERVER_NAME,
        resource_schema="network_planning_report.v1",
        uri=report_published.uri,
    )
    artifact = {
        "schema_version": "report.v1",
        "title": title,
        "status": "ready",
        "source": report_ref.model_dump(mode="json"),
        "dataClassification": snapshot.data_classification,
        "demoTemplate": snapshot.demo_template,
    }
    artifact_published = _store().publish("report.v1", artifact)
    summary = (
        f"Published {title} with current and optimized coverage, cost, candidate "
        "selection, and limitations."
    )
    structured = NetworkPlanningReportToolResult(
        summary=summary,
        resource_name=report_published.resource_id,
        resource_ref=report_ref,
        title=title,
        markdown_sha256=digest,
        report_resource_name=artifact_published.resource_id,
        report_ref=resource_ref(artifact_published),
    ).model_dump(mode="json")
    return _resource_call_result(report_published, summary=summary, structured=structured)


def publish_risk_register(
    decision_scope: str,
    risks: list[RiskItem],
) -> Annotated[CallToolResult, RiskRegisterToolResult]:
    """Validate and publish a bounded risk register backed by planning Resources."""
    for risk in risks:
        for evidence_ref in risk.evidence_refs:
            _validate_evidence_ref(evidence_ref)
    register = build_risk_register(decision_scope, risks)
    published = _store().publish(register.schema_version, register)
    summary = (
        f"Published risk register {register.register_id} with {len(risks)} risks; "
        f"{register.unresolved_risk_count} have likelihood-impact score at least 12."
    )
    structured = RiskRegisterToolResult(
        **register.model_dump(),
        summary=summary,
        resource_name=published.resource_id,
        resource_ref=resource_ref(published),
    ).model_dump(mode="json")
    return _resource_call_result(published, summary=summary, structured=structured)


def validate_network_resource(
    resource_ref: ResourceRef,
) -> ValidationResult:
    """Validate a snapshot, route matrix, scenario result, comparison, or solution."""
    errors: list[str] = []
    warnings: list[str] = []
    checks: list[str] = []
    payload = _store().load(resource_ref)
    schema = str(payload.get("schema_version", payload.get("schemaVersion", "unknown")))
    model_by_schema = {
        "network_snapshot.v1": NetworkSnapshot,
        "route_matrix.v1": ComposableRouteMatrix,
        "network_assignment.v1": AssignmentResult,
        "cost_matrix.v1": CostMatrix,
        "network_scenario_result.v1": NetworkScenarioResult,
        "current_coverage_result.v1": CurrentCoverageResult,
        "scenario_comparison.v1": ScenarioComparison,
        "facility_location_solution.v1": FacilityLocationSolution,
        "financial_evaluation.v1": FinancialEvaluation,
        "risk_register.v1": RiskRegister,
    }
    model_type = model_by_schema.get(schema)
    if schema in {
        "network_comparison_map.v1",
        "geojson.v1",
        "network_planning_report.v1",
        "report.v1",
        "network_service_metrics.v1",
        "network_baseline.v1",
        "network_comparison.v1",
        "facility_location_solution.v2",
        "service_constrained_solution.v1",
    }:
        if schema == "geojson.v1":
            if payload.get("type") != "FeatureCollection" or not isinstance(
                payload.get("features"), list
            ):
                errors.append("GeoJSON resource must be a FeatureCollection")
            elif len(payload["features"]) > 5_000:
                errors.append("GeoJSON feature limit exceeded")
        elif schema == "network_comparison_map.v1":
            if not isinstance(payload.get("geojson_resource_name"), str) or not isinstance(
                payload.get("layers"), list
            ):
                errors.append("network comparison map is missing its bounded GeoJSON and layers")
            checks.append("network comparison map retains exact GeoJSON and planner evidence names")
        elif schema == "network_planning_report.v1":
            if payload.get("status") != "ready" or not isinstance(
                payload.get("markdown_sha256"), str
            ):
                errors.append("network planning report is not ready or lacks its content hash")
            if not isinstance(payload.get("evidence_index"), list):
                errors.append("network planning report is missing its evidence index")
            elif any(
                not isinstance(entry, dict)
                or not isinstance(entry.get("resource_schema"), str)
                or not isinstance(entry.get("resource_name"), str)
                for entry in payload["evidence_index"]
            ):
                errors.append("network planning report contains an invalid evidence index")
            checks.append("report contains a bounded immutable source")
        elif schema == "report.v1":
            if payload.get("status") != "ready" or not isinstance(payload.get("source"), dict):
                errors.append("report delivery does not reference a ready source")
            checks.append("report delivery preserves the source Resource reference")
        elif schema == "network_service_metrics.v1":
            metrics = payload.get("metrics")
            if not isinstance(metrics, list):
                errors.append("service metrics are missing their metrics list")
            checks.append("service metrics retain demand-weighted coverage values")
        elif schema == "network_baseline.v1":
            if payload.get("label") not in {"actual_current", "optimized_existing_footprint"}:
                errors.append("network baseline has an invalid plan label")
            if not isinstance(payload.get("assignment"), dict):
                errors.append("network baseline is missing its assignment")
            checks.append(
                "baseline distinguishes actual current coverage from an optimized footprint"
            )
        elif schema == "network_comparison.v1":
            if not isinstance(payload.get("baseline_ref"), dict) or not isinstance(
                payload.get("candidate_ref"), dict
            ):
                errors.append("network comparison is missing opaque scenario references")
            checks.append("network comparison retains references instead of copying scenario rows")
        elif schema in {"facility_location_solution.v2", "service_constrained_solution.v1"}:
            if payload.get("status") not in {
                "optimal",
                "feasible",
                "timeout",
                "infeasible",
                "unavailable",
            }:
                errors.append("facility solution has an invalid solver status")
            checks.append("facility solution exposes its explicit solver status")
        if not errors:
            checks.append(f"resource conforms to {schema}")
    elif model_type is None:
        errors.append(f"unsupported resource schema {schema!r}")
    else:
        try:
            value = model_type.model_validate(payload)
            checks.append(f"resource conforms to {schema}")
            if isinstance(value, NetworkScenarioResult):
                allocation_total = sum(item.units for item in value.allocations)
                if allocation_total != value.metrics.total_demand_units:
                    errors.append("allocation units do not equal metrics.total_demand_units")
                covered = sum(item.units for item in value.allocations if item.covered)
                if covered != value.metrics.covered_demand_units:
                    errors.append(
                        "covered allocation units do not equal metrics.covered_demand_units"
                    )
                expected_ratio = (
                    covered / value.metrics.total_demand_units
                    if value.metrics.total_demand_units
                    else 0
                )
                if abs(expected_ratio - value.metrics.coverage_ratio) > 1e-12:
                    errors.append("coverage_ratio is inconsistent with allocation units")
                checks.append("scenario allocation totals and coverage ratio are consistent")
                warnings.extend(value.issues)
            if isinstance(value, LegacyRouteMatrix):
                pairs = {
                    (entry.origin_city_id, entry.destination_city_id) for entry in value.entries
                }
                if len(pairs) != len(value.entries):
                    errors.append("route matrix contains duplicate origin-destination pairs")
                checks.append("route matrix origin-destination pairs are unique")
            if isinstance(value, FacilityLocationSolution):
                if value.status == "optimal" and (
                    value.metrics.coverage_ratio + 1e-12 < value.target_coverage_ratio
                ):
                    errors.append("optimal solution does not reach its coverage target")
                checks.append("facility-location status is consistent with target coverage")
            if isinstance(value, FinancialEvaluation):
                if len({ref.uri for ref in value.input_refs}) != 3:
                    errors.append("financial evaluation must cite three distinct inputs")
                checks.append("financial evaluation retains its snapshot and scenario lineage")
            if isinstance(value, RiskRegister):
                if value.unresolved_risk_count != sum(
                    risk.likelihood * risk.impact >= 12 for risk in value.risks
                ):
                    errors.append("risk register unresolved count is inconsistent")
                checks.append("risk register scores and evidence references are present")
        except ValidationError as error:
            errors.extend(
                f"{'.'.join(str(part) for part in item['loc'])}: {item['msg']}"
                for item in error.errors()
            )
    return ValidationResult(
        valid=not errors,
        resource_schema=schema,
        errors=errors,
        warnings=warnings,
        checks=checks,
    )


def _publish_new_resource(schema: str, value: object, summary: str) -> CallToolResult:
    published = _store().publish(schema, value if isinstance(value, dict) else value)
    artifact = _artifact_ref(published)
    return CallToolResult(
        content=[
            TextContent(type="text", text=summary),
            _resource_link(published, summary),
        ],
        structuredContent={
            "summary": summary,
            "resource_name": published.resource_id,
            "artifact_ref": artifact.model_dump(mode="json"),
            "resource_ref": resource_ref(published).model_dump(mode="json"),
        },
    )


def _load_new_model(ref: ArtifactRef, model_type):
    payload = _load_artifact(ref)
    if "schemaVersion" in payload and "schema_version" not in payload:
        payload["schema_version"] = payload["schemaVersion"]
    payload = {
        key: value
        for key, value in payload.items()
        if key not in {"schemaVersion", "contract", "taskEvidence"}
    }
    return model_type.model_validate(payload)


def _legacy_plan_route_matrix(
    case_id: str,
    route_method: Literal["haversine", "navigation", "provided"],
    ctx: Context,
    detour_coefficient: float | None = None,
    average_speed_kph: float | None = None,
) -> CallToolResult:
    """Count routes and validate parameters before building a Case route matrix."""
    try:
        parsed = UUID(case_id)
        plan = CaseMatrixService(_cases()).plan_routes(
            parsed,
            _workspace(ctx),
            route_method,
            detour_coefficient,
            average_speed_kph,
        )
        case = _cases().get_case(parsed, _workspace(ctx))
    except (CaseRepositoryError, ValueError) as error:
        return _case_error_result(
            error
            if isinstance(error, CaseRepositoryError)
            else CaseRepositoryError("route_plan_invalid", str(error))
        )
    return ToolEnvelopeBuilder().build(
        case=case,
        summary=(
            f"Planned {plan.route_count} routes using {plan.method}; "
            f"estimated billable calls: {plan.estimated_billable_calls}."
        ),
        evidence=[
            EvidenceSummary(
                kind="parameter",
                evidence_id="route_count",
                label="Route count",
                value=plan.route_count,
            ),
            EvidenceSummary(
                kind="parameter",
                evidence_id="estimated_billable_calls",
                label="Estimated billable calls",
                value=plan.estimated_billable_calls,
            ),
        ],
        next_action=(
            "confirm_navigation_cost"
            if route_method == "navigation"
            else "build_haversine_route_matrix"
        ),
    )


def _legacy_build_haversine_route_matrix(
    case_id: str,
    detour_coefficient: float,
    average_speed_kph: float,
    ctx: Context,
) -> CallToolResult:
    """Build and persist distance = haversine distance multiplied by a coefficient."""
    try:
        parsed = UUID(case_id)
        matrix, result = CaseMatrixService(_cases()).build_haversine(
            parsed, _workspace(ctx), detour_coefficient, average_speed_kph
        )
        case = _cases().get_case(parsed, _workspace(ctx))
        status = _cases().get_status(parsed, _workspace(ctx))
    except (CaseRepositoryError, ValueError) as error:
        return _case_error_result(
            error
            if isinstance(error, CaseRepositoryError)
            else CaseRepositoryError("route_matrix_invalid", str(error))
        )
    facet = next(item for item in status.facets if item.name.value == "route_matrix")
    return ToolEnvelopeBuilder().build(
        case=case,
        operation=result.operation,
        summary=f"Built and stored {len(matrix.rows)} distance and duration routes.",
        facet_updates=[facet],
        evidence=[
            EvidenceSummary(
                kind="metric",
                evidence_id="route_count",
                label="Stored routes",
                value=len(matrix.rows),
            )
        ],
        next_action="plan_cost_matrix_or_evaluate_service",
    )


def _legacy_validate_route_matrix(
    case_id: str,
    ctx: Context,
) -> CallToolResult:
    """Validate the currently bound Case route matrix without returning its rows."""
    try:
        parsed = UUID(case_id)
        normalized, _ = _cases().load_normalized_input(parsed, _workspace(ctx))
        matrix, _ = _cases().load_route_matrix(parsed, _workspace(ctx))
        validation = _validate_route_matrix_model(
            normalized.demand_cities, normalized.warehouses, matrix
        )
        case = _cases().get_case(parsed, _workspace(ctx))
    except (CaseRepositoryError, ValueError) as error:
        return _case_error_result(
            error
            if isinstance(error, CaseRepositoryError)
            else CaseRepositoryError("route_matrix_validation_failed", str(error))
        )
    return ToolEnvelopeBuilder().build(
        case=case,
        summary=(
            f"Route matrix validation {'passed' if validation['valid'] else 'failed'} "
            f"for {validation['route_count']} routes."
        ),
        issues=(
            []
            if validation["valid"]
            else [
                SafeIssue(
                    code="route_matrix_incomplete",
                    severity="error",
                    business_message="距离时效矩阵不完整，不能用于网络分析。",
                )
            ]
        ),
        next_action="evaluate_service_baseline" if validation["valid"] else None,
    )


def _legacy_register_navigation_route_matrix(
    case_id: str,
    navigation_rows: list[RouteMatrixRow],
    ctx: Context,
) -> CallToolResult:
    """Register one batch of navigation results without returning route rows."""
    if len(navigation_rows) > 50_000:
        return _case_error_result(
            CaseRepositoryError(
                "navigation_matrix_too_large",
                "The navigation matrix exceeds the bounded batch size.",
            )
        )
    try:
        parsed = UUID(case_id)
        matrix, operation = CaseMatrixService(_cases()).register_navigation(
            parsed,
            _workspace(ctx),
            navigation_rows,
        )
        case = _cases().get_case(parsed, _workspace(ctx))
        status = _cases().get_status(parsed, _workspace(ctx))
    except (CaseRepositoryError, ValueError, ValidationError) as error:
        if isinstance(error, CaseRepositoryError):
            return _case_error_result(error)
        return _case_error_result(CaseRepositoryError("navigation_matrix_invalid", str(error)))
    facet = next(item for item in status.facets if item.name.value == "route_matrix")
    issues = (
        [
            SafeIssue(
                code="navigation_matrix_incomplete",
                severity="error",
                business_message=(
                    f"导航结果缺少 {len(matrix.missing_routes)} 条路线，不能用于网络分析。"
                ),
            )
        ]
        if matrix.missing_routes
        else []
    )
    return ToolEnvelopeBuilder().build(
        case=case,
        operation=operation,
        summary=(
            f"Registered {len(matrix.rows)} navigation routes; "
            f"{len(matrix.missing_routes)} routes remain incomplete."
        ),
        facet_updates=[facet],
        issues=issues,
        next_action=(
            "validate_route_matrix" if not matrix.missing_routes else "request_navigation_rows"
        ),
    )


def _legacy_plan_cost_matrix(
    case_id: str,
    ctx: Context,
    fallback_rule: dict[str, object] | None = None,
    warehouse_scope: Literal["existing_only", "all_warehouses"] = "existing_only",
) -> CallToolResult:
    """Check whether quotes and an optional fallback rule can form a complete cost matrix."""
    try:
        parsed = UUID(case_id)
        matrix, result = CaseMatrixService(_cases()).build_costs(
            parsed, _workspace(ctx), fallback_rule, warehouse_scope
        )
        case = _cases().get_case(parsed, _workspace(ctx))
        status = _cases().get_status(parsed, _workspace(ctx))
    except (CaseRepositoryError, ValueError) as error:
        return _case_error_result(
            error
            if isinstance(error, CaseRepositoryError)
            else CaseRepositoryError("cost_matrix_invalid", str(error))
        )
    facet = next(item for item in status.facets if item.name.value == "cost_matrix")
    issues = []
    if matrix.missing_routes:
        issues.append(
            SafeIssue(
                code="cost_rule_required",
                severity="warning",
                business_message=(
                    f"仍有 {len(matrix.missing_routes)} 条路线缺少报价；"
                    "请提供每公里费用、固定起步价和币种。"
                ),
            )
        )
    return ToolEnvelopeBuilder().build(
        case=case,
        operation=result.operation if result is not None else None,
        summary=(
            f"Cost matrix has {len(matrix.rows)} usable routes and "
            f"{len(matrix.missing_routes)} missing routes."
        ),
        facet_updates=[facet],
        issues=issues,
        next_action=("request_cost_rule" if matrix.missing_routes else "evaluate_cost_baseline"),
    )


def _legacy_compute_optimal_assignment(
    network_case_ref: ArtifactRef | ResourceRef,
    route_matrix_ref: ArtifactRef,
    cost_matrix_ref: ArtifactRef | None = None,
    objective: str = "min_time",
) -> CallToolResult:
    case = _case_from_input_ref(network_case_ref)
    routes = _load_new_model(route_matrix_ref, ComposableRouteMatrix)
    costs = _load_new_model(cost_matrix_ref, CostMatrix) if cost_matrix_ref else None
    if objective not in {"min_time", "min_cost"}:
        raise ValueError("objective_must_be_min_time_or_min_cost")
    assignment = solve_assignment(case.demand, case.warehouses, routes, costs, objective)
    return _publish_new_resource(
        assignment.schema_version,
        assignment,
        f"Computed a {objective} assignment for {assignment.total_demand} demand units; {assignment.unassigned_demand} remain unassigned.",
    )


def _legacy_evaluate_service_targets(
    assignment_ref: ArtifactRef,
    target_hours: list[float],
) -> CallToolResult:
    assignment = _load_new_model(assignment_ref, AssignmentResult)
    metrics = [
        metric.model_dump(mode="json") for metric in service_metrics(assignment, target_hours)
    ]
    payload = {"schema_version": "network_service_metrics.v1", "metrics": metrics}
    return _publish_new_resource(
        "network_service_metrics.v1",
        payload,
        "Calculated service coverage for the requested time targets.",
    )


def _legacy_summarize_network_cost(
    assignment_ref: ArtifactRef,
    cost_matrix_ref: ArtifactRef,
    currency: str = "IDR",
) -> CallToolResult:
    assignment = _load_new_model(assignment_ref, AssignmentResult)
    costs = _load_new_model(cost_matrix_ref, CostMatrix) if cost_matrix_ref else None
    if costs is None:
        raise ValueError("cost_matrix_required_for_network_cost_summary")
    cost_index = {
        (row.origin_id, row.destination_id, row.layer): row.cost_per_demand_unit
        for row in costs.rows
    }
    by_warehouse: dict[str, float] = {}
    linehaul = 0.0
    last_mile = 0.0
    missing_routes: list[tuple[str, str, str]] = list(costs.missing_routes)
    for row in assignment.rows:
        if row.warehouse_id is None:
            continue
        if row.cost is not None:
            amount = row.cost * row.demand_quantity
            last_mile += amount
            by_warehouse[row.warehouse_id] = by_warehouse.get(row.warehouse_id, 0.0) + amount
        if row.upstream_center_id and row.upstream_center_id != row.warehouse_id:
            key = (row.upstream_center_id, row.warehouse_id, "linehaul")
            price = cost_index.get(key)
            if price is None:
                missing_routes.append(key)
            else:
                amount = price * row.demand_quantity
                linehaul += amount
                by_warehouse[row.upstream_center_id] = (
                    by_warehouse.get(row.upstream_center_id, 0.0) + amount
                )
    total = last_mile + linehaul
    summary = CostSummary(
        currency=currency.upper(),
        total=total,
        linehaul=linehaul,
        last_mile=last_mile,
        by_warehouse=dict(sorted(by_warehouse.items())),
        complete=not missing_routes and assignment.unassigned_demand == 0,
        missing_routes=sorted(set(missing_routes)),
    )
    state = "complete" if summary.complete else "incomplete"
    return _publish_new_resource(
        summary.schema_version,
        summary,
        f"Network cost summary is {state}: {total:.2f} {currency.upper()} "
        f"(linehaul {linehaul:.2f}, last mile {last_mile:.2f}).",
    )


def _legacy_evaluate_network_baseline(
    case_id: str,
    objective: Literal["min_time", "min_cost"],
    service_targets: list[float],
    ctx: Context,
    coverage_mode: Literal[
        "actual_if_available", "optimized_existing_footprint"
    ] = "actual_if_available",
    include_cost: bool = True,
) -> CallToolResult:
    """Persist an actual or optimized baseline and return only bounded metrics."""
    try:
        parsed = UUID(case_id)
        baseline, result = NetworkAnalysisService(_cases()).evaluate_baseline(
            parsed,
            _workspace(ctx),
            objective,
            service_targets,
            coverage_mode,
            include_cost,
        )
        case = _cases().get_case(parsed, _workspace(ctx))
        status = _cases().get_status(parsed, _workspace(ctx))
    except (CaseRepositoryError, ValueError) as error:
        return _case_error_result(
            error
            if isinstance(error, CaseRepositoryError)
            else CaseRepositoryError("baseline_invalid", str(error))
        )
    facet = next(item for item in status.facets if item.name.value == "baseline")
    evidence = [
        EvidenceSummary(
            kind="metric",
            evidence_id=f"service_{metric.target_hours:g}_hours",
            label=f"Coverage within {metric.target_hours:g} hours",
            value=round(metric.coverage_rate, 6),
        )
        for metric in baseline.service
    ]
    if baseline.cost is not None:
        evidence.append(
            EvidenceSummary(
                kind="metric",
                evidence_id="network_total_cost",
                label=f"Total network cost ({baseline.cost.currency})",
                value=round(baseline.cost.total, 2),
            )
        )
    issues = []
    if baseline.notice_code == "current_assignment_missing":
        issues.append(
            SafeIssue(
                code="current_assignment_missing",
                severity="warning",
                business_message="未发现当前覆盖方案，结果是现有仓范围内的优化基线。",
            )
        )
    return ToolEnvelopeBuilder().build(
        case=case,
        operation=result.operation,
        summary=(
            f"Stored {baseline.label} baseline for {baseline.assignment.total_demand} "
            f"demand units; {baseline.assignment.unassigned_demand} are unassigned."
        ),
        facet_updates=[facet],
        issues=issues,
        evidence=evidence,
        next_action="publish_network_planning_report",
    )


def _legacy_evaluate_facility_scenario(
    case_id: str,
    scenario: dict[str, object],
    ctx: Context,
) -> CallToolResult:
    """Evaluate an explicit add, remove or relocation scenario inside one Case."""
    try:
        parsed = UUID(case_id)
        spec = ScenarioSpec.model_validate(scenario)
        result_value, result = NetworkScenarioService(_cases()).evaluate(
            parsed, _workspace(ctx), spec
        )
        case = _cases().get_case(parsed, _workspace(ctx))
        status = _cases().get_status(parsed, _workspace(ctx))
    except (CaseRepositoryError, ValueError, ValidationError) as error:
        return _case_error_result(
            error
            if isinstance(error, CaseRepositoryError)
            else CaseRepositoryError("scenario_invalid", str(error))
        )
    facet = next(item for item in status.facets if item.name.value == "scenario")
    evidence = [
        EvidenceSummary(
            kind="metric",
            evidence_id=f"scenario_service_{metric.target_hours:g}_hours",
            label=f"Scenario coverage within {metric.target_hours:g} hours",
            value=round(metric.coverage_rate, 6),
        )
        for metric in result_value.service
    ]
    if result_value.cost is not None:
        evidence.append(
            EvidenceSummary(
                kind="metric",
                evidence_id="scenario_total_cost",
                label=f"Scenario total cost ({result_value.cost.currency})",
                value=round(result_value.cost.total, 2),
            )
        )
    return ToolEnvelopeBuilder().build(
        case=case,
        operation=result.operation,
        summary=(
            f"Stored scenario with {len(result_value.warehouse_changes.get('added', []))} "
            f"added and {len(result_value.warehouse_changes.get('removed', []))} "
            f"removed warehouses; {result_value.assignment.unassigned_demand} demand "
            "units are unassigned."
        ),
        facet_updates=[facet],
        evidence=evidence,
        next_action="compare_network_scenarios",
    )


def _legacy_solve_p_median(
    case_id: str,
    number_to_open: int,
    ctx: Context,
    fixed_existing_ids: list[str] | None = None,
    optional_existing_ids: list[str] | None = None,
    time_limit_seconds: float = 30,
) -> CallToolResult:
    """Solve p-median against the current Case matrices and candidate set."""
    try:
        parsed = UUID(case_id)
        solution, result = FacilityLocationService(_cases()).solve(
            parsed,
            _workspace(ctx),
            number_to_open=number_to_open,
            fixed_existing_ids=set(fixed_existing_ids or []),
            optional_existing_ids=set(optional_existing_ids or []),
            time_limit_seconds=time_limit_seconds,
        )
        case = _cases().get_case(parsed, _workspace(ctx))
        status = _cases().get_status(parsed, _workspace(ctx))
    except (CaseRepositoryError, ValueError) as error:
        return _case_error_result(
            error
            if isinstance(error, CaseRepositoryError)
            else CaseRepositoryError("facility_location_invalid", str(error))
        )
    facet = next(item for item in status.facets if item.name.value == "facility_location")
    return ToolEnvelopeBuilder().build(
        case=case,
        operation=result.operation,
        summary=(
            f"Facility-location status is {solution.status}; selected "
            f"{len(solution.selected_warehouse_ids)} warehouses."
        ),
        facet_updates=[facet],
        evidence=[
            EvidenceSummary(
                kind="metric",
                evidence_id="facility_objective_value",
                label="Facility objective value",
                value=solution.objective_value,
            )
        ],
        issues=(
            []
            if solution.status in {"optimal", "feasible"}
            else [
                SafeIssue(
                    code=f"facility_{solution.status}",
                    severity="warning" if solution.status == "timeout" else "error",
                    business_message=(
                        "选址求解未证明最优，结果只能按当前求得状态解释。"
                        if solution.status == "timeout"
                        else "选址求解没有可交付方案。"
                    ),
                )
            ]
        ),
        next_action="publish_network_planning_report",
    )


def _legacy_solve_service_constrained_location(
    case_id: str,
    number_to_open: int,
    minimum_coverage: float,
    ctx: Context,
    service_target_hours: float = 24,
    time_limit_seconds: float = 30,
) -> CallToolResult:
    """Solve a cost objective with an explicit service-coverage constraint."""
    try:
        parsed = UUID(case_id)
        solution, result = FacilityLocationService(_cases()).solve(
            parsed,
            _workspace(ctx),
            number_to_open=number_to_open,
            fixed_existing_ids=set(),
            optional_existing_ids=set(),
            time_limit_seconds=time_limit_seconds,
            service_constraints=[(service_target_hours, minimum_coverage)],
        )
        case = _cases().get_case(parsed, _workspace(ctx))
        status = _cases().get_status(parsed, _workspace(ctx))
    except (CaseRepositoryError, ValueError) as error:
        return _case_error_result(
            error
            if isinstance(error, CaseRepositoryError)
            else CaseRepositoryError("service_constrained_location_invalid", str(error))
        )
    facet = next(item for item in status.facets if item.name.value == "facility_location")
    achieved = solution.service[0].coverage_rate if solution.service else None
    return ToolEnvelopeBuilder().build(
        case=case,
        operation=result.operation,
        summary=(
            f"Service-constrained location status is {solution.status}; "
            f"coverage at {service_target_hours:g} hours is "
            f"{achieved if achieved is not None else 'unavailable'}."
        ),
        facet_updates=[facet],
        evidence=[
            EvidenceSummary(
                kind="metric",
                evidence_id="service_constrained_coverage",
                label=f"Coverage within {service_target_hours:g} hours",
                value=achieved,
            )
        ],
        next_action="publish_network_planning_report",
    )


@mcp.tool(structured_output=True, annotations=CONTENT_ADDRESSED_RESOURCE_TOOL)
def compare_network_scenarios(
    baseline_ref: ResourceRef,
    candidate_ref: ResourceRef,
    service_targets: Annotated[list[float], Field(min_length=1, max_length=32)],
    ctx: Context,
) -> CallToolResult:
    """Compare a typed baseline with another baseline, scenario, or location result."""
    _runtime().require_workspace(ctx)
    if any(target <= 0 for target in service_targets):
        raise McpResourceContractError("comparison_service_targets_invalid")
    baseline = _runtime().load_model(
        baseline_ref,
        "network_baseline.v2",
        BaselineResult,
    )
    if candidate_ref.resource_schema == "network_scenario.v2":
        candidate = _runtime().load_model(
            candidate_ref,
            "network_scenario.v2",
            ScenarioResult,
        )
        candidate_assignment = candidate.assignment
        candidate_active_ids = set(candidate.active_warehouse_ids)
    elif candidate_ref.resource_schema == "network_baseline.v2":
        candidate = _runtime().load_model(
            candidate_ref,
            "network_baseline.v2",
            BaselineResult,
        )
        candidate_assignment = candidate.assignment
        candidate_active_ids = set(candidate.active_warehouse_ids)
    elif candidate_ref.resource_schema == "facility_location_solution.v3":
        candidate = _runtime().load_model(
            candidate_ref,
            "facility_location_solution.v3",
            PMedianSolution,
        )
        if candidate.assignment is None:
            raise McpResourceContractError("candidate_assignment_unavailable")
        candidate_assignment = candidate.assignment
        candidate_active_ids = set(candidate.active_warehouse_ids)
    else:
        raise McpResourceContractError("comparison_candidate_schema_invalid")
    comparison: AssignmentComparison = compare_assignments(
        baseline.assignment,
        candidate_assignment,
        service_targets,
        set(baseline.active_warehouse_ids),
        candidate_active_ids,
    )
    service_summary = (
        ", ".join(
            f"{metric.target_hours:g}h demand-weighted "
            f"{metric.before_coverage_rate:.1%}→"
            f"{metric.after_coverage_rate:.1%} ({metric.coverage_rate_delta:+.1%})"
            for metric in comparison.service[:8]
        )
        or "none"
    )
    if len(comparison.service) > 8:
        service_summary = f"{service_summary}, +{len(comparison.service) - 8} more"
    cost_summary = (
        "unavailable"
        if comparison.before_cost is None or comparison.after_cost is None
        else f"{comparison.before_cost:.2f}→{comparison.after_cost:.2f} "
        f"({comparison.cost_delta or 0:+.2f})"
    )
    return _runtime().publish(
        comparison.schema_version,
        comparison,
        f"Compared {len(comparison.city_changes)} city assignments; selected "
        f"[{_bounded_id_summary(comparison.selected_warehouse_ids)}], removed "
        f"[{_bounded_id_summary(comparison.removed_warehouse_ids)}], affected "
        f"{len(comparison.affected_city_ids)}, reassigned "
        f"{len(comparison.reassigned_city_ids)}; cost {cost_summary}; service "
        f"{service_summary}.",
    )


def _assignment_rows(payload: dict[str, object]) -> list[dict[str, object]]:
    assignment = payload.get("assignment")
    if isinstance(assignment, dict):
        rows = assignment.get("rows")
        if isinstance(rows, list):
            return [row for row in rows if isinstance(row, dict)]
    rows = payload.get("rows")
    return [row for row in rows if isinstance(row, dict)] if isinstance(rows, list) else []


def _assignment_warehouse_ids(payload: dict[str, object]) -> set[str]:
    return {
        str(row["warehouse_id"])
        for row in _assignment_rows(payload)
        if isinstance(row.get("warehouse_id"), str)
    }


def render_legacy_network_comparison_map(
    network_case_ref: ArtifactRef | ResourceRef,
    baseline_ref: ArtifactRef,
    candidate_ref: ArtifactRef,
) -> CallToolResult:
    case = _case_from_input_ref(network_case_ref)
    baseline = _load_artifact(baseline_ref)
    candidate = _load_artifact(candidate_ref)
    baseline_warehouses = _assignment_warehouse_ids(baseline)
    candidate_warehouses = _assignment_warehouse_ids(candidate)
    warehouse_by_id = {warehouse.warehouse_id: warehouse for warehouse in case.warehouses}
    demand_by_id = {demand.city_id: demand for demand in case.demand}
    features: list[dict[str, object]] = []
    for warehouse in case.warehouses:
        features.append(
            {
                "type": "Feature",
                "geometry": {
                    "type": "Point",
                    "coordinates": [warehouse.longitude, warehouse.latitude],
                },
                "properties": {
                    "kind": "warehouse",
                    "warehouse_id": warehouse.warehouse_id,
                    "name": warehouse.warehouse_name,
                    "warehouse_type": warehouse.warehouse_type,
                    "baseline_active": warehouse.warehouse_id in baseline_warehouses,
                    "candidate_active": warehouse.warehouse_id in candidate_warehouses,
                },
            }
        )
    for demand in case.demand:
        features.append(
            {
                "type": "Feature",
                "geometry": {"type": "Point", "coordinates": [demand.longitude, demand.latitude]},
                "properties": {
                    "kind": "demand",
                    "city_id": demand.city_id,
                    "city_name": demand.city_name,
                    "demand_quantity": demand.demand_quantity,
                },
            }
        )
    for scenario_name, payload in (("baseline", baseline), ("candidate", candidate)):
        for row in _assignment_rows(payload):
            warehouse = warehouse_by_id.get(str(row.get("warehouse_id")))
            demand = demand_by_id.get(str(row.get("demand_city_id")))
            if warehouse is None or demand is None:
                continue
            features.append(
                {
                    "type": "Feature",
                    "geometry": {
                        "type": "LineString",
                        "coordinates": [
                            [warehouse.longitude, warehouse.latitude],
                            [demand.longitude, demand.latitude],
                        ],
                    },
                    "properties": {
                        "kind": "assignment",
                        "scenario": scenario_name,
                        "warehouse_id": warehouse.warehouse_id,
                        "demand_city_id": demand.city_id,
                        "demand_quantity": demand.demand_quantity,
                    },
                }
            )
    geojson = {"type": "FeatureCollection", "features": features}
    geojson_published = _store().publish("geojson.v1", geojson)
    map_payload = {
        "schema_version": "network_comparison_map.v1",
        "baseline_ref": baseline_ref.model_dump(mode="json"),
        "candidate_ref": candidate_ref.model_dump(mode="json"),
        "geojson_resource_name": geojson_published.resource_id,
        "geojson_ref": _artifact_ref(geojson_published).model_dump(mode="json"),
        "layers": ["warehouses", "demand", "baseline", "candidate"],
        "feature_count": len(features),
    }
    return _publish_new_resource(
        "network_comparison_map.v1",
        map_payload,
        "Prepared a map resource for comparing the two network scenarios.",
    )


def _legacy_render_network_comparison_map(
    case_id: str,
    candidate_source: Literal["scenario", "facility_location"],
    ctx: Context,
) -> CallToolResult:
    """Publish a deterministic baseline-versus-candidate GeoJSON Artifact."""
    try:
        parsed = UUID(case_id)
        published, summary, result = NetworkMapService(_cases(), _store()).publish_comparison(
            parsed, _workspace(ctx), candidate_source
        )
        case = _cases().get_case(parsed, _workspace(ctx))
        status = _cases().get_status(parsed, _workspace(ctx))
    except (CaseRepositoryError, ValueError) as error:
        return _case_error_result(
            error
            if isinstance(error, CaseRepositoryError)
            else CaseRepositoryError("comparison_map_invalid", str(error))
        )
    facet = next(item for item in status.facets if item.name.value == "map")
    artifact_ref = f"network-map-{published.resource_id.rsplit('-', maxsplit=1)[-1]}"
    embed_code = f'::codex-inline-vis{{artifact="{artifact_ref}"}}'
    inline_visualization = {
        "type": "open-web-artifact",
        "kind": "inline-visualization.v1",
        "artifact": {
            "ref": artifact_ref,
            "renderer": {
                "kind": "map.v3",
                "payload": {
                    "title": "仓网方案对比",
                    "intent": "比较基线与候选仓网方案",
                    "status": "ready",
                    "summary": "展示需求城市、仓库和两套覆盖关系。",
                    "center": [118.0, -2.0],
                    "zoom": 3.2,
                    "sources": {
                        "network": {
                            "type": "geojson",
                            "data": {
                                "type": "mcp_resource",
                                "server": MCP_SERVER_NAME,
                                "uri": published.uri,
                                "resource_schema": "geojson.v1",
                            },
                        }
                    },
                    "layers": [
                        {
                            "id": "baseline-assignments",
                            "source": "network",
                            "type": "line",
                            "filter": [
                                "all",
                                ["==", ["get", "kind"], "assignment"],
                                ["==", ["get", "scenario"], "baseline"],
                            ],
                            "paint": {
                                "line-color": "#64748b",
                                "line-opacity": 0.28,
                                "line-width": 1,
                            },
                        },
                        {
                            "id": "candidate-assignments",
                            "source": "network",
                            "type": "line",
                            "filter": [
                                "all",
                                ["==", ["get", "kind"], "assignment"],
                                ["==", ["get", "scenario"], "candidate"],
                            ],
                            "paint": {
                                "line-color": "#e76f51",
                                "line-opacity": 0.46,
                                "line-width": 1.4,
                            },
                        },
                        {
                            "id": "demand-cities",
                            "source": "network",
                            "type": "circle",
                            "filter": ["==", ["get", "kind"], "demand"],
                            "paint": {
                                "circle-color": "#2a9d8f",
                                "circle-radius": 3,
                                "circle-opacity": 0.72,
                            },
                        },
                        {
                            "id": "warehouses",
                            "source": "network",
                            "type": "circle",
                            "filter": ["==", ["get", "kind"], "warehouse"],
                            "paint": {
                                "circle-color": "#e9c46a",
                                "circle-radius": 6,
                                "circle-stroke-color": "#264653",
                                "circle-stroke-width": 1.5,
                            },
                        },
                    ],
                },
            },
        },
        "embed": {
            "syntax": "codex-inline-vis.artifact.v1",
            "code": embed_code,
        },
    }
    return ToolEnvelopeBuilder().build(
        case=case,
        operation=result.operation,
        summary=(
            f"Published comparison map with {summary['feature_count']} features and "
            f"{summary['candidate_active_warehouse_count']} candidate-plan warehouses."
        ),
        facet_updates=[facet],
        evidence=[
            EvidenceSummary(
                kind="artifact",
                evidence_id=published.resource_id,
                label="Network comparison map",
                value=f"{published.schema}; {published.size} bytes",
            )
        ],
        published_artifacts=[
            {
                "schema": published.schema,
                "name": published.resource_id,
                "mime_type": "application/geo+json",
                "size": published.size,
            }
        ],
        extra_content=[
            ResourceLink(
                type="resource_link",
                name="Network comparison map",
                title="network_comparison_map.v1",
                uri=published.uri,
                description="Deterministic baseline and candidate assignment map.",
                mimeType="application/geo+json",
                size=published.size,
            )
        ],
        inline_visualization=inline_visualization,
    )


def _legacy_publish_network_planning_report(
    case_id: str,
    source: Literal["baseline", "scenario", "facility_location"],
    ctx: Context,
) -> CallToolResult:
    """Publish one bounded Case result as a durable platform Artifact candidate."""
    try:
        parsed = UUID(case_id)
        published, summary, result = NetworkReportService(_cases(), _store()).publish(
            parsed, _workspace(ctx), source
        )
        case = _cases().get_case(parsed, _workspace(ctx))
        status = _cases().get_status(parsed, _workspace(ctx))
    except (CaseRepositoryError, ValueError) as error:
        return _case_error_result(
            error
            if isinstance(error, CaseRepositoryError)
            else CaseRepositoryError("report_publication_invalid", str(error))
        )
    facet = next(item for item in status.facets if item.name.value == "report")
    return ToolEnvelopeBuilder().build(
        case=case,
        operation=result.operation,
        summary=f"Published {source} report with {summary['service_metric_count']} service metrics.",
        facet_updates=[facet],
        evidence=[
            EvidenceSummary(
                kind="artifact",
                evidence_id=published.resource_id,
                label="Network planning report",
                value=f"{published.schema}; {published.size} bytes",
            )
        ],
        published_artifacts=[
            {
                "schema": published.schema,
                "name": published.resource_id,
                "mime_type": "application/json",
                "size": published.size,
            }
        ],
        next_action=None,
        extra_content=[
            ResourceLink(
                type="resource_link",
                name="Network planning report",
                title="network_planning_report.v1",
                uri=published.uri,
                description="Bounded user-facing network planning result.",
                mimeType="application/json",
                size=published.size,
            )
        ],
    )


def create_network_case(
    country_code: str,
    intent: str,
    ctx: Context,
) -> CallToolResult:
    """Create the authoritative case handle used by all planning Agents."""
    try:
        case = _cases().create_case(_workspace(ctx), country_code, intent)
    except CaseRepositoryError as error:
        return _case_error_result(error)
    return ToolEnvelopeBuilder().build(
        case=case,
        summary=f"Created Network Case {str(case.case_id)[:8]} for {case.country_code}.",
        next_action="define_requirements_or_refresh_sources",
    )


def get_network_case_status(
    case_id: str,
    requested_analysis: list[AnalysisKind],
    ctx: Context,
) -> CallToolResult:
    """Return bounded readiness and next actions without exposing business rows."""
    try:
        status = _cases().get_status(UUID(case_id), _workspace(ctx))
        evaluated = ReadinessEvaluator().evaluate(status, requested_analysis)
        case = _cases().get_case(UUID(case_id), _workspace(ctx))
    except (CaseRepositoryError, ValueError) as error:
        if isinstance(error, CaseRepositoryError):
            return _case_error_result(error)
        return _case_error_result(CaseRepositoryError("case_id_invalid", "Case id is invalid."))
    issues = [
        SafeIssue(
            code=question.code,
            severity="warning",
            business_message=question.business_message,
        )
        for question in evaluated.blocking_questions
    ]
    return ToolEnvelopeBuilder().build(
        case=case,
        summary=(
            f"Network Case {str(case.case_id)[:8]} is revision {case.revision}; "
            f"{len(evaluated.available_actions)} action(s) are currently available."
        ),
        facet_updates=evaluated.facets,
        issues=issues,
        next_action=(evaluated.available_actions[0] if evaluated.available_actions else None),
    )


def archive_network_case(case_id: str, ctx: Context) -> CallToolResult:
    """Archive a case after every running operation is terminal."""
    try:
        parsed = UUID(case_id)
        case = _cases().get_case(parsed, _workspace(ctx))
        _cases().archive_case(parsed, _workspace(ctx))
    except (CaseRepositoryError, ValueError) as error:
        if isinstance(error, CaseRepositoryError):
            return _case_error_result(error)
        return _case_error_result(CaseRepositoryError("case_id_invalid", "Case id is invalid."))
    return ToolEnvelopeBuilder().build(
        case=case.model_copy(update={"state": CaseState.ARCHIVED}),
        summary=f"Archived Network Case {str(case.case_id)[:8]}.",
    )


def refresh_case_sources(case_id: str, ctx: Context) -> CallToolResult:
    """Refresh CSV, JSON and XLSX inventory for a Network Case."""
    try:
        parsed = UUID(case_id)
        sources, changed = SourceInventoryService(_cases()).refresh(parsed, _workspace(ctx))
        case = _cases().get_case(parsed, _workspace(ctx))
        status = _cases().get_status(parsed, _workspace(ctx))
    except (CaseRepositoryError, ValueError) as error:
        if isinstance(error, CaseRepositoryError):
            return _case_error_result(error)
        return _case_error_result(CaseRepositoryError("case_id_invalid", "Case id is invalid."))
    source_facet = next(item for item in status.facets if item.name.value == "sources")
    evidence = [
        EvidenceSummary(
            kind="source",
            evidence_id=str(source.source_id),
            label=source.display_name,
            value=source.byte_size,
        )
        for source in sources
    ]
    return ToolEnvelopeBuilder().build(
        case=case,
        summary=(
            f"Found {len(sources)} supported Workspace source(s); "
            f"the inventory {'changed' if changed else 'is unchanged'}."
        ),
        facet_updates=[source_facet],
        evidence=evidence,
        issues=(
            []
            if sources
            else [
                SafeIssue(
                    code="workspace_sources_missing",
                    severity="warning",
                    business_message="没有发现 CSV、JSON 或 XLSX 业务文件。",
                )
            ]
        ),
        next_action="inspect_case_sources" if sources else None,
    )


def inspect_case_sources(
    case_id: str,
    source_ids: list[str],
    ctx: Context,
) -> CallToolResult:
    """Inspect bounded source structure without returning complete business rows."""
    try:
        parsed = UUID(case_id)
        inspections = SourceInventoryService(_cases()).inspect(
            parsed, _workspace(ctx), [UUID(value) for value in source_ids]
        )
        case = _cases().get_case(parsed, _workspace(ctx))
    except (CaseRepositoryError, ValueError) as error:
        if isinstance(error, CaseRepositoryError):
            return _case_error_result(error)
        return _case_error_result(
            CaseRepositoryError("source_selection_invalid", "Source selection is invalid.")
        )
    evidence: list[EvidenceSummary] = []
    for inspection in inspections:
        evidence.append(
            EvidenceSummary(
                kind="source",
                evidence_id=str(inspection.source.source_id),
                label=inspection.source.display_name,
                value=inspection.record_count,
            )
        )
        evidence.extend(
            EvidenceSummary(
                kind="field",
                evidence_id=f"{inspection.source.source_id}:{field.field_name}"[:128],
                label=field.field_name,
                value=field.inferred_type,
            )
            for field in inspection.fields
        )
    return ToolEnvelopeBuilder().build(
        case=case,
        summary=f"Inspected {len(inspections)} active Case source(s).",
        evidence=evidence[:100],
        next_action="propose_case_mapping",
    )


def propose_case_mapping(case_id: str, ctx: Context) -> CallToolResult:
    """Produce deterministic, persisted mapping candidates for active Case sources."""
    try:
        parsed = UUID(case_id)
        proposal, result = CaseMappingService(_cases()).propose(parsed, _workspace(ctx))
        case = _cases().get_case(parsed, _workspace(ctx))
        status = _cases().get_status(parsed, _workspace(ctx))
    except (CaseRepositoryError, ValueError) as error:
        if isinstance(error, CaseRepositoryError):
            return _case_error_result(error)
        return _case_error_result(
            CaseRepositoryError("mapping_proposal_failed", "Source mapping could not be proposed.")
        )
    evidence = [
        EvidenceSummary(
            kind="mapping_candidate",
            evidence_id=candidate.candidate_id,
            label=(
                f"{role.source_name}: {candidate.source_field} -> "
                f"{candidate.target_entity}.{candidate.target_field}"
            ),
            value=candidate.transform.kind.value,
        )
        for role in proposal.proposals
        for candidate in role.field_candidates
    ]
    ambiguous = [role for role in proposal.proposals if role.ambiguous]
    mapping_facet = next(item for item in status.facets if item.name.value == "mapping")
    return ToolEnvelopeBuilder().build(
        case=case,
        operation=result.operation,
        summary=(
            f"Proposed {len(evidence)} field mappings across "
            f"{len(proposal.proposals)} source role candidate(s)."
        ),
        facet_updates=[mapping_facet],
        evidence=evidence[:100],
        issues=[
            SafeIssue(
                code="mapping_role_ambiguous",
                severity="warning",
                business_message=(f"{role.source_name} 可能对应多个业务数据角色，需要确认。"),
            )
            for role in ambiguous[:20]
        ],
        next_action="request_mapping_confirmation" if ambiguous else "apply_case_mapping",
    )


def apply_case_mapping(
    case_id: str,
    candidate_ids: list[str],
    ctx: Context,
) -> CallToolResult:
    """Apply only previously persisted mapping candidate IDs."""
    try:
        parsed = UUID(case_id)
        component = CaseMappingService(_cases()).apply(parsed, _workspace(ctx), candidate_ids)
        case = _cases().get_case(parsed, _workspace(ctx))
        status = _cases().get_status(parsed, _workspace(ctx))
    except (CaseRepositoryError, ValueError) as error:
        if isinstance(error, CaseRepositoryError):
            return _case_error_result(error)
        return _case_error_result(
            CaseRepositoryError("mapping_selection_invalid", "Mapping selection is invalid.")
        )
    mapping_facet = next(item for item in status.facets if item.name.value == "mapping")
    return ToolEnvelopeBuilder().build(
        case=case,
        summary=f"Applied {component.row_count} persisted mapping candidate(s).",
        facet_updates=[mapping_facet],
        next_action="normalize_case_input",
    )


def normalize_case_input(case_id: str, ctx: Context) -> CallToolResult:
    """Normalize full source rows server-side using the persisted mapping selection."""
    try:
        parsed = UUID(case_id)
        batch, result = NormalizationService(_cases()).normalize(parsed, _workspace(ctx))
        case = _cases().get_case(parsed, _workspace(ctx))
        status = _cases().get_status(parsed, _workspace(ctx))
    except (CaseRepositoryError, ValueError) as error:
        if isinstance(error, CaseRepositoryError):
            return _case_error_result(error)
        return _case_error_result(
            CaseRepositoryError("normalization_failed", "Network input normalization failed.")
        )
    facets = [
        facet
        for facet in status.facets
        if facet.name.value in {"normalized_input", "current_assignment", "geography"}
    ]
    issues = [
        SafeIssue(
            code=issue.code,
            severity=issue.severity,
            business_message=issue.business_message,
        )
        for issue in batch.issues
    ]
    counts = {
        "demand": len(batch.demand_cities),
        "warehouses": len(batch.warehouses),
        "current_assignments": len(batch.current_assignments),
        "route_quotes": len(batch.route_quotes),
    }
    return ToolEnvelopeBuilder().build(
        case=case,
        operation=result.operation if result else None,
        summary=(
            "Normalized Case input: "
            + ", ".join(f"{name}={count}" for name, count in counts.items())
            + "."
        ),
        facet_updates=facets,
        issues=issues,
        evidence=[
            EvidenceSummary(
                kind="metric",
                evidence_id=f"normalized:{name}",
                label=name,
                value=count,
            )
            for name, count in counts.items()
        ],
        next_action="resolve_case_geography"
        if any(facet.name.value == "geography" and facet.state.value != "ready" for facet in facets)
        else "plan_route_matrix",
    )


def define_network_requirements(
    case_id: str,
    requested_analyses: list[AnalysisKind],
    assignment_objective: Literal["min_time", "min_cost"] | None,
    service_target_hours: list[float],
    driver_hours_per_day: float | None,
    ctx: Context,
) -> CallToolResult:
    """Persist the business requirements for the current question."""
    try:
        parsed = UUID(case_id)
        request = RequirementRequest(
            requested_analyses=requested_analyses,
            assignment_objective=assignment_objective,
            service_target_hours=service_target_hours,
            driver_hours_per_day=driver_hours_per_day,
        )
        profile, result = RequirementService(_cases()).define(parsed, _workspace(ctx), request)
        case = _cases().get_case(parsed, _workspace(ctx))
        status = _cases().get_status(parsed, _workspace(ctx))
    except (CaseRepositoryError, ValueError, ValidationError) as error:
        if isinstance(error, CaseRepositoryError):
            return _case_error_result(error)
        return _case_error_result(
            CaseRepositoryError("requirements_invalid", "Network requirements are invalid.")
        )
    facet = next(item for item in status.facets if item.name.value == "requirements")
    evidence = [
        EvidenceSummary(
            kind="parameter",
            evidence_id=f"required:{entity}",
            label=entity,
            value=True,
        )
        for entity in profile.required_entities
    ]
    return ToolEnvelopeBuilder().build(
        case=case,
        operation=result.operation,
        summary=(
            f"Defined {len(profile.required_entities)} required and "
            f"{len(profile.optional_entities)} optional business data entities."
        ),
        facet_updates=[facet],
        evidence=evidence,
        next_action="refresh_case_sources",
    )


def _resource_case_records(
    network_case_ref: ArtifactRef | ResourceRef,
) -> tuple[NetworkCase, list[DemandCityRecord], list[WarehouseRecord]]:
    """Load bounded domain records from one immutable normalized-input resource."""
    case = _case_from_input_ref(network_case_ref)
    demand = [
        DemandCityRecord(
            city_id=row.city_id,
            city_name=row.city_name,
            province_id=row.province_id,
            province_name=row.province_name,
            demand_quantity=Decimal(str(row.demand_quantity)),
            longitude=row.longitude,
            latitude=row.latitude,
        )
        for row in case.demand
    ]
    warehouses = [
        WarehouseRecord(
            warehouse_id=row.warehouse_id,
            warehouse_name=row.warehouse_name,
            warehouse_type=row.warehouse_type,
            city_id=row.city_id,
            city_name=row.city_name,
            longitude=row.longitude,
            latitude=row.latitude,
            upstream_center_id=row.upstream_center_id,
            is_existing=row.is_existing,
            is_fixed=row.is_fixed,
        )
        for row in case.warehouses
    ]
    return case, demand, warehouses


def _load_ready_network(resource_ref: ResourceRef) -> PreparedNetworkResource:
    prepared = _runtime().load_model(
        resource_ref,
        "normalized_network_input.v1",
        PreparedNetworkResource,
    )
    if prepared.state != "ready":
        raise McpResourceContractError("normalized_input_not_ready")
    return prepared


@mcp.tool(structured_output=True, annotations=CONTENT_ADDRESSED_RESOURCE_TOOL)
def prepare_network_distribution_map(
    normalized_input_ref: ResourceRef,
    ctx: Context,
    include_candidates: bool = False,
) -> CallToolResult:
    """Publish map points and exact map_utils.create_map_card handoff arguments."""
    _runtime().require_workspace(ctx)
    prepared = _load_ready_network(normalized_input_ref)
    normalized = NormalizedInputBatch(
        demand_cities=prepared.demand_cities,
        warehouses=prepared.warehouses,
        current_assignments=prepared.current_assignments,
        route_quotes=prepared.route_quotes,
        provided_route_facts=prepared.provided_route_facts,
        issues=prepared.issues,
    )
    geojson = build_network_distribution_geojson(
        normalized,
        include_candidates=include_candidates,
    )
    demand_count = len(prepared.demand_cities)
    existing_count = sum(warehouse.is_existing for warehouse in prepared.warehouses)
    candidate_count = (
        sum(not warehouse.is_existing for warehouse in prepared.warehouses)
        if include_candidates
        else 0
    )
    summary = (
        f"Prepared interactive map data with {demand_count} demand cities, "
        f"{existing_count} existing warehouses, and {candidate_count} candidates."
    )
    result = _runtime().publish_geojson(geojson.schema_version, geojson, summary)
    structured = result.structuredContent
    if structured is None:
        raise McpResourceContractError("map_data_result_missing")
    structured.update(
        {
            "feature_count": len(geojson.features),
            "layer_counts": {
                "demand": demand_count,
                "existing_warehouses": existing_count,
                "candidate_warehouses": candidate_count,
            },
            "map_card_handoff": build_network_distribution_map_card_handoff(
                MapResourceRef.model_validate(structured["data_ref"]),
                include_candidates=include_candidates,
            ).model_dump(mode="json", by_alias=True),
        }
    )
    return result


@mcp.tool(structured_output=True, annotations=CONTENT_ADDRESSED_RESOURCE_TOOL)
def plan_route_matrix(
    normalized_input_ref: ResourceRef,
    route_method: Literal["haversine", "navigation", "provided"],
    ctx: Context,
    detour_coefficient: float | None = None,
    average_speed_kph: float | None = None,
) -> CallToolResult:
    """Plan required layered route pairs without persisting workflow state."""
    _runtime().require_workspace(ctx)
    prepared = _load_ready_network(normalized_input_ref)
    plan = _plan_composable_route_matrix(
        prepared.demand_cities,
        prepared.warehouses,
        route_method,
        detour_coefficient,
        average_speed_kph,
    )
    return _runtime().publish(
        plan.schema_version,
        plan,
        f"Planned {plan.route_count} layered routes using {plan.method}; "
        f"estimated billable navigation calls: {plan.estimated_billable_calls}; "
        f"provided route facts available: {len(prepared.provided_route_facts)}.",
    )


@mcp.tool(structured_output=True, annotations=CONTENT_ADDRESSED_RESOURCE_TOOL)
def build_haversine_route_matrix(
    normalized_input_ref: ResourceRef,
    detour_coefficient: float,
    average_speed_kph: float,
    ctx: Context,
    prior_route_matrix_ref: ResourceRef | None = None,
) -> CallToolResult:
    """Build missing haversine facts and reuse only exact prior pair facts."""
    _runtime().require_workspace(ctx)
    prepared = _load_ready_network(normalized_input_ref)
    prior = (
        _runtime().load_model(
            prior_route_matrix_ref,
            "route_matrix.v2",
            ComposableRouteMatrix,
        )
        if prior_route_matrix_ref is not None
        else None
    )
    matrix = build_route_matrix_with_reuse(
        prepared.demand_cities,
        prepared.warehouses,
        prior.rows if prior is not None else [],
        detour_coefficient,
        average_speed_kph,
    )
    validation = matrix.validation
    return _runtime().publish(
        matrix.schema_version,
        matrix,
        "Built route matrix with "
        f"{validation['reused_pair_count']} reused, "
        f"{validation['computed_pair_count']} computed, and "
        f"{validation['missing_pair_count']} missing pairs.",
    )


@mcp.tool(structured_output=True, annotations=CONTENT_ADDRESSED_RESOURCE_TOOL)
def build_provided_route_matrix(
    normalized_input_ref: ResourceRef,
    warehouse_scope: Literal["existing_only", "all_warehouses"],
    ctx: Context,
) -> CallToolResult:
    """Materialize uploaded distance and duration facts for one warehouse scope."""
    _runtime().require_workspace(ctx)
    prepared = _load_ready_network(normalized_input_ref)
    warehouses = prepared.warehouses
    if warehouse_scope == "existing_only":
        warehouses = [warehouse for warehouse in warehouses if warehouse.is_existing]
    matrix = _build_provided_route_matrix(
        prepared.demand_cities,
        warehouses,
        prepared.provided_route_facts,
    )
    validation = matrix.validation
    return _runtime().publish(
        matrix.schema_version,
        matrix,
        f"Built provided route matrix for {warehouse_scope} with "
        f"{validation['provided_pair_count']} supplied, "
        f"{validation['missing_pair_count']} missing, and "
        f"{validation['ignored_input_pair_count']} out-of-scope pair facts.",
    )


@mcp.tool(structured_output=True, annotations=CONTENT_ADDRESSED_RESOURCE_TOOL)
def validate_route_matrix(
    normalized_input_ref: ResourceRef,
    route_matrix_ref: ResourceRef,
    ctx: Context,
) -> CallToolResult:
    """Validate completeness and uniqueness against the normalized network."""
    _runtime().require_workspace(ctx)
    prepared = _load_ready_network(normalized_input_ref)
    matrix = _runtime().load_model(
        route_matrix_ref,
        "route_matrix.v2",
        ComposableRouteMatrix,
    )
    validation = _validate_route_matrix_model(
        prepared.demand_cities,
        prepared.warehouses,
        matrix,
    )
    payload = {
        "schemaVersion": "route_matrix_validation.v1",
        **{key: value for key, value in validation.items() if key != "schema"},
    }
    return _runtime().publish(
        "route_matrix_validation.v1",
        payload,
        f"Route matrix validation {'passed' if validation['valid'] else 'failed'} "
        f"for {validation['route_count']} routes.",
    )


@mcp.tool(structured_output=True, annotations=CONTENT_ADDRESSED_RESOURCE_TOOL)
def register_navigation_route_matrix(
    normalized_input_ref: ResourceRef,
    navigation_result_relative_path: Annotated[
        str,
        Field(min_length=1, max_length=1024),
    ],
    ctx: Context,
    prior_route_matrix_ref: ResourceRef | None = None,
) -> CallToolResult:
    """Register navigation facts from one validated Workspace-relative JSON file."""
    workspace = _runtime().require_workspace(ctx)
    prepared = _load_ready_network(normalized_input_ref)
    document = read_json_document(workspace, navigation_result_relative_path)
    try:
        supplied = ComposableRouteMatrix.model_validate(document)
    except ValidationError as error:
        raise McpResourceContractError("navigation_result_invalid") from error
    if supplied.method != "navigation":
        raise McpResourceContractError("navigation_result_method_mismatch")
    prior = (
        _runtime().load_model(
            prior_route_matrix_ref,
            "route_matrix.v2",
            ComposableRouteMatrix,
        )
        if prior_route_matrix_ref is not None
        else None
    )
    prior_rows = prior.rows if prior is not None else []
    matrix = _register_composable_navigation_matrix(
        prepared.demand_cities,
        prepared.warehouses,
        [*prior_rows, *supplied.rows],
    )
    if matrix.missing_routes:
        raise McpResourceContractError("navigation_matrix_incomplete")
    matrix = matrix.model_copy(
        update={
            "validation": {
                **matrix.validation,
                "reused_pair_count": len(prior_rows),
                "registered_pair_count": len(supplied.rows),
                "missing_pair_count": 0,
            }
        }
    )
    return _runtime().publish(
        matrix.schema_version,
        matrix,
        f"Registered navigation matrix with {len(prior_rows)} reused and "
        f"{len(supplied.rows)} supplied pair facts.",
    )


@mcp.tool(structured_output=True, annotations=CONTENT_ADDRESSED_RESOURCE_TOOL)
def plan_cost_matrix(
    normalized_input_ref: ResourceRef,
    warehouse_scope: Literal["existing_only", "all_warehouses"],
    ctx: Context,
    calculation_policy: CostCalculationPolicy | None = None,
    route_matrix_ref: ResourceRef | None = None,
    prior_cost_matrix_ref: ResourceRef | None = None,
) -> CallToolResult:
    """Build quote-first lane costs with an optional explicit calculation policy."""
    _runtime().require_workspace(ctx)
    prepared = _load_ready_network(normalized_input_ref)
    warehouses = prepared.warehouses
    if warehouse_scope == "existing_only":
        warehouses = [warehouse for warehouse in warehouses if warehouse.is_existing]
    route_matrix = (
        _runtime().load_model(
            route_matrix_ref,
            "route_matrix.v2",
            ComposableRouteMatrix,
        )
        if route_matrix_ref is not None
        else None
    )
    prior = (
        _runtime().load_model(
            prior_cost_matrix_ref,
            "cost_matrix.v2",
            CostMatrix,
        )
        if prior_cost_matrix_ref is not None
        else None
    )
    matrix = _build_composable_cost_matrix(
        prepared.demand_cities,
        warehouses,
        prepared.route_quotes,
        calculation_policy,
        route_matrix,
        prior.rows if prior is not None else None,
        warehouse_scope=warehouse_scope,
    )
    validation = matrix.validation
    return _runtime().publish(
        matrix.schema_version,
        matrix,
        "Built cost matrix with "
        f"{validation['reused_pair_count']} reused, "
        f"{validation['computed_pair_count']} computed, and "
        f"{validation['missing_pair_count']} missing lane costs.",
    )


@mcp.tool(structured_output=True, annotations=CONTENT_ADDRESSED_RESOURCE_TOOL)
def evaluate_network_baseline(
    normalized_input_ref: ResourceRef,
    route_matrix_ref: ResourceRef,
    objective: Literal["min_time", "min_cost"],
    service_targets: Annotated[list[float], Field(min_length=1, max_length=32)],
    coverage_mode: Literal[
        "actual_current", "optimized_existing_footprint"
    ],
    ctx: Context,
    cost_matrix_ref: ResourceRef | None = None,
) -> Annotated[CallToolResult, NetworkBaselineResourceToolResult]:
    """Evaluate one explicitly selected actual or optimized-existing baseline."""
    _runtime().require_workspace(ctx)
    if any(target <= 0 for target in service_targets):
        raise McpResourceContractError("baseline_service_targets_invalid")
    prepared = _load_ready_network(normalized_input_ref)
    routes = _runtime().load_model(
        route_matrix_ref,
        "route_matrix.v2",
        ComposableRouteMatrix,
    )
    costs = (
        _runtime().load_model(cost_matrix_ref, "cost_matrix.v2", CostMatrix)
        if cost_matrix_ref is not None
        else None
    )
    if objective == "min_cost" and costs is None:
        raise McpResourceContractError("min_cost_baseline_requires_cost_matrix")
    active_ids = {
        warehouse.warehouse_id
        for warehouse in prepared.warehouses
        if warehouse.is_existing
    }
    if coverage_mode == "actual_current":
        if not prepared.current_assignments:
            raise McpResourceContractError("current_assignments_required")
        assignment = solve_current_assignment(
            prepared.demand_cities,
            prepared.warehouses,
            prepared.current_assignments,
            routes,
            costs,
            objective,
        )
        label: Literal["actual_current", "optimized_existing_footprint"] = "actual_current"
    else:
        assignment = solve_assignment(
            prepared.demand_cities,
            prepared.warehouses,
            routes,
            costs,
            objective,
            active_ids,
        )
        label = "optimized_existing_footprint"
    ordered_targets = sorted(set(service_targets))
    coverage, detail_target, uncovered = _baseline_coverage_projection(
        assignment,
        prepared.demand_cities,
        prepared.warehouses,
        ordered_targets,
    )
    baseline = BaselineResult(
        label=label,
        active_warehouse_ids=sorted(active_ids),
        assignment=assignment,
        service=service_metrics(assignment, ordered_targets),
        coverage=coverage,
        cost=summarize_assignment_cost(assignment, costs) if costs is not None else None,
        notice_code=None,
    )
    label_text = (
        "actual current assignment"
        if label == "actual_current"
        else "optimized existing-warehouse footprint"
    )
    message = (
        f"Evaluated the {label_text}; active warehouses "
        f"{len(baseline.active_warehouse_ids)}, cost "
        f"{_cost_metric_summary(baseline.cost)}, service "
        f"{_coverage_metric_summary(coverage)}; uncovered at "
        f"{detail_target:g}h: {len(uncovered)} cities."
    )
    result = _runtime().publish(baseline.schema_version, baseline, message)
    if result.structuredContent is None:
        raise McpResourceContractError("baseline_result_missing")
    result.structuredContent.update(
        {
            "coverage_metrics": [
                metric.model_dump(mode="json") for metric in coverage
            ],
            "detail_target_hours": detail_target,
            "uncovered_city_count": len(uncovered),
            "uncovered_cities": [
                item.model_dump(mode="json") for item in uncovered[:100]
            ],
            "uncovered_cities_truncated": len(uncovered) > 100,
        }
    )
    return result


@mcp.tool(structured_output=True, annotations=CONTENT_ADDRESSED_RESOURCE_TOOL)
def evaluate_facility_scenario(
    normalized_input_ref: ResourceRef,
    route_matrix_ref: ResourceRef,
    scenario: ScenarioSpec,
    ctx: Context,
    cost_matrix_ref: ResourceRef | None = None,
) -> CallToolResult:
    """Evaluate an add, remove or relocation scenario without a mutable Case."""
    _runtime().require_workspace(ctx)
    if not scenario.service_targets or any(
        target <= 0 for target in scenario.service_targets
    ):
        raise McpResourceContractError("scenario_service_targets_invalid")
    prepared = _load_ready_network(normalized_input_ref)
    routes = _runtime().load_model(
        route_matrix_ref,
        "route_matrix.v2",
        ComposableRouteMatrix,
    )
    costs = (
        _runtime().load_model(cost_matrix_ref, "cost_matrix.v2", CostMatrix)
        if cost_matrix_ref is not None
        else None
    )
    if scenario.objective == "min_cost" and costs is None:
        raise McpResourceContractError("min_cost_scenario_requires_cost_matrix")
    warehouse_by_id = {
        warehouse.warehouse_id: warehouse for warehouse in prepared.warehouses
    }
    add_ids = set(scenario.add_warehouse_ids)
    remove_ids = set(scenario.remove_warehouse_ids)
    for relocation in scenario.relocations:
        remove_ids.add(relocation.remove_warehouse_id)
        add_ids.add(relocation.add_warehouse_id)
    unknown = (add_ids | remove_ids) - set(warehouse_by_id)
    if unknown:
        raise McpResourceContractError("scenario_unknown_warehouses")
    if any(warehouse_by_id[item].is_existing for item in add_ids):
        raise McpResourceContractError("scenario_add_requires_candidate_warehouse")
    if any(not warehouse_by_id[item].is_existing for item in remove_ids):
        raise McpResourceContractError("scenario_remove_requires_existing_warehouse")
    if add_ids & remove_ids:
        raise McpResourceContractError("scenario_add_remove_conflict")
    active_ids = {
        warehouse.warehouse_id
        for warehouse in prepared.warehouses
        if warehouse.is_existing
    }
    active_ids.difference_update(remove_ids)
    active_ids.update(add_ids)
    invalid_upstreams = sorted(
        warehouse.warehouse_id
        for warehouse in prepared.warehouses
        if warehouse.warehouse_id in active_ids
        and warehouse.upstream_center_id is not None
        and warehouse.upstream_center_id not in active_ids
    )
    if invalid_upstreams:
        raise McpResourceContractError("scenario_active_upstream_required")
    assignment = solve_assignment(
        prepared.demand_cities,
        prepared.warehouses,
        routes,
        costs,
        scenario.objective,
        active_ids,
    )
    result = ScenarioResult(
        active_warehouse_ids=sorted(active_ids),
        assignment=assignment,
        cost=summarize_assignment_cost(assignment, costs) if costs is not None else None,
        service=service_metrics(assignment, sorted(set(scenario.service_targets))),
        warehouse_changes={"added": sorted(add_ids), "removed": sorted(remove_ids)},
    )
    return _runtime().publish(
        result.schema_version,
        result,
        f"Evaluated a scenario with {len(result.active_warehouse_ids)} active warehouses; "
        f"added [{_bounded_id_summary(sorted(add_ids))}], removed "
        f"[{_bounded_id_summary(sorted(remove_ids))}]; cost "
        f"{_cost_metric_summary(result.cost)}, service "
        f"{_service_metric_summary(result.service)}.",
    )


@mcp.tool(structured_output=True, annotations=BOUNDED_LOCAL_COMPUTE_TOOL)
def solve_p_median(
    normalized_input_ref: ResourceRef,
    route_matrix_ref: ResourceRef,
    cost_matrix_ref: ResourceRef,
    number_to_open: Annotated[int, Field(ge=0)],
    fixed_existing_ids: list[str],
    optional_existing_ids: list[str],
    service_targets: Annotated[list[float], Field(min_length=1, max_length=32)],
    time_limit_seconds: Annotated[float, Field(gt=0, le=300)],
    ctx: Context,
    service_constraints: list[ServiceCoverageConstraint] | None = None,
) -> CallToolResult:
    """Solve finite-candidate min-cost p-median under explicit existing-site policy."""
    _runtime().require_workspace(ctx)
    if any(target <= 0 for target in service_targets):
        raise McpResourceContractError("p_median_service_targets_invalid")
    prepared = _load_ready_network(normalized_input_ref)
    routes = _runtime().load_model(
        route_matrix_ref,
        "route_matrix.v2",
        ComposableRouteMatrix,
    )
    costs = _runtime().load_model(
        cost_matrix_ref,
        "cost_matrix.v2",
        CostMatrix,
    )
    route_validation = _validate_route_matrix_model(
        prepared.demand_cities,
        prepared.warehouses,
        routes,
    )
    if not route_validation["valid"]:
        raise McpResourceContractError("p_median_route_matrix_incomplete")
    if costs.warehouse_scope != "all_warehouses" or costs.missing_routes:
        raise McpResourceContractError("p_median_cost_matrix_incomplete")
    constraints = [
        (constraint.target_hours, constraint.minimum_coverage)
        for constraint in service_constraints or []
    ]
    try:
        solved, _branches, timed_out = enumerate_p_median(
            prepared.demand_cities,
            prepared.warehouses,
            routes,
            costs,
            number_to_open,
            set(fixed_existing_ids),
            set(optional_existing_ids),
            time_limit_seconds,
            constraints,
        )
    except SolverUnavailable as error:
        solved = None
        timed_out = False
        unavailable_message = str(error)
    else:
        unavailable_message = None
    if solved is None:
        status = (
            "timeout" if timed_out else ("unavailable" if unavailable_message else "infeasible")
        )
        solution = PMedianSolution(
            status=status,
            active_warehouse_ids=[],
            opened_candidate_ids=[],
            closed_existing_ids=[],
            optimality="not_available",
            message=unavailable_message or "No feasible p-median solution was found.",
        )
    else:
        solution = PMedianSolution(
            status="timeout" if timed_out else "optimal",
            active_warehouse_ids=solved.active_warehouse_ids,
            opened_candidate_ids=solved.opened_candidate_ids,
            closed_existing_ids=solved.closed_existing_ids,
            assignment=solved.assignment,
            objective_value=solved.objective_value,
            cost=summarize_assignment_cost(solved.assignment, costs),
            service=service_metrics(
                solved.assignment,
                sorted(set(service_targets)),
            ),
            optimality="feasible_only" if timed_out else "proven",
            message="The solver returned a feasible solution before the time limit."
            if timed_out
            else None,
        )
    return _runtime().publish(
        solution.schema_version,
        solution,
        f"p-median status is {solution.status}; active warehouses "
        f"{len(solution.active_warehouse_ids)}, opened "
        f"[{_bounded_id_summary(solution.opened_candidate_ids)}], closed "
        f"[{_bounded_id_summary(solution.closed_existing_ids)}]; cost "
        f"{_cost_metric_summary(solution.cost)}, service "
        f"{_service_metric_summary(solution.service)}.",
    )


def _legacy_solve_service_constrained_location_resource(
    network_case_ref: ArtifactRef | ResourceRef,
    route_matrix_ref: ArtifactRef,
    cost_matrix_ref: ArtifactRef,
    number_to_open: int,
    service_target_hours: float,
    minimum_coverage: float,
    time_limit_seconds: float = 30,
) -> CallToolResult:
    """Solve p-median with a demand-weighted service coverage constraint."""
    case, _demand, _warehouses = _resource_case_records(network_case_ref)
    routes = _load_new_model(route_matrix_ref, ComposableRouteMatrix)
    costs = _load_new_model(cost_matrix_ref, CostMatrix)
    try:
        solved, _branches, timed_out = enumerate_p_median(
            case.demand,
            case.warehouses,
            routes,
            costs,
            number_to_open,
            set(),
            set(),
            time_limit_seconds,
            [(service_target_hours, minimum_coverage)],
        )
    except SolverUnavailable as error:
        solution = ServiceConstrainedSolution(
            status="unavailable",
            selected_warehouse_ids=[],
            optimality="not_available",
        )
        message = str(error)
    else:
        if solved is None:
            solution = ServiceConstrainedSolution(
                status="timeout" if timed_out else "infeasible",
                selected_warehouse_ids=[],
                optimality="feasible_only" if timed_out else "not_available",
            )
        else:
            value, active_ids, assignment = solved
            solution = ServiceConstrainedSolution(
                status="timeout" if timed_out else "optimal",
                selected_warehouse_ids=sorted(active_ids),
                cost=value,
                service=service_metrics(assignment, [service_target_hours]),
                optimality="feasible_only" if timed_out else "proven",
            )
        message = (
            f"Service-constrained location status is {solution.status}; "
            f"target coverage is {minimum_coverage:.3f}."
        )
    return _publish_new_resource(solution.schema_version, solution, message)


def _load_final_delivery_inputs(
    normalized_input_ref: ResourceRef,
    baseline_ref: ResourceRef,
    facility_location_ref: ResourceRef,
    comparison_ref: ResourceRef,
) -> tuple[
    PreparedNetworkResource,
    NormalizedInputBatch,
    BaselineResult,
    PMedianSolution,
    AssignmentComparison,
]:
    prepared = _load_ready_network(normalized_input_ref)
    baseline = _runtime().load_model(
        baseline_ref,
        "network_baseline.v2",
        BaselineResult,
    )
    facility = _runtime().load_model(
        facility_location_ref,
        "facility_location_solution.v3",
        PMedianSolution,
    )
    comparison = _runtime().load_model(
        comparison_ref,
        "network_assignment_comparison.v1",
        AssignmentComparison,
    )
    normalized = NormalizedInputBatch(
        demand_cities=prepared.demand_cities,
        warehouses=prepared.warehouses,
        current_assignments=prepared.current_assignments,
        route_quotes=prepared.route_quotes,
        provided_route_facts=prepared.provided_route_facts,
        issues=prepared.issues,
    )
    return prepared, normalized, baseline, facility, comparison


def _write_final_delivery_bundle(
    bundle: NetworkComparisonMapBundle | NetworkPlanningReportBundle,
    output_relative_path: str,
    ctx: Context,
    summary: str,
) -> CallToolResult:
    created = _runtime().create_workspace_model(
        ctx,
        output_relative_path,
        bundle,
        max_bytes=MAX_WORKSPACE_FILE_BYTES,
    )
    structured = NetworkFinalArtifactToolResult(
        summary=summary,
        artifact=NetworkFinalArtifactDescriptor(
            schema=bundle.schema_version,
            displayName=bundle.title,
            mimeType="application/json",
            workspaceRelativePath=created.relative_path,
            byteSize=created.byte_size,
        ),
    )
    return CallToolResult(
        content=[TextContent(type="text", text=summary)],
        structuredContent=structured.model_dump(mode="json", by_alias=True),
    )


@mcp.tool(structured_output=True, annotations=FINAL_WORKSPACE_DELIVERY_TOOL)
def render_network_comparison_map(
    normalized_input_ref: ResourceRef,
    baseline_ref: ResourceRef,
    facility_location_ref: ResourceRef,
    comparison_ref: ResourceRef,
    output_relative_path: Annotated[str, Field(min_length=1, max_length=1024)],
    ctx: Context,
) -> CallToolResult:
    """Create a self-contained baseline-versus-facility map JSON file."""
    _runtime().require_workspace(ctx)
    prepared, normalized, baseline, facility, comparison = (
        _load_final_delivery_inputs(
            normalized_input_ref,
            baseline_ref,
            facility_location_ref,
            comparison_ref,
        )
    )
    bundle = build_network_comparison_map_bundle(
        normalized,
        baseline,
        facility,
        comparison,
        country_code=prepared.country_code,
    )
    return _write_final_delivery_bundle(
        bundle,
        output_relative_path,
        ctx,
        "Created the self-contained warehouse network comparison map.",
    )


@mcp.tool(structured_output=True, annotations=FINAL_WORKSPACE_DELIVERY_TOOL)
def publish_network_planning_report(
    normalized_input_ref: ResourceRef,
    baseline_ref: ResourceRef,
    facility_location_ref: ResourceRef,
    comparison_ref: ResourceRef,
    output_relative_path: Annotated[str, Field(min_length=1, max_length=1024)],
    ctx: Context,
) -> CallToolResult:
    """Create a self-contained warehouse network planning report JSON file."""
    _runtime().require_workspace(ctx)
    prepared, normalized, baseline, facility, comparison = (
        _load_final_delivery_inputs(
            normalized_input_ref,
            baseline_ref,
            facility_location_ref,
            comparison_ref,
        )
    )
    bundle = build_network_planning_report_bundle(
        normalized,
        baseline,
        facility,
        comparison,
        country_code=prepared.country_code,
    )
    return _write_final_delivery_bundle(
        bundle,
        output_relative_path,
        ctx,
        "Created the self-contained warehouse network planning report.",
    )


def main() -> None:
    parser = argparse.ArgumentParser(description="Supply-chain network planning MCP server")
    parser.add_argument(
        "--transport",
        choices=("stdio", "streamable-http"),
        default="stdio",
    )
    args = parser.parse_args()

    global _workspace_root, _data_root, _profile_state_root, _mcp_resource_runtime, _case_store
    _workspace_root = Path.cwd().resolve(strict=True)
    _data_root = Path(os.environ.get("SUPPLY_CHAIN_DATA_ROOT", _workspace_root)).resolve()
    _profile_state_root = Path(os.environ.get("CODEX_HOME", _workspace_root / ".codex")).resolve()
    _mcp_resource_runtime = bind_runtime(
        _workspace_root,
        _profile_state_root,
        MCP_SERVER_NAME,
        RESOURCE_URI_PREFIX,
    )
    _case_store = CaseRepository.from_profile(_profile_state_root)
    if args.transport == "stdio":
        asyncio.run(run_stdio())
    else:
        mcp.run(transport=args.transport)


async def run_stdio() -> None:
    initialization_options = mcp._mcp_server.create_initialization_options(
        experimental_capabilities={SANDBOX_STATE_META_CAPABILITY: {}},
    )
    async with stdio_server() as streams:
        await mcp._mcp_server.run(streams[0], streams[1], initialization_options)


if __name__ == "__main__":
    main()
