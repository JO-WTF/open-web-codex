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

Observed on 2026-08-03 from the current working branch:

| Component | State |
| --- | --- |
| Codex subtree | integrated through `openai/codex` `6e5a2d6b8d148a5554fdceb6f399ca45bd1c78d9` |
| Observed official main snapshot | `95637f7056835fea66bdd0044414af480fc0fd74`; observed 2026-07-28 with 142 commits pending, not refreshed or synchronized in this iteration |
| Local Codex seams | retained changes remain classified by `docs/custom-codex-patch-map.md`; compare them against `codex-upstream/main`, never this repository's `main` |
| Local customization footprint | seven retained Runtime/TUI seams, derived artifacts and focused tests; `ToolName` uses the official implementation |
| Web platform | Restored browser UI, Axum/PostgreSQL platform, native Profile Registry/Host, encrypted Provider Secret injection, durable approvals, independent authorized managed Workspaces, lease-based Run orchestration, typed REST resources and authenticated WebSocket |

Thread-first Data Intake contracts and persistence are now present in the checked-in
platform: Thread startup can reuse an authorized existing Workspace without requiring a
Dataset Release; Workspace-wide SourceAsset discovery, bounded Excel/CSV/JSON profiling,
versioned generic warehouse-network contract metadata, InputRequest projection, confirmation revisions and
same-Thread immutable Release/Binding/Snapshot startup are typed and persisted. This is
implementation evidence only. No claim of end-to-end Indonesia planning readiness is made
until real Runtime/MCP profiling, domain validation, Release provenance and Browser
restart/recovery journeys pass the validation gates below.

## Reproduced evidence

- `scripts/codex-upstream-status.sh` reports the subtree integrated through
  `6e5a2d6b8d14`. The last verified 2026-07-28 upstream snapshot was
  `95637f705683` with 142 commits pending; this iteration deliberately does not
  refresh or synchronize that snapshot. The
  customization status script reports 951 raw path differences: 807
  upstream-only, 69 local-only and 75 diverged.
- The current upstream structure and all seven documented seams are integrated;
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
- The current organization Draft `Indonesia Network Planning Copilot`
  (`enterprise-supervisor-copilot@5.2.0`) binds Data `5.1.0`, Network `5.1.0`
  and Visualization `2.0.0` as optional governed Roles for the Workspace-wide
  Network Planning intake; no repository Supervisor Release is currently
  published. Platform behavior contract `1.1.0`
  requires evidence-gap planning and exact Artifact handoffs without prescribing
  a fixed Agent order or role count. Fifteen conditional contracts cover the
  requirement/source/mapping/gap/dataset/readiness evidence and the generic
  network snapshot, coverage, optimization, comparison, report and map handoffs.
  The 5.2.0 response contract requires a business-language status, completed-work
  summary, every relevant field mapping, each evidence-backed multi-source conflict,
  parameter status and next action; it does not treat an internal handoff as a
  completed planning result.
  Normal intake has no static example-source tools or fallback. The separate
  `supply_chain_demo` Server exposes one explicit, deterministic empty-Workspace
  generator and preserves `synthetic_demo` provenance. Its generic contract uses
  city-to-city routes and origin-region-to-destination-city rates; the explicit large
  template has 120,000 demand points while keeping both lane tables at 144 rows. The Learn tutorial is a
  distinct `indonesia-warehouse-network@1.5.0` Blueprint installation.
  Network/Data/Visualization MCP checks pass locally, but the real browser +
  Runtime journey for Excel, CSV, JSON, restart recovery and duplicate-Turn
  prevention has not yet passed; this capability therefore remains unavailable.
