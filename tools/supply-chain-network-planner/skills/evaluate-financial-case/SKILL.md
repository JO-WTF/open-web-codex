---
name: evaluate-financial-case
description: Evaluate opening investment, recurring operating savings, NPV, payback, and financial viability for one compatible supply-chain network option using exact planning Resources.
---

# Evaluate Financial Case

Use this method only when investment economics can change the supply-chain decision.

1. Require exact `network_snapshot.v1` and two compatible
   `network_scenario_result.v1` references representing the baseline and candidate.
2. Confirm the horizon, discount rate, and annual growth assumption. Defaults are
   acceptable only for a clearly labeled exploratory estimate.
3. Call `supply_chain_planner.evaluate_financial_case`. Do not recreate its calculations
   in prose.
4. Validate the returned `financial_evaluation.v1` Resource.
5. Return its unchanged `data_ref` and `resource_name`, with currency, period,
   assumptions, exclusions, NPV, payback, and sensitivity limits.

Do not compare scenarios from different snapshots, routes, service policies, currencies,
or periods. A positive modeled NPV does not authorize an investment.
