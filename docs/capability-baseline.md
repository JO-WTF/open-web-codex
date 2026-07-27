# Capability baseline

This document is the current capability evidence ledger. It answers which
Runtime and platform behaviors exist in the checked-in branch, how far they
were validated, and which claims are still unsafe to make. Product
requirements belong in `product-design.md`; stage order belongs in
`roadmap.md`; active work belongs in `development-plan.md`.

It is not a changelog or test manual. A source file, route, manifest
declaration, passing unit test and passing real end-to-end journey are different
evidence levels and must remain distinguishable. When a migration, protocol,
ownership boundary or Runtime revision changes, earlier integration results do
not remain valid automatically.

## Snapshot

Observed on 2026-07-27 from the current working branch:

| Component | State |
| --- | --- |
| Codex subtree | integrated through `openai/codex` `6e5a2d6b8d148a5554fdceb6f399ca45bd1c78d9` |
| Observed official main | `cba0e2701c9e3e67a877a16dbbd7a577d477a630`; 126 commits await the next dedicated sync branch |
| Local Codex seams | retained changes remain classified by `docs/custom-codex-patch-map.md`; compare them against `codex-upstream/main`, never this repository's `main` |
| Local customization footprint | six retained Runtime/TUI seams, derived artifacts and focused tests; `ToolName` uses the official implementation |
| Web platform | Restored browser UI, Axum/PostgreSQL platform, native Profile Registry/Host, encrypted Provider Secret injection, durable approvals, independent authorized managed Workspaces, lease-based Run orchestration, typed REST resources and authenticated WebSocket |

## Reproduced evidence

- `scripts/codex-upstream-status.sh` reports the subtree integrated through
  `6e5a2d6b8d14` with 126 official commits awaiting a dedicated sync. The
  customization status script reports 919 raw path differences: 790
  upstream-only, 70 local-only and 59 diverged.
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
- The disposable-PostgreSQL Enterprise Supervisor journey completes nine
  functional checks through the real Codex Runtime: one Policy-bound root
  Thread creates the exact `data_agent` and `network_planning_agent` Runtime
  Roles, the first child produces a `planning-dataset.v1` Resource, the second
  child reads that same Resource before calculation, and the latest rerun uses
  36 enterprise MCP calls to produce twelve ready Task Artifacts before the root
  returns a six-section, evidence-citing warehouse decision report. The browser
  history endpoint and the composed Policy/Agent/Artifact overview recover the
  same result. Exact call and Artifact counts are trajectory evidence, not a
  product invariant; the E2E gates the required schema minimums and requires
  every registered Artifact to become ready.
- The platform source contains PostgreSQL migrations and API handlers for
  bootstrap/session, organization membership, project, Task, Run and persisted
  Run events. The current single-user Server ensures an implicit local Owner;
  the browser obtains a local Session and enters the WebApp without login or
  registration. Username/password login and its Argon2id implementation remain
  in the Server for later multi-user restoration but are not exposed by the
  current browser entry.
- The native Profile Host real-binary smoke covers an offline Turn, paginated
  full-item history, process-instance rotation, restart and Thread resume/read.
  A second real-binary Provider smoke covers two custom
  Providers, forced model refresh, switching, cache isolation and omission of
  direct credentials from returned catalogs.
- The source contains ignored PostgreSQL migration/restart and two-Organization
  security integration suites, but the base migrations changed with the
  independent Workspace schema and those suites have not been rerun against a
  fresh database in the current environment. Their earlier results are not
  current evidence. Focused non-PostgreSQL Rust tests pass; fresh migration,
  restart, cross-Organization denial and recovery remain required gates.
