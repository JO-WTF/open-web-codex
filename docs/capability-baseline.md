# Capability baseline

This document records checked-in, directly observed capability state. Product
requirements belong in `product-design.md`; planned work belongs in
`development-plan.md`.

## Snapshot

Observed on 2026-07-23 from the current synchronization branch:

| Component | State |
| --- | --- |
| Codex subtree | integrated through `openai/codex` `6e5a2d6b8d148a5554fdceb6f399ca45bd1c78d9` |
| Observed official main | `10cc57c95c2c8f1d01c8deaa75efb29b099d9c28`; 26 commits await the next dedicated sync branch |
| Local Codex seams | retained changes remain classified by `docs/custom-codex-patch-map.md`; compare them against `codex-upstream/main`, never this repository's `main` |
| Local customization footprint | six retained Runtime/TUI seams, derived artifacts and focused tests; `ToolName` uses the official implementation |
| Web platform | Restored browser UI, Axum/PostgreSQL platform, native Profile Registry/Host, encrypted Provider Secret injection, durable approvals, isolated Git workspaces, lease-based Run orchestration, typed REST resources and authenticated WebSocket |

## Reproduced evidence

- `scripts/codex-upstream-status.sh` reports the subtree synchronized with
  official main; the customization status script reports 126 local-only
  differences with zero upstream-only or diverged paths.
- The current upstream structure and all six documented seams are integrated;
  regenerated app-server Schema and TypeScript fixtures have no drift.
- The locally built `codex app-server` completes `initialize` and returns
  `capabilityManifest`, `codexHome`, `platformFamily`, `platformOs` and
  `userAgent`.
- The observed manifest contains 18 declarations, including
  `models.providers`.
- On the current Runtime lineage, `just fmt`, app-server and config Schema
  generation, 288 app-server protocol tests, 175 Chat transport tests, 26
  Provider metadata tests, 57 Provider transport tests, 41 model-manager tests,
  17 focused Core Chat tests, 27 focused Core Provider tests, five app-server
  model-list tests, and the 3,233-test full TUI suite pass.
- The latest official authentication routing, forked approval-reviewer,
  cross-environment Turn diff and `PathUri` canonicalization changes are
  integrated. Their focused evidence is 159 login, 19 app-server fork, 24 Core
  Turn diff, 86 apply-patch and 60 `PathUri` passing tests.
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
- Git Runtime validation covers source/ref rejection, private mirror creation,
  one clone per Run, locking, selected-path commit, status projection and
  cleanup. Run orchestration covers idempotent creation, `SKIP LOCKED` leasing,
  heartbeats, cancellation, recovery and authorized workspace binding.
- The browser uses only typed platform REST resources and an authenticated
  `/api/events/ws` stream. Durable events are replayed by Task sequence; live
  delivery is filtered by Organization. The former local gateway, raw RPC,
  query-token event stream and desktop application are absent.
- The checked-in 1421 WebApp React component tree and CSS match the established
  `main` WebApp byte-for-byte. `src/services/webClient.ts` is the only product
  source seam: it preserves the existing UI method contract while calling typed
  Server resources. Typecheck, production build, no-desktop and UI-parity gates
  pass; all 1,123 browser tests pass. Direct-Server tests cover history,
  reconnect/resync replay, status recovery, current-Thread checkout selection,
  Provider/model defaults, approvals, structured input, MCP and rate limits.
- The 1421 WebApp adapter currently covers managed Projects, Threads/Turns,
  durable events, approvals and structured input, Provider/model selection,
  Profile rate limits, MCP status, workspace files, Git status, message send,
  interrupt and steer. A real Codex/DeepSeek journey verifies Provider add and
  switch, code execution, real stdio MCP invocation, approval resolution,
  delayed Turn state, cross-Thread history restore, file preview and durable/live
  event ordering. Browser smoke additionally verifies Running-to-Idle sidebar
  convergence while switching Threads. The old root App Bridge has been deleted; both root and
  `/web` now load only WebApp, while unused old App source awaits later pruning.
