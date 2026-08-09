"""Deterministic financial and risk evidence for supply-chain decisions."""

from __future__ import annotations

import hashlib
import json
from decimal import Decimal

from .mcp_contracts import ResourceRef
from .models import (
    FinancialEvaluation,
    NetworkScenarioResult,
    NetworkSnapshot,
    RiskItem,
    RiskRegister,
)


def _content_identity(prefix: str, payload: object) -> str:
    encoded = json.dumps(
        payload,
        ensure_ascii=True,
        sort_keys=True,
        separators=(",", ":"),
        default=str,
    ).encode("utf-8")
    return f"{prefix}_{hashlib.sha256(encoded).hexdigest()[:24]}"


def evaluate_financial_case(
    snapshot: NetworkSnapshot,
    baseline: NetworkScenarioResult,
    candidate: NetworkScenarioResult,
    *,
    snapshot_ref: ResourceRef,
    baseline_result_ref: ResourceRef,
    candidate_result_ref: ResourceRef,
    horizon_years: int,
    discount_rate: float,
    annual_growth_rate: float,
) -> FinancialEvaluation:
    if not 1 <= horizon_years <= 20:
        raise ValueError("horizon_years must be between 1 and 20")
    if not 0 <= discount_rate <= 1:
        raise ValueError("discount_rate must be between 0 and 1")
    if not -0.5 <= annual_growth_rate <= 1:
        raise ValueError("annual_growth_rate must be between -0.5 and 1")
    for result in (baseline, candidate):
        if (
            result.snapshot_id != snapshot.snapshot_id
            or result.service_policy_id != snapshot.service_policy.policy_id
        ):
            raise ValueError("financial inputs must use the supplied snapshot and policy")
    if baseline.route_matrix_id != candidate.route_matrix_id:
        raise ValueError("financial inputs must use the same route matrix")

    baseline_ids = set(baseline.active_facility_ids)
    candidate_ids = set(candidate.active_facility_ids)
    added_ids = candidate_ids - baseline_ids
    facilities = {item.facility_id: item for item in snapshot.facilities}
    if any(facility_id not in facilities for facility_id in added_ids):
        raise ValueError("candidate result references an unknown facility")
    opening_investment = sum(
        (facilities[facility_id].opening_cost for facility_id in added_ids),
        Decimal("0"),
    )
    annual_savings = baseline.metrics.total_cost - candidate.metrics.total_cost
    discount = Decimal(str(discount_rate))
    growth = Decimal(str(annual_growth_rate))
    npv = -opening_investment
    cumulative = Decimal("0")
    payback_years: float | None = None
    for year in range(1, horizon_years + 1):
        year_savings = annual_savings * ((Decimal("1") + growth) ** (year - 1))
        npv += year_savings / ((Decimal("1") + discount) ** year)
        if payback_years is None and year_savings > 0:
            previous = cumulative
            cumulative += year_savings
            if cumulative >= opening_investment:
                fraction = (
                    (opening_investment - previous) / year_savings
                    if opening_investment > previous
                    else Decimal("0")
                )
                payback_years = float(Decimal(year - 1) + fraction)

    identity_payload = {
        "snapshot_id": snapshot.snapshot_id,
        "baseline_result_id": baseline.result_id,
        "candidate_result_id": candidate.result_id,
        "horizon_years": horizon_years,
        "discount_rate": discount_rate,
        "annual_growth_rate": annual_growth_rate,
    }
    return FinancialEvaluation(
        evaluation_id=_content_identity("finance", identity_payload),
        snapshot_id=snapshot.snapshot_id,
        baseline_result_id=baseline.result_id,
        candidate_result_id=candidate.result_id,
        currency=snapshot.currency,
        planning_period=snapshot.planning_period,
        horizon_years=horizon_years,
        discount_rate=discount_rate,
        annual_growth_rate=annual_growth_rate,
        opening_investment=opening_investment,
        annual_operating_savings=annual_savings,
        net_present_value=npv,
        payback_years=payback_years,
        financially_viable=npv > 0
        and candidate.metrics.coverage_ratio > baseline.metrics.coverage_ratio,
        input_refs=[snapshot_ref, baseline_result_ref, candidate_result_ref],
        assumptions=[
            "Scenario total-cost differences recur once per planning period.",
            "Demand growth changes annual operating savings at the supplied rate.",
            "Opening cost is paid at the start of the first period.",
            "Taxes, financing structure, residual value, and construction delay are excluded.",
        ],
    )


def build_risk_register(decision_scope: str, risks: list[RiskItem]) -> RiskRegister:
    risk_ids = [risk.risk_id for risk in risks]
    if len(risk_ids) != len(set(risk_ids)):
        raise ValueError("risk register contains duplicate risk_id values")
    payload = {
        "decision_scope": decision_scope,
        "risks": [risk.model_dump(mode="json") for risk in risks],
    }
    return RiskRegister(
        register_id=_content_identity("risks", payload),
        decision_scope=decision_scope,
        risks=risks,
        unresolved_risk_count=sum(risk.likelihood * risk.impact >= 12 for risk in risks),
    )