- AES-256-GCM Provider Secret storage is identity-bound, and the real
  app-server secured-Provider smoke proves that Codex config receives only a
  generated environment key while ciphertext and private child environment are
  removed together on deletion. The security integration source additionally
  covers Profile-owner enforcement, session Organization switching,
  role-gated writes, durable approval delivery, uncertain-delivery retry,
  Runtime request-id reuse, stale-request cancellation and audit, but its
  database-backed result is pending the fresh-schema rerun. Passwords use
  Argon2id; accepted legacy SHA-256 hashes are upgraded on successful login.
- Git Runtime validation covers source/ref rejection, private mirror creation,
  explicit managed Workspace provisioning, locking, selected-path commit,
  status projection and explicit cleanup. A Workspace now has its own identity,
  grant and lifecycle; a Run selects `workspace_id`, and Run execution,
  cancellation, lease expiry and recovery neither create nor remove its
  checkout. The adapter passes the authorized root through the official Codex
  `cwd` fields. Registration of existing roots, shared-Workspace concurrency
  and multi-`cwd` behavior still require database and real-Runtime validation.
- The browser uses only typed platform REST resources and an authenticated
  `/api/events/ws` stream. Initial navigation snapshots establish the latest
  durable Task cursor after subscription; reconnects replay only later
  sequences, and live delivery is filtered by Organization. The former local gateway, raw RPC,
  query-token event stream and desktop application are absent.
- The checked-in 1421 WebApp presentation and CSS match the established WebApp.
  `src/services/webClient.ts` is the primary compatibility seam; three complete
  source-file hashes pin the reviewed non-visual Thread-context wiring in
  `WebApp.tsx` and FileManager so the exception cannot expand into UI drift.
  Typecheck, production build and no-desktop gates pass; all 1,221 browser
  tests pass. The UI-parity report still records the deliberate browser UI
  extensions that have not yet been folded into its reference baseline.
  Direct-Server tests cover authoritative history,
  reconnect/resync replay, status recovery, direct authorized Workspace file
  and Git projection,
  Provider/model defaults, approvals, structured input, MCP and rate limits.
