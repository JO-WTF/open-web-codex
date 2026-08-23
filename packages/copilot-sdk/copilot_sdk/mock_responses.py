"""Deterministic local Responses SSE fixture for native Copilot acceptance."""

from __future__ import annotations

import json
import threading
from dataclasses import dataclass, field
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any


def _event(kind: str, payload: dict[str, Any]) -> str:
    return f"event: {kind}\ndata: {json.dumps(payload, separators=(',', ':'))}\n\n"


def _response(response_id: str, item: dict[str, Any]) -> bytes:
    events = [
        ("response.created", {"type": "response.created", "response": {"id": response_id}}),
        ("response.output_item.done", {"type": "response.output_item.done", "item": item}),
        (
            "response.completed",
            {
                "type": "response.completed",
                "response": {
                    "id": response_id,
                    "usage": {
                        "input_tokens": 0,
                        "input_tokens_details": None,
                        "output_tokens": 0,
                        "output_tokens_details": None,
                        "total_tokens": 0,
                    },
                },
            },
        ),
    ]
    return "".join(_event(kind, payload) for kind, payload in events).encode()


def _call(call_id: str, name: str, arguments: dict[str, Any], *, namespace: str | None = None) -> dict[str, Any]:
    item: dict[str, Any] = {
        "type": "function_call",
        "call_id": call_id,
        "name": name,
        "arguments": json.dumps(arguments, separators=(",", ":")),
    }
    if namespace is not None:
        item["namespace"] = namespace
    return item


def _tool_search_call(call_id: str, query: str) -> dict[str, Any]:
    return {
        "type": "tool_search_call",
        "call_id": call_id,
        "execution": "client",
        "arguments": {"query": query, "limit": 20},
    }


def _message(item_id: str, text: str) -> dict[str, Any]:
    return {
        "type": "message",
        "role": "assistant",
        "id": item_id,
        "content": [{"type": "output_text", "text": text}],
    }


def _has_tool_output(value: Any, call_id: str) -> bool:
    if isinstance(value, dict):
        if value.get("type") in ("function_call_output", "custom_tool_call_output") and value.get(
            "call_id"
        ) == call_id:
            return True
        return any(_has_tool_output(item, call_id) for item in value.values())
    if isinstance(value, list):
        return any(_has_tool_output(item, call_id) for item in value)
    return False


def _has_tool_search_output(value: Any, call_id: str) -> bool:
    if isinstance(value, dict):
        if value.get("type") == "tool_search_output" and value.get("call_id") == call_id:
            return True
        return any(_has_tool_search_output(item, call_id) for item in value.values())
    if isinstance(value, list):
        return any(_has_tool_search_output(item, call_id) for item in value)
    return False


def _spawned_agent_id(value: Any, call_id: str) -> str | None:
    if isinstance(value, dict):
        if value.get("type") in ("function_call_output", "custom_tool_call_output") and value.get(
            "call_id"
        ) == call_id:
            output = value.get("output")
            if isinstance(output, str):
                try:
                    output = json.loads(output)
                except json.JSONDecodeError:
                    return None
            if isinstance(output, dict):
                candidate = output.get("agent_id")
                if isinstance(candidate, str):
                    return candidate
        for child in value.values():
            found = _spawned_agent_id(child, call_id)
            if found is not None:
                return found
    elif isinstance(value, list):
        for child in value:
            found = _spawned_agent_id(child, call_id)
            if found is not None:
                return found
    return None


def _has_child_task_envelope(body: dict[str, Any], task_name: str) -> bool:
    inputs = body.get("input")
    if not isinstance(inputs, list):
        return False
    for item in inputs:
        if not isinstance(item, dict) or item.get("type") != "agent_message":
            continue
        content = item.get("content")
        if not isinstance(content, list):
            continue
        expected = f"Message Type: NEW_TASK\nTask name: /root/{task_name}\nSender: /root\nPayload:\n"
        has_header = any(
            isinstance(part, dict)
            and part.get("type") == "input_text"
            and part.get("text") == expected
            for part in content
        )
        has_encrypted_payload = any(
            isinstance(part, dict) and part.get("type") == "encrypted_content"
            for part in content
        )
        if has_header and has_encrypted_payload:
            return True
    return False


def _has_user_prompt(body: dict[str, Any], prompt: str) -> bool:
    inputs = body.get("input")
    if not isinstance(inputs, list):
        return False
    for item in inputs:
        if not isinstance(item, dict) or item.get("role") != "user":
            continue
        content = item.get("content")
        if not isinstance(content, list):
            continue
        for part in content:
            if (
                isinstance(part, dict)
                and part.get("type") in ("input_text", "text")
                and isinstance(part.get("text"), str)
                and prompt in part["text"]
            ):
                return True
    return False


