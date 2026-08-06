"""FastMCP entry point for supply-chain network planning."""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
import time
import urllib.error
import urllib.parse
import urllib.request
from datetime import UTC, datetime
from pathlib import Path
from typing import Annotated, Literal

from mcp.server.fastmcp import FastMCP
from mcp.types import (
    CallToolResult,
    EmbeddedResource,
    ResourceLink,
    TextContent,
    TextResourceContents,
)
from pydantic import Field, ValidationError

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
from .models import (
    ComparisonToolResult,
    CurrentCoverageResult,
    CurrentCoverageToolResult,
    DataAgentRef,
    DataRef,
    FacilityLocationSolution,
    FacilityLocationToolResult,
    FinancialEvaluation,
    FinancialEvaluationToolResult,
    MapDataRef,
    NetworkInput,
    NetworkMapRenderToolResult,
    NetworkMapToolResult,
    NetworkPlanningReportToolResult,
    NetworkScenarioResult,
    NetworkSnapshot,
    NetworkSnapshotPreparationToolResult,
    PlanningDataset,
    ResourceToolResult,
    RiskItem,
    RiskRegister,
    RiskRegisterToolResult,
    RouteEntry,
    RouteMatrix,
    ScenarioComparison,
    ValidationResult,
)
from .resource_store import PublishedResource, ResourceStore, data_ref

MAX_SOURCE_BYTES = 20 * 1024 * 1024
MAX_PROFILE_GOAL_CHARS = 8_000

mcp = FastMCP(
    "Supply Chain Network Planner",
    instructions=(
        "Use prepare_network_snapshot before any calculation, then register one immutable "
        "navigation route matrix for that snapshot. Carry every returned data_ref unchanged; "
        "its server is supply_chain_planner and its URI is the durable MCP Resource identity. "
        "Do not describe a result as current coverage unless evaluate_current_coverage was "
        "used: it separates actual assignments from optimized assignments on the same existing "
        "footprint. Scenario and location tools use integer, splittable planning units and an "
        "explicit end-to-end service policy. Location results are exact only over the candidate "
        "facilities contained in the snapshot. Call validate_network_resource before presenting "
        "a decision. Financial evaluation and risk registration are separate optional evidence "
        "steps; use them only when the business question requires those decisions."
    ),
    json_response=True,
)

_workspace_root = Path.cwd().resolve()
_data_root = Path(os.environ.get("SUPPLY_CHAIN_DATA_ROOT", _workspace_root)).resolve()
_profile_state_root = Path(os.environ.get("CODEX_HOME", _workspace_root / ".codex")).resolve()
_resource_store: ResourceStore | None = None
_data_resource_store: ResourceStore | None = None
_CONTRACT_PATH = (
    Path(__file__).resolve().parents[1] / "contracts" / "warehouse-network-planning-1.0.0.json"
)


def _store() -> ResourceStore:
    global _resource_store
    if _resource_store is None:
        resource_root = Path(
            os.environ.get(
                "SUPPLY_CHAIN_RESOURCE_DIR",
                _profile_state_root / "mcp-state" / "supply-chain-network-planner" / "resources",
            )
        ).resolve()
        _resource_store = ResourceStore(resource_root)
    return _resource_store


def _data_store() -> ResourceStore:
    global _data_resource_store
    if _data_resource_store is None:
        resource_root = Path(
            os.environ.get(
                "SUPPLY_CHAIN_DATA_RESOURCE_DIR",
                _profile_state_root / "mcp-state" / "supply-chain-data" / "resources",
            )
        ).resolve()
        _data_resource_store = ResourceStore(
            resource_root,
            uri_prefix="supply-chain-data://resources/",
        )
    return _data_resource_store


def _resource_name(value: object) -> str:
    """Return the exact opaque Resource name carried by a DataRef."""
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


