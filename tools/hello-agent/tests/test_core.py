from __future__ import annotations

import pytest

from hello_agent.core import Greeting, build_greeting, evaluate_greeting


def test_writer_normalizes_name_and_builds_canonical_message() -> None:
    greeting = build_greeting(" 小林 ")

    assert greeting == Greeting(name="小林", message="你好，小林！")


@pytest.mark.parametrize("name", ["", "   ", "x" * 41])
def test_writer_rejects_invalid_names(name: str) -> None:
    with pytest.raises(ValueError):
        build_greeting(name)


def test_reviewer_approves_writer_result() -> None:
    greeting = build_greeting("小林")

    review = evaluate_greeting(greeting)

    assert review.approved is True
    assert review.reasons == []
    assert review.greeting is greeting


def test_reviewer_rejects_mismatched_message_without_rewriting_it() -> None:
    greeting = Greeting(name="小林", message="你好，小周！")

    review = evaluate_greeting(greeting)

    assert review.approved is False
    assert review.greeting == greeting
    assert review.reasons == ["message does not match the expected greeting"]


def test_reviewer_reports_all_relevant_problems() -> None:
    greeting = Greeting(name=" 小林 ", message="x" * 81)

    review = evaluate_greeting(greeting)

    assert review.approved is False
    assert review.reasons == [
        "name must not contain leading or trailing whitespace",
        "message must contain at most 80 characters",
        "message does not match the expected greeting",
    ]
