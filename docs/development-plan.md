# Development plan

## Planning model

This plan uses five product milestones and four parallel workstreams. It replaces
the previous 550-item sequential checklist. Detailed issues may expand a
milestone, but an issue is complete only with code, tests and observable evidence.

Workstreams:

- **Runtime:** `codex/`, app-server, generated contracts and upstream sync.
- **Platform:** Web API, PostgreSQL, Profile Host, Runner, Git and security.
- **Experience:** browser application, task workbench and Codex Studio.
- **Contract/QA:** fixtures, smoke tests, compatibility, recovery and release
  evidence.

## M0 — Monorepo and contract reset

Status: in progress.

Deliverables:

- [x] Import CodexMonitor and customized Codex into one repository.
- [x] Preserve the custom Codex history required for upstream three-way merges.
- [x] Add official upstream status and guarded sync tooling.
- [x] Replace outdated product, capability and plan baselines.
- [ ] Reconcile main-only changes in both source forks and decide whether each is
  superseded or reapplied.
- [ ] Sync `open-codex` with a reviewed official Codex checkpoint.
- [ ] Generate Capability Manifest, schema bundle and build identity in Codex.
- [ ] Consume the generated bundle in Web contract tests.
- [ ] Pass real initialize, Thread, Provider/model and approval smoke tests.

Gate M0: a pinned Codex binary and digest produces contracts accepted by Web CI;
the source-level baseline and generated manifest agree.

## M1 — Single-user Alpha vertical slice

Runtime and Platform proceed in parallel after the Manifest shape is stable.

Platform:

- PostgreSQL migrations for Project, Task, Run, Profile, Workspace, Approval,
  event cursor and audit records.
- Server session authentication suitable for a single administrator deployment.
- Profile Home creation, single-process lock, app-server start/stop/restart and
  handshake.
- Repository mirror, isolated worktree, Run lease and cleanup worker.
- Internal Codex adapter with typed requests, server-request correlation,
  bounded queues and reconnect projection.

Experience:

- Project import and Task creation.
- Profile and Provider/model selection.
- Task workbench with activity, plan, command output and diff.
- Approval, structured input, cancel, resume and explicit commit.
- Loading, empty, reconnect, blocked and terminal states.

Gate M1: one browser can complete the Alpha workflow, then recover the same Task
after browser refresh and Profile Host restart.

## M2 — Multi-user Beta and isolation

- Invitation-only accounts, memberships, RBAC and session revocation.
- One persistent Profile per member with isolated secrets and `CODEX_HOME`.
- Durable event sequence, WebSocket resume cursor and idempotent commands.
- Control lease, approval authorization, concurrent-decision protection and
  complete audit records.
- Rootless execution boundary, egress policy, quotas and Runner recovery.
- Push with protected-branch checks and no force-push path.
- Provider management plus MCP inventory/OAuth/elicitation Web flows.

Gate M2: two users run concurrent Tasks with fault injection and no cross-user
data, event, credential or filesystem access.

## M3 — Capability-gated Codex Studio

Each module ships independently in this order:

1. Profiles and Provider/model management.
2. MCP servers, tools, OAuth and elicitation.
3. Plugin discovery and lifecycle supported by the pinned build.
4. Memory health, restart continuity, reset and export.
5. Native Agent management and multi-agent trajectory.
6. Personal/project Skill validation, test, publish and rollback.

Every module requires a supported or explicitly accepted experimental Manifest
entry, offline fixture coverage, a real smoke test and an unavailable state.

Gate M3: no Studio write action bypasses capability, authorization, path or
audit checks.

## M4 — Production hardening and Web-only GA

- Backup/restore for PostgreSQL, Profile Homes and repository metadata.
- Codex upgrade canary, compatibility check and rollback without Profile rewrite.
- Capacity, long-running process, disk pressure and queue recovery tests.
- Security review for session, CSRF, WebSocket, SSRF, path traversal, secret
  leakage, sandbox escape and cross-project identifiers.
- Browser/accessibility regression matrix and operational runbooks.
- One stable Beta release cycle using Web as the primary client.
- Remove Tauri, desktop release assets and preview shared-token/SSE transport.

Gate M4: all product acceptance criteria pass on a clean Linux deployment and no
production flow requires a desktop client.

## Critical path

```text
Generated runtime contract
  -> Profile Host
  -> isolated Runner/worktree
  -> single-user Task loop
  -> durable multi-user control
  -> capability-gated Studio
  -> hardening and Tauri removal
```

Agent/Skill/Plugin completeness does not block M1. Missing Studio capabilities
remain disabled while the core Task loop advances.

## Definition of done

- Success, denial, timeout, retry and recovery paths are covered.
- Contracts, migrations, audit fields and operational documentation are updated.
- Logs and errors contain no secret, Prompt, memory body or unbounded code body.
- Component checks and the narrowest relevant integration tests pass.
- Cross-project changes include a real app-server smoke result.
- A milestone checkbox is not marked complete from code inspection alone when a
  runtime or recovery claim requires execution evidence.
