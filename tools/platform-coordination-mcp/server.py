"""Read-only MCP tools for the platform's durable collaboration projection."""

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


_GATE_URL = "OPEN_WEB_CODEX_COORDINATION_GATE_URL"
_GATE_KEY = "OPEN_WEB_CODEX_COORDINATION_GATE_KEY"
_PATH = "/api/internal/coordination/v1/query"
_QUERIES = frozenset(
    {"status", "executions", "work_state", "blocking_inputs", "deliverables"}
)

mcp = FastMCP("platform_coordination")


def _request(run_id: str, query: str) -> dict[str, object]:
    try:
        UUID(run_id)
    except ValueError as exc:
        raise ValueError("run_id must be a UUID") from exc
    if query not in _QUERIES:
        raise ValueError(f"query must be one of: {', '.join(sorted(_QUERIES))}")
    url = os.environ.get(_GATE_URL, "").strip()
    key_text = os.environ.get(_GATE_KEY, "").strip()
    if not url or not key_text:
        raise RuntimeError("platform coordination gate is unavailable")
    try:
        key = base64.urlsafe_b64decode(key_text + "=" * (-len(key_text) % 4))
    except ValueError as exc:
        raise RuntimeError("platform coordination gate key is invalid") from exc
    body = json.dumps(
        {"runId": run_id, "query": query},
        ensure_ascii=False,
        separators=(",", ":"),
    ).encode("utf-8")
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
            "X-Open-Web-Codex-Coordination-Timestamp": timestamp,
            "X-Open-Web-Codex-Coordination-Signature": signature,
        },
        method="POST",
    )
    try:
        with urllib.request.urlopen(request, timeout=10) as response:
            payload = json.loads(response.read().decode("utf-8"))
    except (urllib.error.HTTPError, urllib.error.URLError, TimeoutError) as exc:
        raise RuntimeError("platform coordination query failed") from exc
    if not isinstance(payload, dict):
        raise RuntimeError("platform coordination returned an invalid result")
    return payload


@mcp.tool()
def get_collaboration_status(run_id: str) -> dict[str, object]:
    """读取当前 Run 的 Work State、Agent 执行和交付件摘要。"""

    return _request(run_id, "status")


@mcp.tool()
def list_agent_executions(run_id: str) -> dict[str, object]:
    """读取当前 Run 的 Agent 执行状态，不读取线程正文或思维链。"""

    return _request(run_id, "executions")


@mcp.tool()
def get_work_state_summary(run_id: str) -> dict[str, object]:
    """读取当前 Run 绑定的 Work State 摘要。"""

    return _request(run_id, "work_state")


@mcp.tool()
def list_blocking_inputs(run_id: str) -> dict[str, object]:
    """读取阻塞当前协作的用户输入和参数请求摘要。"""

    return _request(run_id, "blocking_inputs")


@mcp.tool()
def list_deliverables(run_id: str) -> dict[str, object]:
    """读取已登记的交付件引用和状态。"""

    return _request(run_id, "deliverables")


if __name__ == "__main__":
    mcp.run()