- Focused disposable-PostgreSQL journeys initialize the current schema from
  empty, publish immutable Workspace Dataset and capability-package Releases,
  bind exact Release IDs and content hashes to an organization Agent Release,
  reject cross-Organization access, and re-resolve the derived Runtime Role,
  Dataset and package dependencies during governed Run preflight. Deliberate
  dependency-hash corruption is rejected rather than silently resolved.
  Repository and Web tests also cover a direct root Agent Run without wrapping
  it in a one-Agent Supervisor. The current delivery-audit tutorial Release has
  passed a real Runtime/browser run with one exact MCP approval and Tool call,
  a ready `delivery_audit_report.v1` Artifact, no shell execution and complete
  history recovery.
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
- The focused Agent publication journey proves the current migration set
  initializes on an empty database. The broader ignored PostgreSQL
  migration/restart and two-Organization security integration suites have not
  all been rerun against this schema in the current environment. Their earlier
  results are not automatically current evidence; comprehensive restart,
  cross-Organization denial and recovery remain required gates.
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
- The checked-in WebApp presentation and CSS match the established WebApp.
  `src/services/webClient.ts` is the narrow current-contract UI adapter; three complete
  source-file hashes pin the reviewed non-visual Thread-context wiring in
  `WebApp.tsx` and FileManager so the exception cannot expand into UI drift.
  Typecheck, lint, production build and no-desktop gates pass; all 1,270 browser
  tests pass. The UI-parity report still records the deliberate browser UI
  extensions that have not yet been folded into its reference baseline.
  Direct-Server tests cover authoritative history,
  reconnect/resync replay, status recovery, direct authorized Workspace file
  and Git projection,
  Provider/model defaults, approvals, structured input, MCP and rate limits.
