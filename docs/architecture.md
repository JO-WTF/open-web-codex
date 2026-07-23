# Architecture

## Architectural objective

`open-web-codex` is a multi-user Web control plane around the official Codex
runtime. It is not a browser reimplementation of Codex. The design maximizes
reuse of `codex/`, keeps product-specific Runtime changes behind narrow
app-server contracts, and preserves regular subtree synchronization with
`openai/codex/main`.

The initial deployable is a modular monolith with PostgreSQL and colocated
Profile Host/Runner processes. Boundaries are interfaces and ownership rules,
not a requirement to create network microservices. Components separate only
when measured capacity or isolation needs justify it.

## System shape

```text
Browser
  -> authenticated platform HTTP + WebSocket API
  -> authorization / Task / Run / Approval services
  -> Profile Host ---------------------> persistent per-user Profile Home
  -> Codex app-server                   -> Thread / Turn / memory / agents
  -> event normalizer and durable projection

Run orchestrator
  -> repository mirror (read-only to Agent)
  -> per-Run worktree (authorized writable workspace)
  -> Runner sandbox / Git delivery

Codex build
  -> generated protocol Schema + TypeScript
  -> generated Capability Manifest + fixtures + digest
  -> Web feature policy and compatibility gate
```

The checked-in repository has one browser bridge. `apps/web/src/platform` calls
typed platform resources under `/api`; live updates use an authenticated
WebSocket whose first frame carries the session token. `apps/web/server`,
`apps/web/crates` and `apps/web/migrations` own the server boundary. There is no
local sidecar, raw browser JSON-RPC route, query-token event stream, or trusted
browser-supplied filesystem path.

The established React component tree and styles remain the browser product.
Desktop imports are replaced at their original call sites by browser adapters,
so UI components do not own platform authorization or transport details. Native
window, tray, updater, daemon and desktop file-manager actions either map to a
safe browser capability or return an explicit deployment-managed/unavailable
result; they never cause a Tauri runtime to reappear.

## Facts and ownership

| Fact | Authoritative owner | Web may persist |
| --- | --- | --- |
| User, organization, membership and session | Web platform database | complete platform record |
| Project, Task, Run, Thread model selection, lease, approval and audit | Web platform database | complete platform record |
| Profile ownership and process health | Web database + Profile Host | mapping, health, build and capability snapshot |
| Thread, Turn, items, compaction and model-visible context | Codex Profile/app-server | opaque IDs, event projection and search index |
| Provider config and runtime model catalog | Codex Profile/app-server | secret references, global default Provider/model selection, policy and display cache scoped to Profile |
| Agent scheduling and parent/child execution | Codex runtime | observable trajectory and status projection |
| Skills, plugins, MCP and memory state | Codex Profile/app-server | permissions, audit and capability-gated projection |
| Repository objects and worktree contents | Git/Runner | metadata, status, diff summary and artifact references |

The platform must recover model-visible history from Codex. Event projections
are rebuildable UI/read models and never become a second Thread store, memory
engine or agent scheduler.

## Web / app-server / Codex server boundary contract

All feature work must start by selecting the owning layer below. A change that
cannot be placed in exactly one owner must be split until each part has a clear
owner and contract.

| Layer | Owns | Must not own |
| --- | --- | --- |
| WebApp / browser | Presentation state, input controls, optimistic UI, safe rendering of platform DTOs, accessibility, and browser-only fallbacks | MCP/Skills/Plugins discovery, tool catalogs, model-visible prompt injection, Thread/Turn semantics, filesystem authority, credentials, raw app-server JSON-RPC, or local Profile paths |
| Platform app-server | Authentication, authorization, Profile/Runner lifecycle, Task/Run/Approval/Git persistence, Secret injection, audit, durable event projection, typed browser DTOs and capability gating | Model reasoning, context compaction, memory, tool execution policy, MCP/Skills/Plugins lifecycle, Provider transport internals, or untyped protocol passthrough to the browser |
| Profile Host / adapter | Narrow, typed bridge from platform resources to Codex app-server requests and notifications; process-instance isolation; request-id mapping; safe event normalization | Product UI behavior, broad protocol rewriting, model/tool discovery emulation, or persistent state that belongs to Codex Profile or platform tables |
| Codex app-server / Runtime | Thread/Turn lifecycle, model context, tools, MCP, Skills, Plugins, memory, multi-agent coordination, Provider model/transport behavior and generated protocol facts | Web sessions, organizations, browser DTOs, platform authorization, Git workspace provisioning, deployment scripts, or Profile ownership policy |
| Plugin / Skill / MCP package | Model-visible capability instructions and tool/server declarations consumed by Codex discovery | Web-side command interception, platform config mutation, or hidden Profile `config.toml` edits |

