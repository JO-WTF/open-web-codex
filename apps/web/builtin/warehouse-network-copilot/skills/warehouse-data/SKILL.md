---
name: warehouse-data
description: Inspect and prepare warehouse-network input data when the user asks to use Excel, CSV, or JSON files in the current Workspace, map fields, normalize records, resolve administrative locations, or validate geographic data.
---

# Prepare warehouse-network data

- Read only files the user or supervisor identifies by Workspace-relative path. Ask in business language when the intended file or mapping is ambiguous.
- Inspect structure before normalization. Explain missing required data without exposing internal schemas, Runtime IDs, provider storage locations, or server paths.
- Use the Data role's allowed deterministic tools for file discovery, inspection, normalization, and geographic preparation. Candidate warehouses must come from a candidate source that the user confirms; never generate planning candidates from an administrative catalog. If a later analysis needs candidates and no confirmed candidate source is present, return `needs_input` and ask for that source.
- Keep provider-owned typed intermediate data in the exact MCP Resource returned by the allowed tool. Return that typed reference to the supervisor without rebuilding its URI, adding a content hash, or relabeling it as an Artifact.
- Write a visible ordinary Workspace file when the user requests a saved result or an explicit cross-package handoff. Use create-new semantics and surface name conflicts instead of silently overwriting.
- Do not calculate network strategy, communicate directly with another Task, create platform-owned workflow state, or pass intermediate data through Artifacts.
