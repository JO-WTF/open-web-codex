# Capability catalog rules

The root `AGENTS.md` remains authoritative. These rules apply to code-managed
resources below `capabilities/`.

- Keep publication metadata, reviewed instructions, explicit dependencies and
  Artifact handoff contracts together under one exact Release. Do not put Tool,
  MCP, Skill or Plugin implementations here.
- Drafts use server-managed integer revisions. A published Release is immutable
  and receives its version and hashes from the Capability Compiler; authors do
  not maintain semantic versions, Runtime Role names or content hashes.
- Agent capability requirements must use stable Runtime/Plugin/MCP identifiers.
  Do not infer availability from Provider names, version strings, paths, error
  text or browser state.
- Supervisor definitions reference exact Agent Releases. Runtime Roles, Skill
  roots, MCP inventory and capability gates are derived and validated by the
  Capability Compiler; the browser must not supply them.
- An Agent is composed from exact Skill Releases and declared Tool
  capabilities. It must not inherit a whole hidden capability template or add
  undeclared Runtime capabilities, MCP servers, Tools or capability roots.
  A Supervisor binds immutable Agent Release identities and content hashes.
- Artifact producers and consumers must agree with the referenced Agent input
  and output contracts. Handoffs use durable typed Dataset, Artifact or Domain
  Resource references, not unbounded chat payloads. Chinese `SKILL.md` files
  must name the input owner, permitted Tool capability, expected deliverable,
  user-input condition and failure/unavailable behavior.
- A catalog change is complete only after the catalog tests, Server policy
  tests, Web contract type checking and relevant real Runtime smoke pass or the
  remaining unverified scope is recorded in `docs/capability-baseline.md`.
