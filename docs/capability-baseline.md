# Capability baseline

This document records checked-in, directly observed capability state. Product
requirements belong in `product-design.md`; planned work belongs in
`development-plan.md`.

## Snapshot

Observed on 2026-07-21 from source commit
`e8db56a4c7735dff09c8e18ac4a439508830b588`:

| Component | State |
| --- | --- |
| Codex subtree | integrated through `openai/codex` `1bbdb32789e1f79932df44941236ea3658f6e965` |
| Observed official main | `0b175e6439a8608ba7726ee153fd8590619e8f34`; 206 commits await integration |
| Local Codex seams vs official main | 63 local-only and 56 diverged paths; 1,049 upstream-only paths await the guarded sync |
| Local customization footprint vs integrated base | 119 paths: 67 production source, 18 focused tests, 31 generated artifacts and 3 docs/config paths |
| Web platform | Axum/PostgreSQL prototype plus the earlier loopback Web MVP |

## Reproduced evidence

- `scripts/codex-upstream-status.sh` reports `Status: update available`; the
  customization status script reports 1,168 tree differences split into 1,049
  upstream-only, 63 local-only and 56 diverged paths.
- A three-way merge preview reports five textual conflict paths. One is a
  generated TypeScript file; the source conflicts are concentrated in Chat API,
  Provider tool-policy tests and TUI Provider integration.
- The locally built `codex app-server` completes `initialize` and returns
  `capabilityManifest`, `codexHome`, `platformFamily`, `platformOs` and
  `userAgent`.
- The observed manifest contains 18 declarations, including
  `models.providers`.
- The last complete validation against integrated upstream `1bbdb32789e1` was
  recorded at commit `26329a12767414ec0a1b6d0f0c9c7c5a65147529`: `just fmt`,
  app-server Schema generation, app-server protocol, Chat transport,
  Provider/model-manager, app-server model-list, TUI scoped tests, the Web
  contract check and the real `--require-manifest` app-server smoke passed.
- No validation claim is made yet for the pending replay onto `0b175e6439a8`.
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
| Capability negotiation | available, provisional | `initialize` emits schema version, build identity, protocol range, status, limits and reasons; method registries validate Manifest wire-name refs, experimental consistency, and product attribution policy; capability declarations remain hand-assembled Alpha subset rather than full generated policy |
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
| Structured reply cards / map cards | declared unsupported | no generated card contract, card Artifact store, renderer gate or real app-server smoke exists |
| Provider/model management | declared supported by the checked-in Runtime | `models.providers`, `modelProvider/list`, controlled Profile config writes, provider-scoped refresh, model selection and context-window persistence are wired and covered by scoped Runtime/TUI tests; real credential-isolation smoke remains required for release promotion |

## Web platform assessment

| Surface | Current state | Production gap |
| --- | --- | --- |
| Independent server | Axum server and Cargo workspace build structure exist | deployment/config hardening and end-to-end startup gate remain |
| Persistence | PostgreSQL migrations cover users/sessions, organizations/memberships, projects, tasks, runs and versioned run-event projections with monotonic replay sequence | Profiles, Workspaces, approvals, leases, audit, artifacts, jobs, retention and complete constraints are missing |
| Authentication | bootstrap, password session creation and auth extractor exist | HttpOnly-only session flow, CSRF, logout/revocation, rate limiting and complete tests are missing |
| Authorization | membership checks exist on part of the organization surface | centralized resource/action RBAC and cross-user denial matrix are missing |
| Codex bridge | Fake/Real adapter and event fan-out exist; transitional `profile-host` provisions `CODEX_HOME` before Tauri app-server spawn | current Real adapter proxies legacy daemon RPC/SSE; a persistent native Profile Host, raw-RPC removal and production authorization boundary remain incomplete |
| Task/Run | CRUD/start/cancel/message, safe Item/Delta projection, monotonic cursor replay and Thread-history reconciliation exist | worktree provisioning, Profile Host, idempotent scheduler, approvals, authenticated subscriptions and restart E2E remain incomplete |
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