@dataclass
class MockResponsesFixture:
    target_kind: str
    agent: str | None
    server: str
    tool_name: str
    arguments: dict[str, Any]
    root_prompt: str
    child_prompt: str
    child_task_name: str
    collaboration_namespace: bool = True
    request_classifications: list[dict[str, Any]] = field(default_factory=list)
    _server: ThreadingHTTPServer | None = field(default=None, init=False)
    _thread: threading.Thread | None = field(default=None, init=False)
    _lock: threading.Lock = field(default_factory=threading.Lock, init=False)
    _root_started: bool = field(default=False, init=False)

    @property
    def base_url(self) -> str:
        if self._server is None:
            raise RuntimeError("fixture is not running")
        host, port = self._server.server_address
        return f"http://{host}:{port}/v1"

    def __enter__(self) -> MockResponsesFixture:
        fixture = self

        class Handler(BaseHTTPRequestHandler):
            def do_POST(self) -> None:  # noqa: N802 - stdlib callback name
                length = int(self.headers.get("content-length", "0"))
                body = json.loads(self.rfile.read(length))
                classification = classify_request(
                    body,
                    root_prompt=fixture.root_prompt,
                    child_task_name=fixture.child_task_name,
                )
                with fixture._lock:
                    fixture.request_classifications.append(classification)
                if _has_tool_output(body, "copilot-root-mcp"):
                    output = _response(
                        "copilot-root-final",
                        _message("copilot-root-message", "Root verified the MCP result."),
                    )
                elif _has_tool_output(body, "copilot-child-mcp"):
                    output = _response(
                        "copilot-child-final",
                        _message("copilot-child-message", "Worker verified the MCP result."),
                    )
                elif _has_child_task_envelope(body, fixture.child_task_name) or (
                    not fixture.collaboration_namespace
                    and _has_input_text(body, fixture.child_prompt)
                ):
                    output = _response(
                        "copilot-child-mcp-response",
                        _call(
                            "copilot-child-mcp",
                            fixture.tool_name,
                            fixture.arguments,
                            namespace=f"mcp__{fixture.server}",
                        ),
                    )
                elif _has_tool_output(body, "copilot-root-wait"):
                    output = _response(
                        "copilot-root-final",
                        _message("copilot-root-message", "Supervisor received the worker result."),
                    )
                elif (
                    not fixture.collaboration_namespace
                    and _has_tool_search_output(body, "copilot-root-wait-search")
                ):
                    agent_id = _spawned_agent_id(body, "copilot-root-spawn")
                    output = _response(
                        "copilot-root-wait-response",
                        _call(
                            "copilot-root-wait",
                            "wait_agent",
                            {"targets": [agent_id] if agent_id is not None else []},
                            namespace="multi_agent_v1",
                        ),
                    )
                elif _has_tool_output(body, "copilot-root-spawn"):
                    if not fixture.collaboration_namespace:
                        output = _response(
                            "copilot-root-wait-search-response",
                            _tool_search_call(
                                "copilot-root-wait-search", "wait for a spawned agent"
                            ),
                        )
                    else:
                        output = _response(
                            "copilot-root-wait-response",
                            _call(
                                "copilot-root-wait",
                                "wait_agent",
                                {"timeout_ms": 30_000},
                                namespace="collaboration",
                            ),
                        )
                elif (
                    not fixture.collaboration_namespace
                    and _has_tool_search_output(body, "copilot-root-spawn-search")
                ):
                    output = _response(
                        "copilot-root-spawn-response",
                        _call(
                            "copilot-root-spawn",
                            "spawn_agent",
                            {
                                "message": fixture.child_prompt,
                                "agent_type": fixture.agent,
                                "fork_context": False,
                            },
                            namespace="multi_agent_v1",
                        ),
                    )
                elif not fixture._root_started and _has_user_prompt(body, fixture.root_prompt):
                    with fixture._lock:
                        fixture._root_started = True
                    if fixture.target_kind == "root":
                        output = _response(
                            "copilot-root-mcp-response",
                            _call(
                                "copilot-root-mcp",
                                fixture.tool_name,
                                fixture.arguments,
                                namespace=f"mcp__{fixture.server}",
                            ),
                        )
                    else:
                        assert fixture.agent is not None
                        if fixture.collaboration_namespace:
                            output = _response(
                                "copilot-root-spawn-response",
                                _call(
                                    "copilot-root-spawn",
                                    "spawn_agent",
                                    {
                                        "task_name": fixture.child_task_name,
                                        "message": fixture.child_prompt,
                                        "agent_type": fixture.agent,
                                        "fork_turns": "none",
                                    },
                                    namespace="collaboration",
                                ),
                            )
                        else:
                            output = _response(
                                "copilot-root-spawn-search-response",
                                _tool_search_call(
                                    "copilot-root-spawn-search", "spawn a specialized agent"
                                ),
                            )
                else:
                    output = _response(
                        "copilot-startup-response",
                        _message("copilot-startup-message", "Ready."),
                    )
                self.send_response(200)
                self.send_header("content-type", "text/event-stream")
                self.send_header("content-length", str(len(output)))
                self.end_headers()
                self.wfile.write(output)

            def log_message(self, format: str, *args: object) -> None:
                return

        self._server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self._thread = threading.Thread(target=self._server.serve_forever, daemon=True)
        self._thread.start()
        return self

    def __exit__(self, *args: object) -> None:
        if self._server is not None:
            self._server.shutdown()
            self._server.server_close()
        if self._thread is not None:
            self._thread.join(timeout=2)