- The latest official managed-config exact-value enforcement, missing sandbox
  path handling, and skill-name metrics sanitization changes are integrated.
  Their config, MCP, Core Skills, protocol and app-server regressions pass on
  the synchronized tree.
- The latest official loopback proxy allowlist behavior is integrated without
  adding a local customization. All 207 network-proxy tests pass: 206 pass in
  the local-port environment, while its DNS-failure case passes under network
  isolation because the host resolver otherwise synthesizes an address for the
  reserved `.invalid` name. The added CLI sandbox cases are Linux-only and do
  not compile as tests on the current macOS host.
- The latest official named `/new` and `/clear` session lifecycle is integrated
  through the upstream TUI structure; the Provider event and slash-command
  seams merge without a parallel session implementation.
- Official repository-rule review attribution, failed-turn TUI recovery, and
  inherited-FD Windows process-tree coverage are integrated without new local
  seams. The failed-turn TUI regression passes with the Provider dispatcher
  attachment intact.
- The `6e5a2d6b8d14` update is conflict-free against the retained seams. Its 63
  focused realtime-conversation tests pass, as do the HTTP client, LM Studio
  and model-manager suites. The broader scoped run passes 3,144 of 3,149 tests;
  the remaining five require binding local mock ports or launching a nested
  exec-server, both denied by the current validation sandbox.

The successful checks prove only the surfaces named above. They do not prove
multi-Profile process routing, production sandbox strength, complete Cookie/CSRF
security, Push delivery, or every Studio capability.

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
| Independent server | Axum server serves the browser, REST API, authenticated WebSocket, Profile Host and Runner from one deployable | deployment/config hardening and supervised production packaging remain |
| Persistence | PostgreSQL migrations cover users/sessions, organizations/memberships, Profiles/capabilities/encrypted Secrets, projects, tasks, Runs, leases, Workspaces, durable approvals/audit and versioned Run-event projections | artifacts, retention, legacy-row repair and complete constraints remain missing |
| Authentication | bootstrap and login use Argon2id, sessions bind an Organization, and legacy hashes upgrade after successful verification | HttpOnly-only session flow, CSRF, logout/revocation, rate limiting and complete browser flows are missing |
| Authorization | Project/Task/Run and runtime calls enforce session Organization; Provider/approval calls additionally enforce Profile ownership; a two-Organization denial regression passes | centralized policy abstraction, Project-specific roles and the full concurrent multi-user matrix remain missing |
| Codex bridge | Fake/Real adapter and event projection exist; Real uses the native Profile Registry/Host JSONL connection. Provider Secrets are encrypted and injected only into the owned child environment. Runtime-facing operations remain internal and browser routes are typed | composition is still one configured Profile process per server; per-user dynamic process routing remains incomplete |
| Task/Run | CRUD/start/cancel/message/steer/compact/review, idempotent scheduling, DB leases/heartbeats/recovery, isolated Git workspaces, safe Item/Delta and approval projection, monotonic replay, terminal execution, workspace files, nested Git roots, full local Git operations and explicit remote operations exist | artifact storage, approval expiry/restart delivery, protected-branch policy and full multi-Profile routing remain incomplete |
| Browser | established `main` 1421 WebApp UI and CSS run through typed resources for workspace/thread/message, approvals, Provider/model, MCP/rate-limit snapshots, files and Git status; the real core journey and Thread-switch browser smoke pass; no standalone Gateway or old root Bridge is loaded or built | broader visual/accessibility regression, unused old App source pruning, cookie-only sessions and production accessibility remain incomplete |

## Immediate capability gates

1. Derive the remaining Manifest method sets and experimental state from
   generated Codex protocol/build facts.
2. Keep the digest-addressed contract bundle and Web feature policy in sync.
3. Keep the passing real Thread restart/resume/read and Provider/approval/MCP
   journey as a release gate; add multi-cwd and multi-agent isolation coverage
   before promoting the corresponding declarations to product support.
4. Replace the single configured Profile composition root with authorized
   per-user Profile routing before multi-user Beta.
