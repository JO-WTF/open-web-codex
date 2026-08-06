---
name: create-demo-workspace-data
description: Generate the reviewed synthetic warehouse-network data set in the current authorized Workspace when the user explicitly asks for Demo, sample, example, or synthetic data.
---

# Create Demo Workspace Data

Require an explicit user request for Demo, sample, example, or synthetic data. Use this skill only after that request. Never trigger because a Workspace is empty; a failed discovery or a missing source is not Demo intent.

## Mandatory order

Do not skip a phase, run phases in parallel, or repeat a phase with the same evidence.

1. **Confirm the planning requirements first.** The Network Planning Agent must have prepared a complete `data_requirement_profile.v1`, and the user must have confirmed the whole profile. The Supervisor must pass the exact resource handoff to the Data Agent. Read the Profile once through the read-only `supply_chain_planner` Resource boundary, verify its schema and confirmation state, and use its bounded contents. Do not look for it in `supply_chain_data`, construct a URI, or copy it into another registry. It must state the required city-level demand, warehouse coverage, city-to-city transport quotes, demand dates, units and planning targets. The planning period, currency and route method are derived from the generated sources and are not user confirmation fields. If this prerequisite is missing or unreadable, stop before calling the Demo Tool and tell the user: “我会先确认这次规划需要哪些城市、需求、仓库和运输报价数据，再生成示例数据。”
2. **Generate the reviewed Demo source once.** Call `supply_chain_demo.create_demo_workspace_sources` exactly once, using its current default template and the default seed unless the user explicitly supplies a supported seed. The current reviewed template is city-grain data for 24 Indonesian cities: 24 city-demand rows, 6 warehouses, and 144 reusable city-to-city lanes. It is not a 120,000-demand-point file. Do not select retired `basic` or `large` template identifiers.
3. **Stop on a generation boundary error.** Stop for missing trusted Workspace metadata, a non-empty Workspace, a target conflict, or any Tool failure. Do not retry the same operation, switch templates, or create files another way.
4. **Discover and inspect the result.** After a successful `created` or `reused` result, call `supply_chain_data.discover_workspace_sources`, then inspect every returned source. Inspection includes an exact record count and a head preview. The preview rows are examples only; never treat 20 preview rows, or their count, as the full Demo source. Do not inspect host paths or paste raw rows into messages.
5. **Prepare, then wait for confirmation.** Publish the source profile, then pass its returned `data_ref` unchanged as `source_profile_ref` when publishing the mapping proposal. The mapping Tool reads the canonical source profile; never paste or reconstruct the full Profile JSON, and never continue after `mapping_candidates_empty` or another mapping error. The mapping must preserve city-level demand, warehouse-to-city coverage, and reusable city-to-city quotes without expanding rows by demand point. Stop for whole-revision user confirmation when a mapping, unit, conversion, relation, or planning parameter affects the result.
6. **Normalize only after confirmation.** Call `supply_chain_data.normalize_planning_dataset` once with the exact confirmed profile, mapping, and parameter snapshots. If the Tool says confirmation is missing or stale, report the missing business confirmation and stop; do not retry unchanged input.
7. **Validate and hand off.** Call `supply_chain_data.validate_planning_dataset`. Hand off only a validated `planning-dataset.v2` and retain the `synthetic_demo` classification and template provenance. The Network Planning Agent may analyze only after this handoff.

## User-facing communication

Speak in the user's language and use business terms. Every progress update should answer: what is done, what it means for the planning work, and what is needed next.

Say, for example: “示例数据已生成，覆盖 24 个印尼城市、6 个仓库和 144 条城市间运输线路。下一步需要确认需求与运输数据的对应关系。”

Do not expose Agent names, “capability gap”, registry names, URI values, hashes, resource IDs, tool-call IDs, host paths, “handoff tuple”, runtime versions, or internal Agent-to-Agent routing. Do not describe an internal handoff as a business failure. If an internal transfer is blocked, say: “规划数据还没有准备齐，暂时不能开始分析；我会先补齐数据定义，完成后继续处理。”

Never auto-confirm mappings or business parameters, create a Dataset Release directly, start network analysis, expose host paths, or fall back to packaged examples or historical Profile Resources.
The Data Agent must not rewrite the generated Demo files to repair a parameter mismatch. A mismatch is a generator/contract defect: stop the workflow, report it in business language, and fix the generator before running the Demo flow again.