def classify_request(
    body: dict[str, Any], *, root_prompt: str, child_task_name: str
) -> dict[str, Any]:
    """Return only bounded routing facts; never retain request content."""

    return {
        "userPrompt": _has_user_prompt(body, root_prompt),
        "child": _has_child_task_envelope(body, child_task_name),
        "inputShape": _input_shape(body),
        "toolOutputCallIds": _tool_output_call_ids(body)[:8],
        "toolOutputs": _tool_output_summaries(body)[:8],
    }


def _input_shape(body: dict[str, Any]) -> list[dict[str, Any]]:
    inputs = body.get("input")
    if not isinstance(inputs, list):
        return []
    shape: list[dict[str, Any]] = []
    for item in inputs[:16]:
        if not isinstance(item, dict):
            shape.append({"type": type(item).__name__})
            continue
        entry: dict[str, Any] = {
            "type": item.get("type"),
            "role": item.get("role"),
        }
        content = item.get("content")
        if isinstance(content, list):
            entry["content"] = [
                {
                    "type": part.get("type") if isinstance(part, dict) else type(part).__name__,
                }
                for part in content[:16]
            ]
        shape.append(entry)
    return shape


def _tool_output_call_ids(value: Any) -> list[str]:
    found: list[str] = []

    def visit(item: Any) -> None:
        if len(found) >= 8:
            return
        if isinstance(item, dict):
            call_id = item.get("call_id")
            if (
                item.get("type") in ("function_call_output", "custom_tool_call_output")
                and isinstance(call_id, str)
                and call_id
            ):
                found.append(call_id)
            for child in item.values():
                visit(child)
        elif isinstance(item, list):
            for child in item:
                visit(child)

    visit(value)
    return found


def _tool_output_summaries(value: Any) -> list[dict[str, Any]]:
    found: list[dict[str, Any]] = []

    def visit(item: Any) -> None:
        if len(found) >= 8:
            return
        if isinstance(item, dict):
            call_id = item.get("call_id")
            if (
                item.get("type") in ("function_call_output", "custom_tool_call_output")
                and isinstance(call_id, str)
                and call_id
            ):
                summary: dict[str, Any] = {"callId": call_id[:300]}
                output = item.get("output")
                summary["outputType"] = type(output).__name__
                if isinstance(output, str):
                    try:
                        decoded = json.loads(output)
                    except json.JSONDecodeError:
                        pass
                    else:
                        output = decoded
                        summary["decodedType"] = type(output).__name__
                if isinstance(output, list):
                    summary["itemTypes"] = [
                        item.get("type") if isinstance(item, dict) else type(item).__name__
                        for item in output[:16]
                    ]
                extracted = _safe_output_field_types(output)
                if extracted:
                    summary["fields"] = extracted
                found.append(summary)
            for child in item.values():
                visit(child)
        elif isinstance(item, list):
            for child in item:
                visit(child)

    visit(value)
    return found


def _safe_output_field_types(value: Any) -> list[dict[str, str]]:
    fields: list[dict[str, str]] = []

    def append(key: str, item: Any) -> None:
        if len(fields) >= 16:
            return
        fields.append({"key": key, "type": type(item).__name__})

    def visit(item: Any) -> None:
        if len(fields) >= 16:
            return
        if isinstance(item, str):
            try:
                decoded = json.loads(item)
            except json.JSONDecodeError:
                return
            visit(decoded)
        elif isinstance(item, dict):
            for key, child in item.items():
                if key in (
                    "status",
                    "error",
                    "code",
                    "agent_id",
                    "thread_id",
                ):
                    append(key, child)
                visit(child)
        elif isinstance(item, list):
            for child in item:
                visit(child)

    visit(value)
    return fields


def _has_input_text(body: dict[str, Any], text: str) -> bool:
    inputs = body.get("input")
    if not isinstance(inputs, list):
        return False
    for item in inputs:
        if not isinstance(item, dict):
            continue
        content = item.get("content")
        if not isinstance(content, list):
            continue
        for part in content:
            if (
                isinstance(part, dict)
                and part.get("type") in ("input_text", "text")
                and isinstance(part.get("text"), str)
                and text in part["text"]
            ):
                return True
    return False
