---
name: prepare-network-baseline
description: Prepare an immutable, validated supply-chain network snapshot and route matrix from a planning-dataset.v2 Resource. Use before calculating current coverage, comparing a network option, or optimizing reviewed candidate locations.
---

# Prepare Network Baseline

Create one auditable input state before any coverage, cost, or location calculation.
Read [planning-contracts.md](../../references/planning-contracts.md) before using the
planning tools.

## Workflow

1. Identify the planning period, currency, demand unit, service promise, and the exact
   end-to-end SLA formula. Do not infer a 1-day threshold from route duration alone.
2. Require a validated `planning-dataset.v2` produced by
   `$prepare-planning-dataset`. Read its Resource and use the exact `network_input`,
   `route_provider`, `route_method`, and `route_entries` fields. Preserve stable
   business identifiers; do not add candidate or route facts from local files.
3. Confirm that every demand point and facility has coordinates and that the dataset
   contains every required facility-demand route pair.
4. Call `supply_chain_planner.prepare_network_snapshot`. Carry its `data_ref` unchanged.
   Never mutate a published snapshot; prepare a new one when source facts or policy
   assumptions change.
5. Register the dataset's exact route rows with its declared provider and method. Do
   not regenerate reviewed routes unless the assignment explicitly requires refreshed
   route evidence from an authorized routing capability.
6. Require a complete matrix for decision work. Use an incomplete matrix only for
   explicit diagnostics, keeping missing and unreachable pairs visible.
7. Call `supply_chain_planner.validate_network_resource` for both snapshot and route
   matrix. Stop on validation errors. Report missing or unreachable route pairs rather
   than silently substituting straight-line distance.

## Input rules

- `demand_units` and `capacity_units` are nonnegative integer planning units.
- `currency` is one ISO-style three-letter currency and all costs use it.
- Every facility-demand pair must resolve to exactly one transport rate, using precedence:
  demand-specific, then region-specific, then facility default.
- Existing demand relationships use `current_facility_id`; missing relationships remain
  missing and are not imputed.
- Include candidate facilities and their routes in the same snapshot when the next task
  is location optimization. Otherwise the solver cannot evaluate them.

## Handoff

Return the snapshot and route-matrix `data_ref` objects, source counts, planning period,
currency, service-policy formula, route completeness, geocoding exceptions, and material
data-quality caveats. Do not paste Resource JSON into the answer.
