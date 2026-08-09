---
name: warehouse-network
description: Define warehouse-network data requirements and perform deterministic network analysis when the user asks about routes, cost, coverage, service levels, warehouse changes, p-median placement, constrained optimization, maps, or reports using typed MCP Resources or ordinary Workspace files.
---

# Analyze warehouse networks

- State required business inputs before analysis and ask the supervisor for missing or ambiguous information.
- Read user files and explicit cross-package handoffs from user-approved Workspace-relative paths. Read provider-owned intermediate data only from an exact typed MCP Resource reference returned by an allowed tool or supplied explicitly by the supervisor; never construct or guess a URI.
- Treat the Network role's deterministic tools as composable capabilities, not a fixed workflow. Call only the capabilities required by the user's decision; `plan_route_matrix` is optional when its count and cost preview are not needed, and maps or reports are created only when requested.
- Require explicit business choices at the Tool boundary: route method and any haversine detour/speed parameters; cost warehouse scope, currency/rules when quotes are incomplete; baseline objective, coverage mode and service targets; scenario objective and targets; and p-median open count, complete fixed/optional existing-warehouse policy, service targets, time limit and any coverage constraints. Do not supply an Indonesia-specific or hidden default.
- Obtain explicit user permission before paid navigation calls or allowing an existing warehouse to close. Navigation provider output crosses into this package only as a validated Workspace-relative JSON file; never substitute a map-provider Resource URI.
- Reuse every exact route or cost pair that the deterministic tool can verify from its real inputs, calculate only missing pairs, and report reused versus computed counts. Do not reject partial reuse because an unrelated row or whole dataset changed.
- Compare an actual baseline only when current assignments exist; otherwise name the result `optimized_existing_footprint`. A close-only scenario does not require p-median or candidate optimization, and a data-only request does not trigger Network, map, or report tools.
- Create final map and report JSON with the exact normalized, baseline, facility-location and comparison Resource references plus an explicit create-new Workspace-relative output path. These final files may be registered by the Platform as Artifacts; intermediate Resources and navigation handoff files may not.
- Save a visible ordinary Workspace intermediate file when the user requests it or an explicit cross-package handoff needs it. Use create-new semantics. Keep provider-owned typed intermediate data in MCP Resources. Artifacts are explicit final user deliverables, not Agent inputs or data exchange.
- Do not invent missing costs, routes, constraints, Runtime readiness, or completed results.
