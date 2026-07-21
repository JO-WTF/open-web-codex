# Capability baseline

This document records checked-in, directly observed capability state. Product
requirements belong in `product-design.md`; planned work belongs in
`development-plan.md`.

## Snapshot

Observed on 2026-07-21 from the current synchronization branch:

| Component | State |
| --- | --- |
| Codex subtree | integrated through `openai/codex` `51200321eb7b862a29ffceaba8b19db1934a9b38` |
| Observed official main | `7442f5f9323d116755dfe630e22c931a8aeaa5c7`; two commits await integration |
| Local Codex seams vs official main | 159 paths before replay: 119 local-only, 36 upstream-only and 4 diverged |
| Local customization footprint | six retained Runtime/TUI seams, one temporary upstreamed fix, derived artifacts and focused tests |
| Web platform | Axum/PostgreSQL platform, native Profile Registry/Host, encrypted Provider Secret injection, durable approvals and typed Provider routes plus remaining loopback Web MVP surfaces |

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
- Blank PostgreSQL migration and restart tests pass. AES-256-GCM Provider Secret
  storage is identity-bound, and the real app-server secured-Provider smoke
  proves that Codex config receives only a generated environment key while
  ciphertext and private child environment are removed together on deletion.
- The authenticated HTTP security regression uses two Organizations and proves
  resource-ID isolation, Profile-owner enforcement, session Organization
  switching, role-gated writes, durable approval decision delivery and an audit
  record. Passwords use Argon2id; accepted legacy SHA-256 hashes are upgraded on
  successful login.

The successful checks prove only the surfaces named above. They do not prove
multi-Profile scheduling, Runner/worktree isolation, authenticated WebSocket
subscriptions or production deployment security.

## Runtime capability assessment

| Capability | Manifest/source state | Validation status |
| --- | --- | --- |
| Protocol Schema | available | generated JSON/TypeScript artifacts exist |
| Capability negotiation | available, provisional | `initialize` emits schema version, build identity, protocol range, status, limits and reasons; method registries validate Manifest wire-name refs, experimental consistency, and product attribution policy; capability declarations remain hand-assembled Alpha subset rather than full generated policy |
| Thread lifecycle | declared supported | initialize smoke passed; multi-cwd and restart recovery smoke still required |
| Turn lifecycle | declared supported | real lifecycle smoke beyond initialize still required |
| Approval lifecycle | declared supported | command, file and permission requests are persisted before a request-id-free browser projection; decisions use optimistic versioning and audit. A real interactive approval smoke, expiry and restart recovery remain gates |
| Profile multi-workspace | declared supported | manifest limits are present; ownership and concurrency behavior remain unverified |
| Memory lifecycle | declared unsupported | Codex contains compaction/memory surfaces, but the Web-safe status/export/reset bridge is absent |
| Native Agent CRUD | declared unsupported | no stable Web-safe CRUD/validation contract |
| Multi-agent trajectory | declared experimental | Codex parent/child and collab events exist; fixture and real trajectory smoke are pending |
| Skills | degraded/unsupported by operation | listing is declared; safe write, validation and isolated testing are not enabled |
| Plugins | declared unsupported | do not enable Studio lifecycle or permissions UI |
| MCP | config degraded; OAuth/elicitation unsupported | status listing is declared; Web-safe CRUD, reload and lifecycle validation are pending |
| Tools discovery | declared unsupported | do not expose a platform fallback catalog |
| Structured reply cards / map cards | declared unsupported | no generated card contract, card Artifact store, renderer gate or real app-server smoke exists |
| Provider/model management | declared supported by the checked-in Runtime | `models.providers`, `modelProvider/list`, controlled Profile config writes, provider-scoped refresh, model selection and context-window persistence are wired; scoped Runtime/TUI tests, two-Provider cache-isolation smoke and encrypted platform Secret injection/deletion smoke pass. Turn-level Provider propagation remains a release gate |

## Web platform assessment

| Surface | Current state | Production gap |
| --- | --- | --- |
| Independent server | Axum server and Cargo workspace build structure exist | deployment/config hardening and end-to-end startup gate remain |
| Persistence | PostgreSQL migrations cover users/sessions, organizations/memberships, Profiles/capabilities/encrypted Secrets, projects, tasks, runs, Workspaces, durable approvals/audit and versioned run-event projections | leases, artifacts, jobs, retention, legacy-row repair and complete constraints remain missing |
| Authentication | bootstrap and login use Argon2id, sessions bind an Organization, and legacy hashes upgrade after successful verification | HttpOnly-only session flow, CSRF, logout/revocation, rate limiting and complete browser flows are missing |
| Authorization | Project/Task/Run and runtime calls enforce session Organization; Provider/approval calls additionally enforce Profile ownership; a two-Organization denial regression passes | centralized policy abstraction, Project-specific roles and the full concurrent multi-user matrix remain missing |
| Codex bridge | Fake/Real adapter and event fan-out exist; Real uses the native Profile Registry/Host JSONL connection. Provider Secrets are encrypted and injected only into the owned child environment. Provider and approval routes are typed and authenticated | composition is still one configured Profile/Workspace per server process; typed Thread/Turn operations and authenticated browser subscriptions remain incomplete, and legacy raw RPC/SSE code still exists behind an off-by-default flag |
| Task/Run | CRUD/start/cancel/message, safe Item/Delta and approval projection, monotonic cursor replay, Thread-history reconciliation and a real Profile restart/resume/read smoke exist | worktree provisioning, multi-user Profile routing, idempotent scheduler, approval expiry/recovery and authenticated subscriptions remain incomplete |
| Browser | loopback MVP can connect workspace, start Thread and send text; Provider catalog/write flows use typed authenticated platform resources rather than raw RPC | other flows still target the local preview Gateway and accept server paths; this is not yet the authenticated multi-user product UI |

## Immediate capability gates

1. Derive Manifest method sets and experimental state from generated Codex
   protocol/build facts; add Provider/model capability IDs.
2. Generate a digest-addressed contract bundle and separate it from Web product
   feature policy.
3. Keep the passing real Thread restart/resume/read smoke and add multi-cwd,
   Provider, approval, multi-agent and MCP smoke suites before promoting the
   corresponding declarations to product support.
4. Replace the remaining legacy raw RPC/SSE browser bridge with typed Task,
   Thread, Turn and Git DTOs plus authenticated WebSocket cursor replay.