- The WebApp adapter currently covers managed Projects, Threads/Turns,
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
| Approval lifecycle | declared supported | command, file, permission and MCP elicitation requests are persisted before a request-id-free browser projection; decisions use optimistic versioning and audit; process-instance identity prevents stale response delivery and supports request-id reuse; uncertain delivery retry and restart cancellation regressions pass. Pending approvals from authorized child Agent Threads are projected into a Task-level queue and Agent Activity approval cards, remain actionable after durable replay, and lock repeated browser decisions while dispatching. Expiry and real rejection/recovery composition remain gates |
| Profile multi-workspace | declared supported | manifest limits are present; ownership and concurrency behavior remain unverified |
| Memory lifecycle | declared unsupported | Codex contains compaction/memory surfaces, but the Web-safe status/export/reset bridge is absent |
| Native Agent CRUD | declared unsupported | no stable Web-safe CRUD/validation contract |
| Multi-agent trajectory | declared experimental; constrained projection/lifecycle path verified, complete tutorial journey pending | The Server persists rebuildable root/child Thread and child-task execution projections from official collaboration, Thread, Turn, Item and approval events, associates child events with the authorized root Run, and prevents child lifecycle events from completing or otherwise mutating the root Run. Read-only `/api/runs/{run_id}/agents`, `/api/runs/{run_id}/agent-executions` and `/api/runs/{run_id}/agent-activities` resources expose bounded browser facts. The activity resource reconstructs an ordered behavior log from durable Run events and omits reasoning, tool arguments/results, internal Resource identifiers and host paths. Targetless V2 `wait_agent` cycles are represented truthfully as bounded waits for any Agent update or new input; when the event history or child-task projection provides an assigned task, each wait activity also carries a short bounded task/progress description, while both start and completion remain visible after refresh without inventing target Agents. Agent assignment starts, progress reports and final reports are represented as separate activities with bounded summaries in their titles and detailed text in their descriptions. The verified run also showed that Runtime may start another full bounded wait after a child has already completed, adding up to 120 seconds without changing correctness; the UI remains explicit and nonblank, but reducing that latency is still a Runtime behavior/performance gap rather than a platform fallback opportunity. Each observed child Turn has a stable task node; terminal rows are frozen, a later Turn on the same Agent Thread receives the next ordinal, and different child Threads can remain active concurrently. PostgreSQL integration covers prompt/event reordering, concurrent child activity, same-Agent reuse, terminal immutability and cross-Organization denial. Task-scoped Artifact resolution accepts an exact unique child-produced Resource only after verifying producer provenance for the same Run, so a root report can safely resolve a child map without requiring the producer and consumer to share one Thread. Same-Agent follow-up, root interruption and recovery are verified for the constrained case. Deeper trees, partial failure, approval rejection, restart composition and multi-user isolation remain required before product support can be promoted beyond it |
| Enterprise Supervisor Policy | built-in `4.0.0` compiler/DB; local contract/catalog checks pass; real journey unavailable | The policy binds optional Data `4.0.0`, Network `4.0.0` and Visualization `2.0.0` Roles. It declares fifteen evidence and result handoffs, but no fixed workflow. Platform projection accepts only the policy-declared producer/schema pairs and persists bounded Intake evidence. Real browser + Runtime validation for Excel, CSV, JSON, restart recovery and duplicate-Turn prevention remains a release gate. |
| Enterprise Agent Definitions | built-in Data `4.0.0`, Network `4.0.0`, Visualization `2.0.0`; local contract/catalog checks pass; real journey unavailable | Data discovers bounded authorized Workspace Excel/CSV/JSON sources and publishes source profiles/mapping candidates without exposing raw rows or host paths. Network owns requirement profiles, gaps, readiness and generic planning results; Visualization renders only the exact comparison-map resources. Strict normalization and generic planner checks pass locally, while real multi-format Runtime/MCP recovery is still required. |
| Skills | bounded package authoring available; Runtime listing degraded/unsupported by operation | Agent Studio lists checked-in packages and can publish one Workspace-scoped package containing a bounded Skill, declared MCP Tools and standard-library Python implementation. The Platform validates package shape and probes the generated MCP Server, while a new Thread still depends on Codex Runtime capability-root and Skill discovery. Exact package Releases can be attached to a published Agent rather than installed globally. Profile-wide Runtime listing still does not enumerate Thread-selected roots, so it cannot prove selected Skill injection; general Skill install/update/delete and Profile-scoped lifecycle remain unsupported |
| Plugins | general lifecycle declared unsupported | The Python authoring slice emits one fixed package shape under an authorized Workspace, but does not expose arbitrary Plugin manifests, launch commands, permissions, Profile installation or Plugin CRUD |
| MCP | reviewed declarations plus bounded Python package authoring; current tutorial Runtime execution verified | Agent Studio lists reviewed MCP declarations separately from current Thread status. Its Python editor accepts bounded Tool JSON Schemas and standard-library functions, generates a fixed stdio MCP launcher, clears inherited environment, validates startup/discovery, can test one declared Tool against an exact authorized Workspace Dataset Release, and atomically publishes an immutable Workspace package. Workspace resolution happens server-side; browser DTOs and Tool diagnostics do not expose local paths. The delivery-audit run verified one explicit MCP approval, exact Dataset identity, one Tool call and a durable Resource. Supply-chain Root has no business MCP, and governed preflight gives each child Role only its exact Server/Tool allowlist while rejecting sibling-package visibility. Secret-backed servers, OAuth/full-form elicitation, arbitrary commands/transports and general MCP config/reload/delete remain unsupported |
| Tools discovery | declared unsupported | do not expose a platform fallback catalog |
| Structured reply cards / report and map cards | typed Artifact persistence verified; Web fix focused-verified, post-fix real-browser check pending | The report Tool publishes an authoritative `indonesia_decision_report.v1` Resource and a typed `report.v1` delivery Artifact; `create_map_card` publishes one `map.v3` Artifact. Map sources are platform-managed GeoJSON with mutually exclusive standard inline `data` or Open Web `data_ref`; authorized Resource refs become opaque Artifact URLs. Layers are official Mapbox Style Specification Layer JSON, validated by `@mapbox/mapbox-gl-style-spec` and passed to `map.addLayer` unchanged except for browser-local layer/source IDs. There is no hand-maintained paint/layout/filter/expression whitelist or map-specific source/layer/payload count limit. Official unknown-property diagnostics are warnings; invalid known syntax fails. Standard camera fields are top-level, while optional text-only hover and legend behavior lives under `extensions`. The Server attaches stable, typed `inlineArtifacts` DTOs to live and restored Agent Message events; those DTOs, not model-authored report prose, Tool payload text or escaped model JSON, are the browser rendering authority. Report acceptance validates the authoritative Resource schema/version, payload digest, exact sources and Tool inputs, typed checks, Dataset Release identity, producer provenance, typed delivery identity and recovery convergence; it does not compare report wording or model text. The pure Learn UI run proved both delivery Artifacts were ready but exposed zero card DOM nodes because the DTOs were attached to collapsed Runtime `commentary`, while `final_answer` contained no embeds. The Web owning-layer fix preserves collapsed commentary prose and independently renders only authorized typed cards in Turn item order, deduped by stable ref without changing phase. Live/restored DOM regression coverage and 58 focused tests pass; a controller URL-policy rejection after service restart prevented the required post-fix real-browser display/recovery check. A ready map card, summary and legend do not require credentials; rendering the geographic basemap additionally requires a configured browser-public Mapbox token and otherwise shows an explicit configuration action. |
| Provider/model management | declared supported by the checked-in Runtime | `models.providers`, `modelProvider/list`, controlled Profile config writes, provider-scoped refresh and context-window persistence are wired. A model refresh or context-window edit schedules Server-owned app-server replacement at the next safe Turn boundary: an in-flight Turn is preserved, the adapter invalidates process-local bindings, resumes the same persisted Thread and starts its next Turn against the rebuilt model catalog. Profile Host also tracks persistent Threads that have an official id but no first rollout yet; Provider-triggered replacement fails closed until such a Thread starts its first Turn or the platform explicitly abandons it through the archive operation, instead of discarding the only Runtime state that can materialize it. The explicit abandon path does not fabricate a rollout or parse Runtime error text. The browser groups built-in, local and custom Providers from Runtime-supplied kinds, defaults the built-in and local groups closed, distinguishes the LM Studio and Ollama `gpt-oss` entries, and switches by clicking the Provider row. Editable model context windows use one save action that persists every changed model sequentially so catalog replacements cannot race. The current Provider and model use a shared high-contrast selected treatment in both themes. The platform stores the last Provider/model pair as a global default and copies it into every new Task; each existing Thread keeps its own database-backed pair. Scoped Runtime/TUI tests, two-Provider cache-isolation smoke, encrypted platform Secret injection/deletion smoke, live existing-Thread Provider transport rebinding and real same-Thread next-Turn context refresh pass; the unmaterialized-Thread replacement guard and explicit abandon path have focused unit and real app-server regression coverage |

