"""Canonical bounded JSON codec shared by provider-owned Resource stores."""

from __future__ import annotations

import json
from collections.abc import Mapping
from typing import Any

from pydantic import BaseModel

from .errors import ProviderContractError

MAX_RESOURCE_BYTES = 32 * 1024 * 1024


def canonical_json_bytes(
    value: BaseModel | Mapping[str, Any],
    *,
    max_bytes: int = MAX_RESOURCE_BYTES,
) -> bytes:
    if isinstance(value, BaseModel):
        payload: Any = value.model_dump(mode="json", by_alias=True)
    elif isinstance(value, Mapping):
        payload = dict(value)
    else:
        raise ProviderContractError("resource_payload_invalid")
    if not isinstance(max_bytes, int) or isinstance(max_bytes, bool) or max_bytes <= 0:
        raise ProviderContractError("resource_bound_invalid")
    try:
        encoded = json.dumps(
            payload,
            ensure_ascii=False,
            sort_keys=True,
            separators=(",", ":"),
            allow_nan=False,
        ).encode("utf-8")
    except (TypeError, ValueError, UnicodeError) as error:
        raise ProviderContractError("resource_payload_invalid") from error
    if len(encoded) > max_bytes:
        raise ProviderContractError("resource_payload_too_large")
    return encoded


def decode_json_object(content: bytes, *, max_bytes: int = MAX_RESOURCE_BYTES) -> dict[str, Any]:
    if not isinstance(content, bytes):
        raise ProviderContractError("resource_payload_invalid")
    if not isinstance(max_bytes, int) or isinstance(max_bytes, bool) or max_bytes <= 0:
        raise ProviderContractError("resource_bound_invalid")
    if len(content) > max_bytes:
        raise ProviderContractError("resource_payload_too_large")
    try:
        payload = json.loads(content.decode("utf-8"))
    except (UnicodeError, json.JSONDecodeError) as error:
        raise ProviderContractError("resource_payload_invalid") from error
    if not isinstance(payload, dict):
        raise ProviderContractError("resource_payload_invalid")
    return payload


def require_payload_schema(payload: Mapping[str, Any], expected_schema: str) -> None:
    camel = payload.get("schemaVersion")
    snake = payload.get("schema_version")
    if camel is not None and snake is not None and camel != snake:
        raise ProviderContractError("resource_payload_schema_ambiguous")
    if (camel if camel is not None else snake) != expected_schema:
        raise ProviderContractError("resource_payload_schema_mismatch")
