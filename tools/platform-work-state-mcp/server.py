"""Bounded Work State mutation tools for authorized Domain Agents."""

from __future__ import annotations

import base64
import hashlib
import hmac
import json
import os
import time
import urllib.error
import urllib.request
from uuid import UUID

from mcp.server.fastmcp import FastMCP


_GATE_URL = "OPEN_WEB_CODEX_WORK_STATE_GATE_URL"
_READ_URL = "OPEN_WEB_CODEX_WORK_STATE_READ_URL"
_GATE_KEY = "OPEN_WEB_CODEX_WORK_STATE_GATE_KEY"
_PATH = "/api/internal/work-state/v1/mutate"
_READ_PATH = "/api/internal/work-state/v1/read"
_MAX_INPUTS = 32
_MAX_COMPONENTS = 32
_MAX_BLOCKING_INPUTS = 16
_MAX_DELIVERABLES = 32

mcp = FastMCP("platform_work_state")


def _request(payload: dict[str, object]) -> dict[str, object]:
    run_id = payload.get("runId")
    state_id = payload.get("workStateId")
    try:
        UUID(str(run_id))
        UUID(str(state_id))
    except (ValueError, TypeError) as exc:
        raise ValueError("run_id and work_state_id must be UUIDs") from exc
    url = os.environ.get(_GATE_URL, "").strip()
    key_text = os.environ.get(_GATE_KEY, "").strip()
    if not url or not key_text:
        raise RuntimeError("platform Work State gate is unavailable")
    try:
        key = base64.urlsafe_b64decode(key_text + "=" * (-len(key_text) % 4))
    except ValueError as exc:
        raise RuntimeError("platform Work State gate key is invalid") from exc
    body = json.dumps(payload, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
    timestamp = str(int(time.time()))
    body_hash = hashlib.sha256(body).hexdigest()
    message = f"{timestamp}\nPOST\n{_PATH}\n{body_hash}".encode("utf-8")
    signature = base64.urlsafe_b64encode(
        hmac.new(key, message, hashlib.sha256).digest()
    ).rstrip(b"=").decode("ascii")
    request = urllib.request.Request(
        url,
        data=body,
        headers={
            "Content-Type": "application/json",
            "Accept": "application/json",
            "X-Open-Web-Codex-Work-State-Timestamp": timestamp,
            "X-Open-Web-Codex-Work-State-Signature": signature,
        },
        method="POST",
    )
    try:
        with urllib.request.urlopen(request, timeout=10) as response:
            result = json.loads(response.read().decode("utf-8"))
    except (urllib.error.HTTPError, urllib.error.URLError, TimeoutError) as exc:
        raise RuntimeError("platform Work State mutation failed") from exc
    if not isinstance(result, dict) or result.get("schemaVersion") != "platform-work-state-result.v1":
        raise RuntimeError("platform Work State returned an invalid result")
    return result


def _read_request(run_id: str, work_state_id: str) -> dict[str, object]:
    try:
        UUID(run_id)
        UUID(work_state_id)
    except (ValueError, TypeError) as exc:
        raise ValueError("run_id and work_state_id must be UUIDs") from exc
    url = os.environ.get(_READ_URL, "").strip()
    key_text = os.environ.get(_GATE_KEY, "").strip()
    if not url or not key_text:
        raise RuntimeError("platform Work State read gate is unavailable")
    try:
        key = base64.urlsafe_b64decode(key_text + "=" * (-len(key_text) % 4))
    except ValueError as exc:
        raise RuntimeError("platform Work State gate key is invalid") from exc
    body = json.dumps(
        {"runId": run_id, "workStateId": work_state_id},
        ensure_ascii=False,
        separators=(",", ":"),
    ).encode("utf-8")
    timestamp = str(int(time.time()))
    body_hash = hashlib.sha256(body).hexdigest()
    message = f"{timestamp}\nPOST\n{_READ_PATH}\n{body_hash}".encode("utf-8")
    signature = base64.urlsafe_b64encode(
        hmac.new(key, message, hashlib.sha256).digest()
    ).rstrip(b"=").decode("ascii")
    request = urllib.request.Request(
        url,
        data=body,
        headers={
            "Content-Type": "application/json",
            "Accept": "application/json",
            "X-Open-Web-Codex-Work-State-Timestamp": timestamp,
            "X-Open-Web-Codex-Work-State-Signature": signature,
        },
        method="POST",
    )
    try:
        with urllib.request.urlopen(request, timeout=10) as response:
            result = json.loads(response.read().decode("utf-8"))
    except (urllib.error.HTTPError, urllib.error.URLError, TimeoutError) as exc:
        raise RuntimeError("platform Work State read failed") from exc
    if not isinstance(result, dict) or result.get("schemaVersion") != "platform-work-state-result.v1":
        raise RuntimeError("platform Work State returned an invalid result")
    return result


@mcp.tool()
def begin_work_operation(
    run_id: str,
    work_state_id: str,
    kind: str,
    idempotency_key: str,
    input_references: list[dict[str, object]] | None = None,
) -> dict[str, object]:
    """在当前 Run 的 Work State 中开始一个幂等 Domain Agent 操作。"""

    references = input_references or []
    if len(references) > _MAX_INPUTS:
        raise ValueError("input_references exceeds the platform limit")
    return _request(
        {
            "action": "begin",
            "runId": run_id,
            "workStateId": work_state_id,
            "kind": kind,
            "idempotencyKey": idempotency_key,
            "inputReferences": references,
        }
    )


@mcp.tool()
def apply_work_state_mutation(
    run_id: str,
    work_state_id: str,
    operation_id: str,
    expected_revision: int,
    components: list[dict[str, object]],
    blocking_inputs: list[dict[str, object]] | None = None,
    deliverables: list[dict[str, object]] | None = None,
    summary: str | None = None,
) -> dict[str, object]:
    """以 compare-and-swap 方式提交组件、阻塞输入和交付件引用。"""

    blockers = blocking_inputs or []
    outputs = deliverables or []
    if len(components) > _MAX_COMPONENTS:
        raise ValueError("components exceeds the platform limit")
    if len(blockers) > _MAX_BLOCKING_INPUTS:
        raise ValueError("blocking_inputs exceeds the platform limit")
    if len(outputs) > _MAX_DELIVERABLES:
        raise ValueError("deliverables exceeds the platform limit")
    return _request(
        {
            "action": "apply",
            "runId": run_id,
            "workStateId": work_state_id,
            "operationId": operation_id,
            "mutation": {
                "expectedRevision": expected_revision,
                "components": components,
                "blockingInputs": blockers,
                "deliverables": outputs,
                "summary": summary,
            },
        }
    )


@mcp.tool()
def fail_work_operation(
    run_id: str,
    work_state_id: str,
    operation_id: str,
    status: str,
    failure_code: str | None = None,
    summary: str | None = None,
) -> dict[str, object]:
    """以显式终态结束失败、取消、超时或中断的 Domain Agent 操作。"""

    if status not in {"failed", "rejected", "cancelled", "timeout", "interrupted"}:
        raise ValueError("status must be a non-success terminal operation state")
    return _request(
        {
            "action": "fail",
            "runId": run_id,
            "workStateId": work_state_id,
            "operationId": operation_id,
            "status": status,
            "failureCode": failure_code,
            "summary": summary,
        }
    )


@mcp.tool()
def get_work_state_context(run_id: str, work_state_id: str) -> dict[str, object]:
    """读取当前 Domain Agent 可见的 Work State 组件和有界 Resource 引用。"""

    return _read_request(run_id, work_state_id)


if __name__ == "__main__":
    mcp.run()
