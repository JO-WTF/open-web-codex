# Capability catalog rules

The root `AGENTS.md` remains authoritative. These rules apply to code-managed
resources below `capabilities/`.

- Keep publication metadata, reviewed instructions and Artifact handoff
  contracts together under one exact ID/version. Do not put Tool, MCP, Skill or
  Plugin implementations here.
- Treat a published version as immutable. A contract change creates a new
  version and updates all current callers, tests, fixtures and documentation in
  the same change. Do not add old-version fallback resolution.
- Agent capability requirements must use stable Runtime/Plugin/MCP identifiers.
  Do not infer availability from Provider names, version strings, paths, error
  text or browser state.
- Supervisor manifests reference exact Agent Definition versions. Runtime Role
  names, MCP inventory and capability gates are derived and validated by
  `open-web-codex-supervisor-catalog`; the browser must not supply them.
- Artifact producers and consumers must agree with the referenced Agent input
  and output contracts. Handoffs use durable typed Artifact references, not
  unbounded chat payloads.
- A catalog change is complete only after the catalog tests, Server policy
  tests, Web contract type checking and relevant real Runtime smoke pass or the
  remaining unverified scope is recorded in `docs/capability-baseline.md`.