Planning and code review must reject these anti-patterns:

1. A browser command or composer shortcut that answers a Runtime capability
   question without sending the user's intent through Codex.
2. A server route or startup script that injects MCP/Skill/Plugin configuration
   directly into a Profile as a substitute for Codex discovery or a typed
   platform lifecycle API.
3. A Web/server prompt injection workaround for provider-neutral capabilities
   when the capability can be expressed by Runtime tools, Skills, Plugins, MCP
   or a generated app-server contract.
4. Browser exposure of raw request ids, raw JSON-RPC, local paths, credentials,
   unbounded protocol payloads or unvetted tool catalogs.
5. Product-specific changes spread through high-churn Codex files when the same
   behavior can live in `apps/web`, a plugin, a skill, an MCP server or a narrow
   generated protocol seam.

Every feature proposal must include a short boundary note naming the owner,
inputs, outputs, capability gate and tests. If the owner is Codex, follow the
upstream customization workflow before editing. If the owner is Web/platform,
prove that the implementation consumes typed contracts rather than recreating
Runtime behavior.

## Multi-user isolation model

The authorization chain is:

```text
session -> user -> organization membership -> project permission
        -> task/run -> profile/thread -> workspace/event/approval/artifact
```

- One member has one persistent personal Profile by default.
- A Profile has a dedicated `CODEX_HOME`, credentials, Provider configuration,
  Threads, memory, skills, plugins and MCP configuration.
- One Profile has at most one primary app-server process. Cross-process locking
  and a process registry enforce the invariant.
- A Profile may execute multiple authorized Tasks only within measured Runtime
  concurrency limits. It never shares a Home with another user.
- Every Run uses a distinct writable Git worktree. Repository mirrors are not
  Agent-writable, and a successor Run may continue a Thread without reusing an
  unrelated writable directory.
- Profile Host validates Profile/User/Thread/Workspace relationships. Runner
  validates Project/Run/Workspace relationships. Normal browser users never
  submit trusted filesystem paths.
- Cache, subscription, model and secret keys include their user/Profile scope.
  Cross-user and guessed-ID denial tests are release gates.

### Single-Profile convergence mode

The current near-term runtime target is a deliberately narrowed deployment mode:
one authenticated human user, one persistent Profile Home, one primary Profile
Host process and one selected workspace context per active Run. This is a
deployment constraint, not a boundary exception. The same ownership table above
continues to apply:

- The platform starts and monitors the single Profile Host, injects only
  authorized environment and secret references, and records safe diagnostics.
- The selected `CODEX_HOME`, Profile identity, workspace root, source root and
  capability roots are fixed at startup or by typed platform lifecycle state;
  browser input never changes server-local paths.
- To unblock the single Profile smoke, the platform may copy a file-backed
  `auth.json` from an already logged-in local Codex home into an empty Profile
  home before starting the Profile Host. This is a transitional single-user
  import path only; it must not become the multi-user credential model.
- Skills, Plugins and MCP are still discovered and executed by Codex Runtime.
  The WebApp does not scan `.mcp.json`, run plugin launchers, answer MCP
  inventory questions locally or write hidden Profile configuration.
- Local capability packages such as `tools/maps-mcp` are made available as
  selected capability roots. Their launchers own package bootstrap, dependency
  checks and MCP server startup; Profile Host only reports safe startup status
  and categorized failures.
- This mode must pass single Profile smoke tests for Provider login/model
  discovery, Runtime MCP discovery, MCP startup, third-party Provider tool calls,
  map-card rendering and Thread resume before multi-Profile routing work
  resumes.

## Runtime bridge

Codex produces a build-specific contract bundle containing:

1. JSON Schema and TypeScript definitions generated by app-server protocol.
2. A Capability Manifest derived from the build's method registry,
   experimental annotations, limits and build identity.
3. Protocol fixtures and stable structured error metadata.
4. Codex commit, target, binary digest and compatibility notes.

The Web build consumes the bundle by digest. A separate Web feature policy maps
product features to capability IDs and minimum versions; it cannot claim a
server supports a feature. A capability is enabled only when generated
contracts, offline fixtures and a real app-server smoke test agree.

Browser DTOs are stable platform resources, not passthrough JSON-RPC. Raw
app-server request IDs, Profile paths, local paths, credentials and unknown
protocol payloads remain inside the Host/adapter boundary. Unknown Runtime
events may be retained for diagnostics but cannot be exposed as an unsafe public
API or crash the event stream.


### Rich reply cards and map visualization

