---
name: prepare-planning-dataset
description: Prepare a validated city-level warehouse-network planning data set from authorized Workspace sources and hand it to Network Planning after the business mapping is confirmed.
---

# Prepare Planning Dataset

Produce an evidence-bearing data handoff; do not select a warehouse-network solution.
Read [planning-contracts.md](../../references/planning-contracts.md) before publishing.

## Mandatory order

The order below is a gate, not a suggestion. Do not normalize data before the required business definitions and mappings are confirmed.

1. **Get the planning requirements.** Confirm that the Network Planning Agent has published a complete `data_requirement_profile.v1`; publishing the Profile does not require a separate user confirmation. Use the exact `resource_name` and `data_ref` handoff to read it once through the read-only `supply_chain_planner` Resource boundary, then verify its schema and provenance. Do not look for it in `supply_chain_data`, construct a URI, or copy it into another registry. If it is missing or unreadable, stop and ask the Supervisor to complete that step. For a Demo request, this step must happen before Demo generation.
2. **Discover the authorized Workspace.** Call `supply_chain_data.discover_workspace_sources` across the trusted Turn Workspace. An empty result is an explicit data gap. Never substitute packaged examples or historical Profile Resources.
3. **Inspect every source.** Call `supply_chain_data.inspect_workspace_sources` for every opaque source reference. The result contains an exact record count and a head preview. The preview is only for understanding columns and example values: never treat its rows, or the number of preview rows, as the full source. Confirm the requested market, demand unit, service promise, and source scope. Derive the planning period from demand dates, the currency from lane quotes, and the route method from the fields actually present; do not ask the user to confirm those three values. Never ask for organization IDs, Profile IDs, credentials, arbitrary SQL, filesystem paths, or write access.
4. **Describe the source and propose the mapping.** Publish `source_profile.v1`, then pass its returned `data_ref` unchanged as `source_profile_ref` to `publish_mapping_proposal`. The mapping Tool reads the canonical Profile itself; never copy the Profile JSON into the next Tool call or rename/flatten its nested `structure`. The proposal must keep demand aggregated by city and period, coverage as warehouse-to-city relationships, and transport quotes as reusable city-to-city rows. Never expand routes once per demand point. If the Tool returns `mapping_candidates_empty` or another mapping error, stop and report the missing business data; do not retry the same request.
5. **Wait for one whole-revision confirmation.** Stop when any mapping, unit, conversion, relation, or parameter is ambiguous or changes the result. Ask the user to confirm the complete revision in business language. Do not publish a new proposal or retry the same request while waiting.
6. **Normalize the confirmed data.** Call `supply_chain_data.normalize_planning_dataset` once with the published requirement Profile and exact confirmed mapping and parameter snapshots. The Tool rereads the complete authorized source files; do not pass, reconstruct, or calculate from preview rows. If the Tool reports missing, stale, or mismatched mapping or parameter confirmation, stop with the missing business prerequisite; do not retry unchanged input.
7. **Validate the handoff.** Call `supply_chain_data.validate_planning_dataset` on the returned `data_ref`. Do not hand off a dataset with validation errors as decision-ready.
8. **Hand off unchanged.** Return the unchanged `data_ref`, bounded summary, demand distribution, promotion share, observed delivery baseline, missing observations, source range, candidate scope, route completeness, units, and material quality limitations.

## Network Planning handoff

The published Resource contains a typed `network_input` projection with aggregated demand, current facility relationships, existing and reviewed candidate facilities, rates, currency, planning period, and service policy. It also carries the source's exact route provider, method, and complete route rows. The Network Planning Agent must read these facts unchanged.

Do not copy raw order rows into messages, invent missing assignments, silently treat promotion demand as recurring growth, or replace the Resource with a prose summary.

## User-facing communication

Use the user's language and business vocabulary. Progress messages must state the completed business step, its effect on the planning work, and the next action or confirmation needed.

Do not expose Agent names, “capability gap”, registry names, URI values, hashes, resource IDs, tool-call IDs, host paths, “handoff tuple”, runtime versions, or internal Agent-to-Agent routing. Translate an internal technical problem into its business effect. For example, say “规划所需的数据还没有准备齐，下一步是补齐数据定义并继续整理”，not the names of internal registries or publication mechanisms.

## Responsibility boundary

- Own source inspection, aggregation, delivery-baseline calculation, provenance, and data-quality reporting.
- Do not modify enterprise source data.
- Do not repair or rewrite a generated Demo source file, including `planning-parameters.json`. If a generated parameter value disagrees with the confirmed business requirement, report a Demo generator/contract mismatch and stop; the Demo generator owns synthetic source contents.
- Do not geocode unapproved addresses or expose direct personal identifiers.
- Do not choose candidate sites, run facility-location optimization, or recommend the final network.
