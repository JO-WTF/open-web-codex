# ADR 0001: Web Harness And Fact Ownership

## Status

Accepted on 2026-07-12.

## Context

CodexMonitor is currently a React/Tauri application whose frontend directly invokes desktop commands and receives Tauri events. V1 must provide a multi-user browser client while preserving Codex native Thread, Turn, multi-agent, Memory, Skill, Plugin and MCP semantics.

Treating every current desktop state as platform data would create a second Agent Runtime. Letting browsers contact app-server directly would expose credentials, filesystem access, protocol instability and process control without a server authorization boundary.

## Decision

Use the following runtime chain:

```text
Browser
  -> same-origin Web API and authenticated WebSocket
  -> Web Server: identity, RBAC, projects, Tasks, approvals and audit
  -> Codex Host: Profile lifecycle, app-server transport and capability gate
  -> Codex app-server: native Agent Runtime
  -> Runner: Repository, Worktree, container, resources and Artifacts
```

Fact ownership is fixed as follows:

| Fact | Owner | Platform storage allowed |
| --- | --- | --- |
| User, organization, project and resource authorization | PostgreSQL | Full platform record |
| Task, Run, Profile/Thread mapping, approval and audit | PostgreSQL | Full platform record and immutable mapping |
| Thread, Turn, Agent item and multi-agent decisions | Codex Profile/app-server | ID mapping, authorized projection, event cache and audit metadata only |
| Context compaction and Memory | Codex Profile/app-server | Status, operation metadata and authorized export reference only |
| Personal Agent/Skill/Plugin/MCP state | Codex Profile | Version/reference and operation audit only |
| Project instructions and project Skills | Git repository | Index and publication metadata only |
| Worktree, Commit and Push state | Git | Status cache and audit only |

The browser never executes or discovers a local Codex CLI. It does not receive Profile Home paths, raw credentials or arbitrary host filesystem access. The Web API is a product contract and does not mirror raw app-server methods one-for-one.

## Enforcement

- Capability Manifest gates every native module and operation.
- Codex Runtime gaps are assigned to the Codex Rust project.
- Architecture tests reject planner, subagent scheduler, Memory merge, Skill interpreter, Plugin runtime and MCP runtime implementations in the Web project.
- `localStorage` contains only non-authoritative UI preferences and private drafts.
- `scripts/check-tauri-boundary.mjs` prevents new desktop coupling during migration.

## Consequences

- Tauri can be removed after Web parity without removing Codex Runtime behavior.
- A server-side Host and Runner remain mandatory even though the browser is the only user client.
- Platform recovery combines PostgreSQL mappings with native Codex Thread recovery; neither source alone is sufficient.
- Unsupported upstream capabilities remain unavailable rather than receiving a product-incompatible fallback.

## Rejected Alternatives

- Browser directly starts or connects to a user-machine CLI.
- One app-server process shared across all users.
- One new Codex Home for every Run.
- Platform-owned Agent planner, Memory store or Skill/Plugin/MCP runtime.
