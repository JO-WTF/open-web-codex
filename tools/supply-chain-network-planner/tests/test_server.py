from __future__ import annotations

import pytest

from supply_chain_planner.server import MAX_PROFILE_GOAL_CHARS, _normalize_profile_goal


def test_profile_goal_has_a_bounded_business_summary_limit() -> None:
    assert len(_normalize_profile_goal("x" * MAX_PROFILE_GOAL_CHARS)) == MAX_PROFILE_GOAL_CHARS
    assert len(_normalize_profile_goal("x" * 5368)) == 5368

    with pytest.raises(ValueError, match="1-8000"):
        _normalize_profile_goal("x" * (MAX_PROFILE_GOAL_CHARS + 1))


def test_profile_goal_rejects_blank_business_summary() -> None:
    with pytest.raises(ValueError, match="1-8000"):
        _normalize_profile_goal("  \n  ")
