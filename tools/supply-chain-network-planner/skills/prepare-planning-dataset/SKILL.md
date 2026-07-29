---
name: prepare-planning-dataset
description: Act as the supply-chain Data Agent to inspect an authorized read-only source, aggregate demand and delivery performance, preserve reviewed candidate and route facts, detect promotion and data-quality effects, and publish planning-dataset.v2 for downstream network planning.
---

# Prepare Planning Dataset

Produce an evidence-bearing data handoff; do not select a warehouse-network solution.
Read [planning-contracts.md](../../references/planning-contracts.md) before publishing.

## Workflow

1. If the assignment does not supply an exact authorized source ID, call
   `supply_chain_data.list_planning_sources` and match typed market, period, currency,
   service-policy, and region metadata. Stop on ambiguity.
2. Confirm the requested market, planning period, demand unit, service promise, and
   source scope. Never ask for organization IDs, Profile IDs, credentials, arbitrary
   SQL, filesystem paths, or write access.
3. Call `supply_chain_data.inspect_planning_source`. Review source timestamp, date range,
   row count, demand units, node count, existing and candidate facility counts, route
   count, blocking errors, and warnings.
4. If the source is appropriate, call `supply_chain_data.build_planning_dataset`. This
   reads the bound source and writes only an immutable Profile-scoped MCP Resource; it
   never modifies source data.
5. Call `supply_chain_data.validate_planning_dataset` on the returned `data_ref`.
   Do not hand off a dataset with validation errors as decision-ready.
6. Return the unchanged `data_ref`, bounded summary, demand distribution, promotion
   share, observed delivery baseline, missing observations, source range, candidate
   scope, route completeness, units, and material quality limitations.

## Network Planning handoff

The published Resource contains a typed `network_input` projection with aggregated
demand, current facility relationships, existing and reviewed candidate facilities,
rates, currency, planning period, and service policy. It also carries the source's exact
route provider, method, and complete route rows. The Network Planning Agent must read
this Resource and use those facts unchanged.

Do not copy raw order rows into messages, invent missing assignments, silently treat
promotion demand as recurring growth, or replace the Resource with a prose summary.

## Responsibility boundary

- Own source inspection, aggregation, delivery-baseline calculation, provenance, and
  data-quality reporting.
- Do not modify enterprise source data.
- Do not geocode unapproved addresses or expose direct personal identifiers.
- Do not choose candidate sites, run facility-location optimization, or recommend the
  final network.
