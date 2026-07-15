# Capability baseline

This document records checked-in, directly observed capability state. Product
requirements belong in `product-design.md`; planned work belongs in
`development-plan.md`.

## Snapshot

Captured on 2026-07-15 from commit `26329a12767414ec0a1b6d0f0c9c7c5a65147529`:

| Component | State |
| --- | --- |
| Codex subtree | synchronized through `openai/codex` `1bbdb32789e1f79932df44941236ea3658f6e965` |
| Pending official commits | 0 according to `scripts/codex-upstream-status.sh` |
| Local Codex seams vs official main | 111 local-only paths; 0 upstream-only and 0 diverged, classified in `docs/custom-codex-patch-map.md` |
| Web platform | Axum/PostgreSQL prototype plus the earlier loopback Web MVP |

## Reproduced evidence

- `scripts/codex-upstream-status.sh` reports `Status: synchronized`; the
  customization status script reports 111 local-only paths and no pending
  official or diverged paths.
- The locally built `codex app-server` completes `initialize` and returns
  `capabilityManifest`, `codexHome`, `platformFamily`, `platformOs` and
  `userAgent`.
- The observed manifest contains 18 declarations, including
  `models.providers`.
- `just fmt`, app-server Schema generation, app-server protocol, Chat transport,
  Provider/model-manager, app-server model-list and TUI scoped tests pass.
- The Web contract check and real `--require-manifest` app-server smoke pass.
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
| Capability negotiation | available, provisional | `initialize` emits schema version, build identity, protocol range, status, limits and reasons; Client/Server Request and Notification method registries now validate Manifest wire-name refs and experimental consistency, but capability declarations are still hand-assembled rather than fully generated from policy |
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
