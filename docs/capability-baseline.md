# Capability baseline

This document records checked-in, directly observed capability state. Product
requirements belong in `product-design.md`; planned work belongs in
`development-plan.md`.

## Snapshot

Captured on 2026-07-12 from commit `e0bb3ab351a9a5d4a8bc798760360f0eaab9c7a7`:

| Component | State |
| --- | --- |
| Codex subtree | synchronized through `openai/codex` `9e552e9d15ba52bed7077d5357f3e18e330f8f38` |
| Pending official commits | 0 according to `scripts/codex-upstream-status.sh` |
| Custom commits above recorded subtree base | 0; product changes are ordinary monorepo commits after the subtree merge |
| Web platform | Axum/PostgreSQL prototype plus the earlier loopback Web MVP |

## Reproduced evidence

- `scripts/codex-upstream-status.sh` reports `Status: synchronized`.
- The locally built `codex app-server` completes `initialize` and returns
  `capabilityManifest`, `codexHome`, `platformFamily`, `platformOs` and
  `userAgent`.
- The observed manifest contains 17 declarations. The Web manifest tests and
  initial contract checks pass.
- Codex generates JSON Schema and TypeScript definitions containing the
  Capability Manifest types.
- The platform source contains PostgreSQL migrations and API handlers for
  bootstrap/session, organization membership, project, Task, Run and persisted
  Run events.

The successful checks prove only the surfaces named above. They do not prove
multi-user isolation, durable Profile recovery, Runner isolation or production
security.

## Runtime capability assessment

| Capability | Manifest/source state | Validation status |
| --- | --- | --- |
| Protocol Schema | available | generated JSON/TypeScript artifacts exist |
| Capability negotiation | available, provisional | `initialize` emits schema version, build identity, protocol range, status, limits and reasons; declarations are still assembled in code rather than derived from the full method registry |
| Thread lifecycle | declared supported | initialize smoke passed; multi-cwd and restart recovery smoke still required |
| Turn lifecycle | declared supported | real lifecycle smoke beyond initialize still required |
| Approval lifecycle | declared supported | runtime methods exist; platform durable request/decision bridge is not implemented |
| Profile multi-workspace | declared supported | manifest limits are present; ownership and concurrency behavior remain unverified |
| Memory lifecycle | declared unsupported | Codex contains compaction/memory surfaces, but the Web-safe status/export/reset bridge is absent |
| Native Agent CRUD | declared unsupported | no stable Web-safe CRUD/validation contract |
| Multi-agent trajectory | declared experimental | Codex parent/child and collab events exist; fixture and real trajectory smoke are pending |
| Skills | degraded/unsupported by operation | listing is declared; safe write, validation and isolated testing are not enabled |
| Plugins | declared unsupported | do not enable Studio lifecycle or permissions UI |
| MCP | config degraded; OAuth/elicitation unsupported | status listing is declared; Web-safe CRUD, reload and lifecycle validation are pending |
| Tools discovery | declared unsupported | do not expose a platform fallback catalog |
| Provider/model management | declared supported by the checked-in Runtime | `modelProvider/list`, controlled Profile config writes, provider-scoped refresh, model selection and context-window persistence are wired; real credential smoke remains required for release promotion |

## Web platform assessment

| Surface | Current state | Production gap |
| --- | --- | --- |
| Independent server | Axum server and Cargo workspace build structure exist | deployment/config hardening and end-to-end startup gate remain |
| Persistence | PostgreSQL migrations cover users/sessions, organizations/memberships, projects, tasks, runs and run events | Profiles, Workspaces, approvals, leases, audit, artifacts, jobs and complete constraints are missing |
| Authentication | bootstrap, password session creation and auth extractor exist | HttpOnly-only session flow, CSRF, logout/revocation, rate limiting and complete tests are missing |
| Authorization | membership checks exist on part of the organization surface | centralized resource/action RBAC and cross-user denial matrix are missing |
| Codex bridge | Fake/Real adapter and event fan-out exist | current Real adapter proxies legacy daemon RPC/SSE; raw `/api/rpc`, permissive CORS and query-token SSE are prototype-only and violate the production boundary |
| Task/Run | CRUD/start/cancel/message and lifecycle projection exist | worktree provisioning, Profile Host, idempotent scheduler, approvals, recovery and monotonic replay contract are incomplete |
| Browser | loopback MVP can connect workspace, start Thread and send text | it still targets the local preview Gateway and accepts server paths; it is not the authenticated multi-user product UI |

## Immediate capability gates

1. Derive Manifest method sets and experimental state from generated Codex
   protocol/build facts; add Provider/model capability IDs.
2. Generate a digest-addressed contract bundle and separate it from Web product
   feature policy.
3. Run real Thread restart/multi-cwd, Provider, approval, multi-agent and MCP
   smoke suites before promoting declarations to product support.
4. Replace the legacy raw RPC/SSE browser bridge with authenticated platform
   DTOs, durable event cursors and server-side ownership checks.
