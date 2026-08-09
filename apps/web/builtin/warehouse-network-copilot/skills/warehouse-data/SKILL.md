---
name: warehouse-data
description: Inspect and prepare warehouse-network input data when the user asks to use Excel, CSV, or JSON files in the current Workspace, map fields, normalize records, resolve administrative locations, prepare candidates, or validate geographic data.
---

# Prepare warehouse-network data

- Read only files the user or supervisor identifies by Workspace-relative path. Ask in business language when the intended file or mapping is ambiguous.
- Inspect structure before normalization. Explain missing required data without exposing internal schemas, Runtime IDs, provider storage locations, or server paths.
- Use the Data role's allowed deterministic tools for file inspection, normalization, administrative areas, place resolution, candidate preparation, and geographic validation.
- Keep provider-owned typed intermediate data in the exact MCP Resource returned by the allowed tool. Return that typed reference to the supervisor without rebuilding its URI, adding a content hash, or relabeling it as an Artifact.
- Write a visible ordinary Workspace file when the user requests a saved result or an explicit cross-package handoff. Use create-new semantics and surface name conflicts instead of silently overwriting.
- Do not calculate network strategy, communicate directly with another Task, create platform-owned workflow state, or pass intermediate data through Artifacts.
