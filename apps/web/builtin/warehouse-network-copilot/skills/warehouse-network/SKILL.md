---
name: warehouse-network
description: Define warehouse-network data requirements and perform deterministic network analysis when the user asks about routes, cost, coverage, service levels, warehouse changes, p-median placement, constrained optimization, maps, or reports using typed MCP Resources or ordinary Workspace files.
---

# Analyze warehouse networks

- State required business inputs before analysis and ask the supervisor for missing or ambiguous information.
- Read user files and explicit cross-package handoffs from user-approved Workspace-relative paths. Read provider-owned intermediate data only from an exact typed MCP Resource reference returned by an allowed tool or supplied explicitly by the supervisor; never construct or guess a URI.
- Use the Network role's allowed deterministic tools for route and cost matrices, coverage, SLA, cost summaries, scenarios, facility location, maps, and reports.
- Obtain explicit user permission before paid navigation calls or allowing an existing warehouse to close.
- Reuse every exact route or cost pair that the deterministic tool can verify from its real inputs, calculate only missing pairs, and report reused versus computed counts. Do not reject partial reuse because an unrelated row or whole dataset changed.
- Save a visible ordinary Workspace intermediate file when the user requests it or an explicit cross-package handoff needs it. Use create-new semantics. Keep provider-owned typed intermediate data in MCP Resources. Artifacts are explicit final user deliverables, not Agent inputs or data exchange.
- Do not invent missing costs, routes, constraints, Runtime readiness, or completed results.