- The 1421 WebApp adapter currently covers managed Projects, Threads/Turns,
  durable events, approvals and structured input, Provider/model selection,
  Profile rate limits, MCP status, workspace files, Git status, message send,
  interrupt and steer. A real Codex/DeepSeek journey verifies Provider add and
  switch, code execution, real stdio MCP invocation, approval resolution,
  delayed Turn state, cross-Thread history restore, file preview and durable/live
  event ordering. Browser smoke additionally verifies Running-to-Idle sidebar
  convergence while switching Threads. Both root and `/web` load only WebApp;
  the old root App/Bridge remain as unreferenced source and are intentionally
  deferred from pruning.
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
| Thread lifecycle | declared supported | real start, persisted full-history pagination, process restart, resume and read smoke passed. New platform Threads select official paginated history. Unloaded Threads resume with `excludeTurns`; the adapter concurrently reads indexed `thread/turns/list(itemsView=notLoaded)` and `thread/items/list` pages and joins Items by stable Turn id. Canonical JSONL remains authoritative: paginated reads catch a lagging SQLite projection up under the Thread writer lock, safely rebuild an inconsistent per-Thread projection only after proving the complete rollout has contiguous ordinals, and correctly decode flattened token-count records with nested rate-limit numbers. The real enterprise Turn and its complete six-section report restore through paginated history after Server/Profile Host restart. Existing legacy rollout histories retain one isolated `thread/turns/list(itemsView=full)` compatibility branch until those Profile histories are retired. The Server inserts durable platform approval projections into their original Turn/sequence positions. The browser does not restore approvals from local storage, and whitespace-only Agent messages are omitted. Multi-cwd and official `cwd` authorization remain gates |
| Turn lifecycle | declared supported | real offline Turn start/completion and post-restart recovery passed |
| Approval lifecycle | declared supported | command, file and permission requests are persisted before a request-id-free browser projection; decisions use optimistic versioning and audit; process-instance identity prevents stale response delivery and supports request-id reuse; uncertain delivery retry and restart cancellation regressions pass. Expiry remains a gate |
| Profile multi-workspace | declared supported | manifest limits are present; ownership and concurrency behavior remain unverified |
| Memory lifecycle | declared unsupported | Codex contains compaction/memory surfaces, but the Web-safe status/export/reset bridge is absent |
| Native Agent CRUD | declared unsupported | no stable Web-safe CRUD/validation contract |
| Multi-agent trajectory | declared experimental | The Server persists a rebuildable root/child Thread projection from official `thread/started`, `thread/status/changed` and collaboration events, associates child events with the authorized root Run, and prevents child lifecycle events from completing or otherwise mutating the root Run. A read-only `/api/runs/{run_id}/agents` resource exposes bounded projection facts. The real enterprise journey proves ordered spawn/wait execution for one root and two exact-role child Threads, child MCP activity association, browser refresh and Server/Profile Host restart recovery. General follow-up, interrupt, deeper trees, partial failure and multi-user isolation remain required before product support can be promoted beyond the constrained case |
| Enterprise Supervisor Policy | real single-Profile case validated | The code-published `enterprise-supervisor-copilot@1.0.0` Policy is resolved server-side, hashed into an immutable Snapshot, bound to the Run and actual root Thread, and injected only through official `thread/start.developerInstructions`. Enqueue resolves only the published id/version and seals the Snapshot. The worker is the sole authoritative gate: immediately before every root `thread/start` or inherited `thread/fork`, it re-resolves the published Policy, rejects any Snapshot digest/instruction drift, requires `agents.multi_agent@1.0.0` and `agents.multi_agent_v1_backend_override@1.0.0` (supported or explicitly enabled experimental) before it calls Runtime, projects the immutable Role templates through the Profile Host, and verifies the target workspace-effective `config/read` view. The second capability gates the typed, request-scoped `thread/start` or `thread/fork` field `multiAgentBackend: "v1"`: Runtime gives it precedence over model metadata and a V2 fork lineage, without persisting a Profile or Project backend choice. A Runtime that does not declare that exact capability is rejected before it can silently ignore the field. Protected multi-agent/Role keys from a Project layer are rejected; the Host reopens the exact managed Role files without following links and checks their SHA-256; the official per-thread config overrides carry only the verified Role and multi-agent settings into Runtime load. The durable `profile_runtime_role_projections` configuration record is distinct from rebuildable `runtime_agent_projections`; Profile Role CRUD cannot manage or override platform Role names. Role conflict, Definition version/template-hash drift, disabled configuration and insufficient limits fail closed without a default Role. Only governed Supervisor starts and forks carry the V1 selector; ordinary Root sessions do not activate, mutate, or receive this multi-agent setup. The real warehouse case completes under the bound version and the browser restores the same binding |
| Enterprise Agent Definitions | real exact-role spawn validated | Code-managed `enterprise-data-agent@1.0.0` and `enterprise-network-planning-agent@1.0.0` manifests declare responsibilities, Artifact inputs/outputs, required capabilities, risks, exact `data_agent` / `network_planning_agent` Runtime Role references, and immutable TOML templates with content hashes. The selected Supervisor Policy, rather than a generic platform startup path or browser Role CRUD, is the only trigger for their Profile Runtime Role projection. The real journey proves both verified Roles are discovered and used by child `thread/started` provenance; unknown-role failure and broader catalog governance remain separate tests |
| Skills | degraded/unsupported by operation | listing is declared; a real new Thread proves the selected `local-maps-mcp` capability root injects the `map-utils` Skill into Runtime context. Profile-wide listing does not enumerate Thread-selected roots; safe write, validation and isolated testing are not enabled |
| Plugins | declared unsupported | do not enable Studio lifecycle or permissions UI |
| MCP | enterprise case available; Studio config degraded | The real Supervisor journey uses a read-only `supply_chain_data` MCP and a bounded `supply_chain_planner` MCP through Runtime discovery. It proves source inspection/build/validation, cross-child Resource read, snapshot and route registration, actual/optimized/candidate evaluations, comparisons and validation; child elicitation approvals are durably captured through the root Run's Agent tree. General OAuth/full-form elicitation and MCP Studio lifecycle remain unsupported. `map_utils` retains its separately tested typed configuration and `map.v3` contract |
| Tools discovery | declared unsupported | do not expose a platform fallback catalog |
| Structured reply cards / map cards | available | `create_map_card` publishes one `map.v3` Artifact contract. Sources are platform-managed GeoJSON with mutually exclusive standard inline `data` or Open Web `data_ref`; authorized Resource refs become opaque Artifact URLs. Layers are official Mapbox Style Specification Layer JSON, validated by `@mapbox/mapbox-gl-style-spec` and passed to `map.addLayer` unchanged except for browser-local layer/source IDs. There is no hand-maintained paint/layout/filter/expression whitelist or map-specific source/layer/payload count limit. Official unknown-property diagnostics are warnings; invalid known syntax fails. Standard camera fields are top-level, while optional text-only hover and legend behavior lives under `extensions`. The Server independently projects the stable browser DTO, and Assistant messages render only explicit standalone Artifact embed directives. |
| Provider/model management | declared supported by the checked-in Runtime | `models.providers`, `modelProvider/list`, controlled Profile config writes, provider-scoped refresh and context-window persistence are wired. A model refresh or context-window edit schedules Server-owned app-server replacement at the next safe Turn boundary: an in-flight Turn is preserved, the adapter invalidates process-local bindings, resumes the same persisted Thread and starts its next Turn against the rebuilt model catalog. The browser groups built-in, local and custom Providers from Runtime-supplied kinds, defaults the built-in and local groups closed, distinguishes the LM Studio and Ollama `gpt-oss` entries, and switches by clicking the Provider row. Editable model context windows use one save action that persists every changed model sequentially so catalog replacements cannot race. The current Provider and model use a shared high-contrast selected treatment in both themes. The platform stores the last Provider/model pair as a global default and copies it into every new Task; each existing Thread keeps its own database-backed pair. Scoped Runtime/TUI tests, two-Provider cache-isolation smoke, encrypted platform Secret injection/deletion smoke, live existing-Thread Provider transport rebinding and real same-Thread next-Turn context refresh pass |

