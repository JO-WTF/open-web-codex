"""Domain-neutral deterministic logic owned by the Tool package."""

from typing import Literal, TypedDict


class AnalysisResult(TypedDict):
    """Stable result returned by ``analyze_record``."""

    status: Literal["ok"]
    record_id: str
    value: int


def analyze_record(record_id: str, value: int) -> AnalysisResult:
    """Validate one record and preserve its exact typed values."""

    normalized_id = record_id.strip()
    if not normalized_id:
        raise ValueError("record_id must not be empty")
    return {"status": "ok", "record_id": normalized_id, "value": value}