def _validate_evidence_ref(ref: DataRef | DataAgentRef) -> None:
    stores = {
        "supply_chain_data": _data_store,
        "supply_chain_planner": _store,
    }
    store_factory = stores.get(ref.server)
    if store_factory is None:
        raise ValueError(f"unsupported evidence data_ref.server {ref.server!r}")
    payload = store_factory().load_uri(ref.uri)
    if payload.get("schema_version") != ref.resource_schema:
        raise ValueError(
            f"evidence data_ref schema {ref.resource_schema!r} does not match "
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
            "data_requirement_profile.v1",
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


def _load_ref(ref: DataRef, model_type):
    if ref.server != "supply_chain_planner":
        raise ValueError("data_ref.server must be supply_chain_planner")
    payload = _store().load(ref)
    value = model_type.model_validate(payload)
    actual_schema = getattr(value, "schema_version", None)
    if actual_schema != ref.resource_schema:
        raise ValueError(
            f"data_ref schema {ref.resource_schema!r} does not match resource schema "
            f"{actual_schema!r}"
        )
    return value


def _ref_for_resource(resource_name: str) -> DataRef:
    """Resolve a server-owned opaque resource name without accepting a URI."""
    if not resource_name or resource_name != Path(resource_name).name:
        raise ValueError("resource_name must be a single opaque Resource identifier")
    payload = _store().load_uri(f"supply-chain://resources/{resource_name}")
    schema = payload.get("schema_version")
    if not isinstance(schema, str) or not schema:
        raise ValueError("Resource is missing schema_version")
    return DataRef(resource_schema=schema, uri=f"supply-chain://resources/{resource_name}")


def _load_planning_dataset(ref: DataAgentRef) -> PlanningDataset:
    if ref.server != "supply_chain_data" or ref.resource_schema != "planning-dataset.v2":
        raise ValueError("planning_dataset_ref must identify supply_chain_data planning-dataset.v2")
    payload = _data_store().load_uri(ref.uri)
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
    for key in ("profile_confirmation", "mapping", "parameters"):
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


@mcp.tool(structured_output=True)
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
    analysis_mode: Literal["candidate_warehouse_optimization"] = "candidate_warehouse_optimization",
) -> Annotated[CallToolResult, ResourceToolResult]:
    """Create the complete user-confirmable Profile for the network question.

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
        "data_requirement_profile.v1",
        {
            "taskGoal": goal,
            "problemType": analysis_mode,
            "entities": contract["requiredEntities"],
            "requiredEntities": contract["requiredEntities"],
            "parameters": contract["businessParameters"],
            "outputs": outputs,
            "conditionalRequirements": [
                "Route facts are required for quoted/navigation routes; otherwise "
                "coordinates and confirmed estimation parameters are required.",
                "Candidate warehouse optimization requires candidate facilities, "
                "capacity and fixed/opening costs.",
            ],
            "assumptions": [
                "Only files and fields confirmed by the user will enter the planning dataset.",
                "No tutorial defaults are applied.",
            ],
            "exclusions": [
                "real-time navigation",
                "live carrier pricing",
                "unbounded source reads",
            ],
            "inputRequest": {
                "kind": "confirm_profile",
                "prompt": "请确认完整数据需求 Profile；任何字段或参数修改请作为普通消息提出。",
            },
        },
    )
    published = _store().publish("data_requirement_profile.v1", profile_payload)
    summary = (
        "Published the complete candidate-warehouse data requirement profile for user confirmation."
    )
    structured = ResourceToolResult(
        summary=summary,
        resource_name=published.resource_id,
        data_ref=data_ref(published),
    ).model_dump(mode="json")
    return _resource_call_result(published, summary=summary, structured=structured)


def _load_planner_payload(ref: DataRef) -> dict[str, object]:
    if ref.server != "supply_chain_planner":
        raise ValueError("profile_ref must identify a supply_chain_planner Resource")
    payload = _store().load(ref)
    if (
        payload.get("schema_version") not in {ref.resource_schema, None}
        and payload.get("schemaVersion") != ref.resource_schema
    ):
        raise ValueError("profile Resource schema does not match its reference")
    return payload


def _load_data_payload(ref: DataAgentRef, schema: str) -> dict[str, object]:
    if ref.server != "supply_chain_data" or ref.resource_schema != schema:
        raise ValueError(f"expected supply_chain_data {schema} Resource")
    return _data_store().load(ref)


@mcp.tool(structured_output=True)
def publish_input_gap(
    profile_ref: DataRef,
    source_profile_ref: DataAgentRef | None = None,
    mapping_proposal_ref: DataAgentRef | None = None,
    parameter_answers: dict[str, object] | None = None,
    profile_confirmation: dict[str, object] | None = None,
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
    if (profile_confirmation or {}).get("confirmed") is not True:
        gaps.append(
            {
                "code": "confirm_profile",
                "message": "需要用户确认完整的数据需求 Profile。",
            }
        )
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
            for kind in ("confirm_profile", "provide_data", "confirm_mapping", "answer_parameters")
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
        summary=summary, resource_name=published.resource_id, data_ref=data_ref(published)
    ).model_dump(mode="json")
    return _resource_call_result(published, summary=summary, structured=structured)


@mcp.tool(structured_output=True)
def publish_analysis_readiness_review(
    profile_ref: DataRef,
    source_profile_ref: DataAgentRef,
    mapping_proposal_ref: DataAgentRef,
    planning_dataset_ref: DataAgentRef,
    parameter_answers: dict[str, object],
    profile_confirmation: dict[str, object] | None = None,
    mapping_confirmation: dict[str, object] | None = None,
) -> Annotated[CallToolResult, ResourceToolResult]:
    """Publish the final checklist only after the strict Dataset validates."""
    profile = _load_planner_payload(profile_ref)
    source = _load_data_payload(source_profile_ref, "source_profile.v1")
    mapping = _load_data_payload(mapping_proposal_ref, "mapping_proposal.v1")
    dataset = _load_planning_dataset(planning_dataset_ref)
    profile_is_confirmed = (
        profile_confirmation is not None and profile_confirmation.get("confirmed") is True
    )
    if not profile_is_confirmed:
        raise ValueError("profile_confirmation_required")
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
        summary=summary, resource_name=published.resource_id, data_ref=data_ref(published)
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


@mcp.tool(structured_output=True)
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
        data_ref=data_ref(published),
    ).model_dump(mode="json")
    return _resource_call_result(published, summary=summary, structured=structured)


@mcp.tool(structured_output=True)
def prepare_network_snapshot_from_planning_dataset(
    planning_dataset_ref: DataAgentRef,
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
        snapshot_ref=data_ref(snapshot_published),
        route_matrix_resource_name=matrix_published.resource_id,
        route_matrix_ref=data_ref(matrix_published),
    ).model_dump(mode="json")
    return _resource_call_result(
        snapshot_published,
        summary=summary,
        structured=structured,
    )


@mcp.tool(structured_output=True)
def register_route_matrix(
    snapshot_ref: DataRef,
    provider: str,
    method: Literal["navigation", "quoted", "haversine_estimate"],
    entries: list[RouteEntry],
    planning_dataset_ref: DataAgentRef,
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
        data_ref=data_ref(published),
    ).model_dump(mode="json")
    return _resource_call_result(published, summary=summary, structured=structured)


@mcp.tool(structured_output=True)
def evaluate_current_coverage(
    snapshot_ref: DataRef,
    route_matrix_ref: DataRef,
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
    matrix = _load_ref(route_matrix_ref, RouteMatrix)
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
        actual_result_ref=data_ref(actual_published),
        optimized_result_resource_name=optimized_published.resource_id,
        optimized_result_ref=data_ref(optimized_published),
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
        data_ref=data_ref(aggregate_published),
    ).model_dump(mode="json")
    return _resource_call_result(
        aggregate_published,
        summary=summary,
        structured=structured,
    )


@mcp.tool(structured_output=True)
def evaluate_network_scenario(
    snapshot_ref: DataRef,
    route_matrix_ref: DataRef,
    scenario_id: str,
    active_facility_ids: list[str],
    planning_dataset_ref: DataAgentRef,
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
    matrix = _load_ref(route_matrix_ref, RouteMatrix)
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
        data_ref=data_ref(published),
    ).model_dump(mode="json")
    return _resource_call_result(published, summary=summary, structured=structured)


@mcp.tool(structured_output=True)
def compare_network_scenarios(
    baseline_result_ref: DataRef,
    candidate_result_ref: DataRef,
    planning_dataset_ref: str,
    execution_snapshot_id: str | None = None,
    binding_fingerprint: str | None = None,
) -> Annotated[CallToolResult, ComparisonToolResult]:
    """Compare two scenario results built from the same snapshot, routes, and policy."""
    _require_analysis_gate(
        "compare_network_scenarios",
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
        data_ref=data_ref(published),
    ).model_dump(mode="json")
    return _resource_call_result(published, summary=summary, structured=structured)


@mcp.tool(structured_output=True)
def solve_facility_location(
    snapshot_ref: DataRef,
    route_matrix_ref: DataRef,
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
    matrix = _load_ref(route_matrix_ref, RouteMatrix)
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
        result_ref=data_ref(result_published),
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
        solution_ref=data_ref(solution_published),
    ).model_dump(mode="json")
    return _resource_call_result(
        solution_published,
        summary=summary,
        structured=structured,
    )


@mcp.tool(structured_output=True)
def evaluate_financial_case(
    snapshot_ref: DataRef,
    baseline_result_ref: DataRef,
    candidate_result_ref: DataRef,
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
        data_ref=data_ref(published),
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


@mcp.tool(structured_output=True)
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
            "server": "supply_chain_planner",
            "uri": geojson_published.uri,
            "format": "geojson",
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
        map_ref=data_ref(map_published),
        geojson_resource_name=geojson_published.resource_id,
        geojson_ref=MapDataRef(uri=geojson_published.uri),
        feature_count=len(features),
        title=title,
        layers=map_manifest["layers"],
        extensions=map_manifest["extensions"],
    ).model_dump(mode="json")
    return _resource_call_result(map_published, summary=summary, structured=structured)


@mcp.tool(structured_output=True)
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
        geojson_ref=MapDataRef(uri=f"supply-chain://resources/{geojson_resource_name}"),
        title=str(payload.get("title", "Network comparison")),
        layers=payload.get("layers", []),
        extensions=payload.get("extensions", {}),
    )
    return _resource_call_result(
        _store().publish("network_map_render_input.v1", result),
        summary=result.summary,
        structured=result.model_dump(mode="json"),
    )


@mcp.tool(structured_output=True)
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
    report_ref = DataRef(resource_schema="network_planning_report.v1", uri=report_published.uri)
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
        data_ref=report_ref,
        title=title,
        markdown_sha256=digest,
        report_resource_name=artifact_published.resource_id,
        report_ref=data_ref(artifact_published),
    ).model_dump(mode="json")
    return _resource_call_result(report_published, summary=summary, structured=structured)


@mcp.tool(structured_output=True)
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
        data_ref=data_ref(published),
    ).model_dump(mode="json")
    return _resource_call_result(published, summary=summary, structured=structured)


@mcp.tool(structured_output=True)
def validate_network_resource(
    resource_ref: DataRef,
) -> ValidationResult:
    """Validate a snapshot, route matrix, scenario result, comparison, or solution."""
    errors: list[str] = []
    warnings: list[str] = []
    checks: list[str] = []
    payload = _store().load(resource_ref)
    schema = str(payload.get("schema_version", "unknown"))
    model_by_schema = {
        "network_snapshot.v1": NetworkSnapshot,
        "route_matrix.v1": RouteMatrix,
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
            checks.append("report contains a bounded immutable source")
        elif schema == "report.v1":
            if payload.get("status") != "ready" or not isinstance(payload.get("source"), dict):
                errors.append("report delivery does not reference a ready source")
            checks.append("report delivery preserves the source Resource reference")
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
            if isinstance(value, RouteMatrix):
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


def main() -> None:
    parser = argparse.ArgumentParser(description="Supply-chain network planning MCP server")
    parser.add_argument(
        "--workspace-root",
        type=Path,
        default=Path.cwd(),
        help="Plugin root used for default data and Resource directories",
    )
    parser.add_argument(
        "--transport",
        choices=("stdio", "streamable-http"),
        default="stdio",
    )
    args = parser.parse_args()

    global _workspace_root, _data_root, _profile_state_root, _resource_store
    _workspace_root = args.workspace_root.resolve()
    _data_root = Path(os.environ.get("SUPPLY_CHAIN_DATA_ROOT", _workspace_root)).resolve()
    _profile_state_root = Path(os.environ.get("CODEX_HOME", _workspace_root / ".codex")).resolve()
    resource_root = Path(
        os.environ.get(
            "SUPPLY_CHAIN_RESOURCE_DIR",
            _profile_state_root / "mcp-state" / "supply-chain-network-planner" / "resources",
        )
    ).resolve()
    _resource_store = ResourceStore(resource_root)
    mcp.run(transport=args.transport)


if __name__ == "__main__":
    main()