## Web platform assessment

| Surface | Current state | Production gap |
| --- | --- | --- |
| Independent server | Axum server serves the browser, REST API, authenticated WebSocket, Profile Host and Runner from one deployable; the single-host deployer builds locked Release artifacts, securely provisions or verifies the fixed `open_web_codex` database, keeps verbose output in bounded logs, health-checks rollout and persists non-secret status metadata | HTTPS reverse proxy, OS supervision, rollback, backup/restore and remaining config hardening are still external GA gates |
| Persistence | PostgreSQL migrations cover users/sessions, organizations/memberships, Profiles/capabilities/encrypted Secrets, projects, tasks, Runs, leases, Workspaces, durable approvals/audit, immutable Supervisor Policy snapshots/bindings, durable Profile Runtime Role configuration projections (`profile_runtime_role_projections`) that are distinct from rebuildable Runtime Agent execution observations (`runtime_agent_projections`), Task-owned typed Artifacts with producer provenance, and versioned Run-event projections. The latest real enterprise case materializes twelve ready Artifacts and reads them across child Threads without exposing internal Resource URIs or host paths. Fresh-schema tests prove deleting a producer Run does not delete the Artifact, its Task grant or immutable provenance identity | Artifact replacement, invalidation, deletion, retention, cross-Run reuse and complete multi-user constraints remain missing |
| Authentication | current single-user startup creates an implicit local Owner and the browser obtains a local Session without credentials; sessions still bind an Organization and all resource authorization remains active; retained bootstrap/login use Argon2id | interactive login/registration is intentionally absent; public or multi-user deployment requires restoring authentication, HttpOnly-only sessions, CSRF, rate limiting and complete logout/revocation flows |
| Authorization | Project/Task/Run and runtime routes contain session-Organization checks; Provider/approval routes additionally check Profile ownership. The two-Organization denial suite passes against the current fresh schema, including Artifact ID guessing and Profile-bound operations | centralized policy abstraction, Project-specific roles and the full concurrent multi-user matrix remain missing |
| Codex bridge | Fake/Real adapter and event projection exist; Real uses the native Profile Registry/Host JSONL connection. Provider Secrets are encrypted and injected only into the owned child environment. Runtime-facing operations remain internal and browser routes are typed | composition is still one configured Profile process per server; per-user dynamic process routing remains incomplete |
| Task/Run | Source, contracts and tests implement CRUD/start/cancel/message/steer/compact/review, idempotent scheduling, DB leases/heartbeats/recovery, independent managed Workspace creation/grants/removal, authoritative Codex history, safe Item/Delta and approval projection, terminal execution, workspace files and Git operations. Runs only reference an authorized Workspace. Enterprise Runs additionally bind an immutable Supervisor Policy, create a root Runtime Agent projection in the same delivery transaction, and authorize approvals from known child Threads through the Agent tree | add existing-root registration, shared-Workspace concurrency and real multi-`cwd` validation. Approval expiry, protected-branch policy and full multi-Profile routing remain incomplete |
| Browser | established WebApp presentation runs through typed resources for workspace/thread/message, approvals, Provider/model, MCP/rate-limit snapshots, files and Git status. Workspace listing and file/Git/terminal/GitHub operations use Workspace IDs directly; Thread/Turn and MCP operations retain Run provenance where the Runtime Thread is required. Thread creation selects an existing Workspace and creates only Task/Run records. A separate Workspace action starts the exact published Enterprise Supervisor Policy; the browser cannot submit Policy contents. Reopening the latest real warehouse Thread restores the bound Policy, root plus two Runtime Agents, twelve ready evidence Artifacts with producer roles, the human-readable collaboration trace and the complete six-section report after Server/Profile Host restart. A shared Agent/Files right sidebar now activates only after a real child Agent exists. Its pinned Supervisor summary shows the main task, current status, safe behavior and latest public progress; child work is a streamed sequence of task nodes bound to real Runtime Turns, so a completed node remains immutable and reusing the same Agent Thread creates a new node. Background Agent updates produce an unread indicator without replacing the active Files tab. Runtime projections omit tool arguments/results, reasoning, internal Resource URIs and host paths while retaining useful public context | make child Agent task nodes navigable to their authoritative histories, add Artifact content/open actions, then cover deeper trees, failure/approval states, narrow layouts, existing-root registration, shared-Workspace concurrency and cross-user denial |

## Immediate capability gates

1. Derive the remaining Manifest method sets and experimental state from
   generated Codex protocol/build facts.
2. Keep the digest-addressed contract bundle and Web feature policy in sync.
3. Keep the passing real Thread restart/resume/read and Enterprise Supervisor
   journey as release gates; add multi-cwd, multi-agent failure and multi-user
   isolation coverage before promoting the experimental declarations.
4. Replace the single configured Profile composition root with authorized
   per-user Profile routing before multi-user Beta.
5. Complete independent Workspace validation: cover shared-Workspace
   concurrency, cross-user denial, resume, explicit lifecycle, existing-root
   registration, multi-`cwd` behavior and Git-operation containment. Start with
   a blank PostgreSQL database so the rewritten base migrations and denial
   suites become current evidence.
6. Complete the Task-owned Artifact lifecycle: replacement, invalidation,
   deletion, retention and authorized cross-Run reuse, while keeping
   Run/Thread/Turn/Item only as provenance.