Structured reply cards are browser projections of Codex message content and
platform artifacts. Codex remains responsible for deciding when to use tools and
what to say. Skills, Plugins and MCP servers may provide model-visible
instructions or tools that emit a compact, stable marker such as
`open-web-card map.v1`; the Web platform may parse and render that marker, but
it must not make the model "discover" a capability by intercepting composer
text or injecting ad-hoc prompts.

Current map-card support follows this checked-in preview flow:

1. Runtime/Skills/MCP may emit a small `open-web-card map.v1` marker or legacy
   map widget marker in assistant text.
2. The browser parser recognizes the marker, hides the raw fenced block and
   renders bounded inline point, line, polygon or GeoJSON data with Mapbox GL.
   The browser reads the restricted public `pk.` token through the typed
   authenticated `/api/configuration/maps` resource. Without a token the map
   card remains visible and opens an in-card configuration dialog; authorized
   owners/admins save through the same resource and all visible cards update.
   The shared dialog selects the one active Mapbox or Google Maps provider for
   server-side `map_utils` tools; saving replaces the prior provider and key.
   `VITE_MAPBOX_ACCESS_TOKEN` remains a build-time fallback.
3. The selected provider/key pair is one encrypted global entry in
   `platform_configuration_secrets`; the next save atomically replaces its
   value. The browser receives provider/configured status and, only while
   Mapbox is active, the restricted public `pk.` token required by Mapbox GL.
   The Server delivers the selected provider and key directly to a strictly
   validated local MCP elicitation URL without opening that one-time page.
   The global scope is temporary and reserves a later per-user move.
4. The platform may later add an Artifact-backed DTO and generated card schema;
   until that exists, large GeoJSON, production token distribution, renderer
   capability policy, permissioned downloads and server-side card storage
   remain disabled gates.
5. Oversized or invalid data produces safe fallback rendering; raw local paths,
   credentials, app-server request IDs and unbounded protocol payloads never
   reach the browser.

This feature must not broaden the Codex subtree or the Web platform into a tool
runtime. Runtime changes are limited to a narrow generated protocol seam only if
the official app-server cannot carry card markers or artifact references through
existing message/event surfaces.

## Primary runtime flows

### Create and run a Task

1. Platform authenticates the session and authorizes project/task creation.
2. A transaction creates the Task and queued Run using an idempotency key.
3. Scheduler leases the Run; Runner prepares mirror and isolated worktree.
4. Profile Host locks/starts the user's Profile, verifies contract compatibility
   and starts or resumes the mapped Codex Thread in the authorized workspace.
5. Runtime events are normalized, assigned a per-Task monotonic sequence and
   persisted before browser fan-out.
6. Terminal state is reconciled across database, Codex Profile and Git. No Run
   remains `running` without a valid lease/heartbeat and recoverable owner.

### Approval or structured input

1. Each app-server process receives a fresh Runtime instance UUID. Profile Host
   receives a Codex Server Request and persists an internal mapping to
   Profile/Task/Run/Thread plus that instance before notification.
2. Platform filters recipients by resource permission and approval policy.
3. The first valid decision wins through compare-and-swap semantics.
4. Host responds only when both the process instance and request id still match.
   Active Turns and unresolved Server Requests block credential-triggered
   restart; after an actual restart, old-instance requests become cancelled and
   a reused numeric request id cannot receive the stale response.
5. An uncertain transport delivery remains retryable only with the same stored
   decision; expiry or Run termination produces an explicit terminal state.

### Commit and push

Runner revalidates workspace ownership and Git status immediately before the
operation. Commit and Push are explicit user actions with audit records. Force
Push, implicit Merge and automatic remote branch deletion are outside the
product contract.

## Upstream synchronization boundary

`codex/` is a Git subtree tracking official `openai/codex/main`. Before touching
high-churn Runtime files, run `scripts/codex-upstream-status.sh`. Official
updates use `scripts/sync-codex-upstream.sh --apply` on a dedicated
`codex/sync-upstream-*` branch.

Prefer, in order:

1. consume an existing upstream app-server method;
2. add generated protocol/manifest metadata around upstream structure;
3. add the smallest isolated Runtime seam with scoped tests;
4. implement platform policy outside `codex/`.

Never fork Thread history, compaction, memory, multi-agent scheduling, Skills,
Plugins or MCP into the Web platform for short-term convenience.

## Current implementation boundary

The live capability and delivery status are intentionally not duplicated here.
Use `docs/capability-baseline.md` for verified Runtime/platform facts and
`docs/development-plan.md` for completed and next work. ADRs under `docs/adr/`
record accepted implementation choices without redefining these ownership
rules.
