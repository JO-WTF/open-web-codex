# Capability baseline

This document records checked-in, directly observed capability state. Product
requirements belong in `product-design.md`; planned work belongs in
`development-plan.md`.

## Snapshot

Observed on 2026-07-21 from source commit
`4cfec57157da3bb1811ba83f793f21845485ca5d`:

| Component | State |
| --- | --- |
| Codex subtree | integrated through `openai/codex` `51200321eb7b862a29ffceaba8b19db1934a9b38` |
| Observed official main | `51200321eb7b862a29ffceaba8b19db1934a9b38`; no commit awaits integration |
| Local Codex seams vs official main | 123 local-only paths: 25 added and 98 modified; no upstream-only, diverged, or missing local paths |
| Local customization footprint | six retained Runtime/TUI seams, one temporary upstreamed fix, derived artifacts and focused tests |
| Web platform | Axum/PostgreSQL prototype, native Profile Host and typed Provider service/routes plus the remaining loopback Web MVP surfaces |

## Reproduced evidence

- `scripts/codex-upstream-status.sh` reports `Status: synchronized`; the
  customization status script reports 123 local-only differences and zero
  upstream-only or diverged paths.
- The current upstream structure and all six documented seams are integrated;
  regenerated app-server Schema and TypeScript fixtures have no drift.
- The locally built `codex app-server` completes `initialize` and returns
  `capabilityManifest`, `codexHome`, `platformFamily`, `platformOs` and
  `userAgent`.
- The observed manifest contains 18 declarations, including
  `models.providers`.
- On the current Runtime lineage, `just fmt`, app-server Schema generation,
  287 app-server protocol tests, 174 Chat transport tests, the Core Chat endpoint
  integration test, 83 Provider tests,
  the 970-test app-server suite, 41 focused Plugin tests, and current TUI
  diff/highlight/Provider/terminal tests pass. The final upstream delta only
  adds a Unix compile gate to the TUI restore helper and is covered by the
  35-test terminal/Provider target.
- The Web contract check passes, and the locally built current Codex CLI passes
  the real app-server initialize Smoke with 18 Capability Manifest declarations.
- The platform source contains PostgreSQL migrations and API handlers for
  bootstrap/session, organization membership, project, Task, Run and persisted
  Run events.
- The native Profile Host real-binary smoke covers an offline Turn, restart and
  Thread resume/read. A second real-binary Provider smoke covers two custom
  Providers, forced model refresh, switching, cache isolation and omission of
  direct credentials from returned catalogs.

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
| Provider/model management | declared supported by the checked-in Runtime | `models.providers`, `modelProvider/list`, controlled Profile config writes, provider-scoped refresh, model selection and context-window persistence are wired; scoped Runtime/TUI tests and a real two-Provider switch/refresh/cache-isolation smoke pass. Turn-level Provider propagation and encrypted platform Secret injection remain release gates |

## Web platform assessment

| Surface | Current state | Production gap |
| --- | --- | --- |
| Independent server | Axum server and Cargo workspace build structure exist | deployment/config hardening and end-to-end startup gate remain |
| Persistence | PostgreSQL migrations cover users/sessions, organizations/memberships, projects, tasks, runs and versioned run-event projections with monotonic replay sequence | Profiles, Workspaces, approvals, leases, audit, artifacts, jobs, retention and complete constraints are missing |
| Authentication | bootstrap, password session creation and auth extractor exist | HttpOnly-only session flow, CSRF, logout/revocation, rate limiting and complete tests are missing |
| Authorization | membership checks exist on part of the organization surface | centralized resource/action RBAC and cross-user denial matrix are missing |
| Codex bridge | Fake/Real adapter and event fan-out exist; Real uses the native `profile-host` JSONL connection. Provider CRUD/model refresh use a reusable typed service and authenticated `/api/providers` routes; Tauri compatibility calls the same service | the server still has a transitional single-Profile/single-Workspace composition and raw `rpc` for other legacy domains; durable Profile registry/ownership, encrypted Secret injection, typed Thread/Turn operations, persistent approvals and authenticated browser subscriptions remain incomplete |
| Task/Run | CRUD/start/cancel/message, safe Item/Delta projection, monotonic cursor replay, Thread-history reconciliation and a real Profile restart/resume/read smoke exist | worktree provisioning, multi-user Profile routing, idempotent scheduler, approvals and authenticated subscriptions remain incomplete |
| Browser | loopback MVP can connect workspace, start Thread and send text; Provider catalog/write flows use typed authenticated platform resources rather than raw RPC | other flows still target the local preview Gateway and accept server paths; this is not yet the authenticated multi-user product UI |

## Immediate capability gates

1. Derive Manifest method sets and experimental state from generated Codex
   protocol/build facts; add Provider/model capability IDs.
2. Generate a digest-addressed contract bundle and separate it from Web product
   feature policy.
3. Keep the passing real Thread restart/resume/read smoke and add multi-cwd,
   Provider, approval, multi-agent and MCP smoke suites before promoting the
   corresponding declarations to product support.
4. Replace the legacy raw RPC/SSE browser bridge with authenticated platform
   DTOs, durable event cursors and server-side ownership checks.