## Web platform assessment

| Surface | Current state | Production gap |
| --- | --- | --- |
| Independent server | Axum server serves the browser, REST API, authenticated WebSocket, Profile Host and Runner from one deployable; the single-host deployer builds locked Release artifacts, securely provisions or verifies the fixed `open_web_codex` database, keeps verbose output in bounded logs, health-checks rollout and persists non-secret status metadata | HTTPS reverse proxy, OS supervision, rollback, backup/restore and remaining config hardening are still external GA gates |
| Persistence | PostgreSQL migrations cover users/sessions, organizations/memberships, Profiles/capabilities/encrypted Secrets, projects, tasks, Runs, leases, Workspaces, immutable Workspace Dataset and capability-package Releases, durable approvals/audit, Agent and Supervisor Definitions/draft Revisions/immutable Releases, exact Agent-to-package/Dataset and Supervisor-to-Agent Release dependencies, global immutable Supervisor instruction-policy Releases, direct Agent Run bindings, Policy snapshots/bindings, rebuildable Runtime Agent tree and child-task execution observations (`runtime_agent_projections`, `runtime_agent_execution_projections`), Task-owned typed Artifacts with Run/Thread/Turn/Item producer provenance, and versioned Run-event projections. Release metadata is governance state; generated Runtime Role configuration is not duplicated in PostgreSQL. Disposable fresh-schema tests cover publication resolution, dependency drift and cross-Organization denial | Release deprecation/deletion policy, concurrent edit conflict UX, Artifact replacement/invalidation/deletion/retention/cross-Run reuse and complete multi-user constraints remain missing |
| Authentication | current single-user startup creates an implicit local Owner and the browser obtains a local Session without credentials; sessions still bind an Organization and all resource authorization remains active; retained bootstrap/login use Argon2id | interactive login/registration is intentionally absent; public or multi-user deployment requires restoring authentication, HttpOnly-only sessions, CSRF, rate limiting and complete logout/revocation flows |
| Authorization | Project/Task/Run and runtime routes contain session-Organization checks; Provider/approval routes additionally check Profile ownership. The two-Organization denial suite passes against the current fresh schema, including Artifact ID guessing and Profile-bound operations | centralized policy abstraction, Project-specific roles and the full concurrent multi-user matrix remain missing |
| Codex bridge | Fake/Real adapter and event projection exist; Real uses the native Profile Registry/Host JSONL connection. Provider Secrets are encrypted and injected only into the owned child environment. Runtime-facing operations remain internal and browser routes are typed | composition is still one configured Profile process per server; per-user dynamic process routing remains incomplete |
| Task/Run | Source, contracts and tests implement CRUD/start/cancel/message/steer/compact/review, idempotent scheduling, DB leases/heartbeats/recovery, independent managed Workspace creation/grants/removal, authoritative Codex history, safe Item/Delta and approval projection, terminal execution, workspace files and Git operations. Runs only reference an authorized Workspace. A governed Run binds exactly one root mode: a direct immutable Agent Release, an immutable Supervisor Release, or a validated Supervisor Draft captured into an immutable Run snapshot. Supervisor Runs create a root Runtime projection in the same delivery transaction and authorize approvals from known child Threads through the Agent tree; direct Agent Runs apply the same package, Dataset, Workspace and capability preflight without manufacturing a one-Agent Supervisor | add existing-root registration, shared-Workspace concurrency and real multi-`cwd` validation. Approval expiry, protected-branch policy and full multi-Profile routing remain incomplete |
| Browser | Composer upload, Files-panel Workspace drag-and-drop upload and inline Intake UI are implemented; local typecheck passes; real journey unavailable | Composer accepts `.xlsx`, `.csv` and `.json`, including attachment-only messages. The Files panel accepts authorized regular Workspace files through the typed upload route and refreshes the tree after completion. Profile, whole-mapping, parameter, data-gap and final-checklist cards are rendered from typed Intake projections; old dedicated mapping/parameter routes are removed. Refresh/recovery and a real Web + Runtime journey remain release gates. |

