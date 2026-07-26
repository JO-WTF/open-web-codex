"""Shared greeting contract and deterministic business rules."""

from __future__ import annotations

from pydantic import BaseModel, ConfigDict, Field

MAX_NAME_CHARACTERS = 40
MAX_MESSAGE_CHARACTERS = 80


class Greeting(BaseModel):
    """The complete handoff from the Writer to the Reviewer."""

    model_config = ConfigDict(extra="forbid", frozen=True)

    name: str = Field(description="The person being greeted")
    message: str = Field(description="The exact greeting produced for that person")


class GreetingReview(BaseModel):
    """The Reviewer's decision and explicit reasons."""

    model_config = ConfigDict(extra="forbid", frozen=True)

    approved: bool
    greeting: Greeting
    reasons: list[str]


def normalize_name(name: str) -> str:
    """Normalize and bound one user-provided name."""

    normalized = name.strip()
    if not normalized:
        raise ValueError("name must not be empty")
    if len(normalized) > MAX_NAME_CHARACTERS:
        raise ValueError(
            f"name must contain at most {MAX_NAME_CHARACTERS} characters"
        )
    return normalized


def build_greeting(name: str) -> Greeting:
    """Build the one canonical greeting for a valid name."""

    normalized = normalize_name(name)
    return Greeting(name=normalized, message=f"你好，{normalized}！")


def evaluate_greeting(greeting: Greeting) -> GreetingReview:
    """Review a greeting without modifying the submitted handoff."""

    reasons: list[str] = []
    expected: Greeting | None = None

    try:
        normalized = normalize_name(greeting.name)
        expected = build_greeting(normalized)
        if greeting.name != normalized:
            reasons.append("name must not contain leading or trailing whitespace")
    except ValueError as error:
        reasons.append(str(error))

    if len(greeting.message) > MAX_MESSAGE_CHARACTERS:
        reasons.append(
            f"message must contain at most {MAX_MESSAGE_CHARACTERS} characters"
        )
    if expected is not None and greeting.message != expected.message:
        reasons.append("message does not match the expected greeting")

    return GreetingReview(
        approved=not reasons,
        greeting=greeting,
        reasons=reasons,
    )