## Immediate capability gates

1. Derive the remaining Manifest method sets and experimental state from
   generated Codex protocol/build facts.
2. Keep the digest-addressed contract bundle and Web feature policy in sync.
3. Do not count the Dataset publication component as available until **Add data** is reachable from
   the production `/web` Files surface and a real browser journey publishes, reopens and reuses a
   Release.
4. Thread creation may proceed with an authorized existing Workspace when
   Thread Readiness is available; Dataset, Binding and Analysis Readiness are
   checked only at analysis start.
5. Keep the real Excel/CSV/JSON Supervisor journey, restart/resume/read and
   duplicate-Turn checks as release gates; add multi-cwd, multi-agent failure
   and multi-user isolation coverage before promoting the experimental
   declarations.
6. Replace the single configured Profile composition root with authorized
   per-user Profile routing before multi-user Beta.
7. Complete independent Workspace validation: cover shared-Workspace
   concurrency, cross-user denial, resume, explicit lifecycle, existing-root
   registration, multi-`cwd` behavior and Git-operation containment. Start with
   a blank PostgreSQL database so the rewritten base migrations and denial
   suites become current evidence.
8. Complete the Task-owned Artifact lifecycle: replacement, invalidation,
   deletion, retention and authorized cross-Run reuse, while keeping
   Run/Thread/Turn/Item only as provenance.
