# open-web-codex Web platform

This directory contains the browser client and the session-backed platform that
hosts the official Codex runtime. It has no desktop application, local daemon,
or browser-to-Codex protocol bridge.

## Runtime shape

```text
Browser
  -> versioned REST resources + authenticated WebSocket
  -> open-web-codex-server
       -> PostgreSQL authorization and durable workflow state
       -> Profile Host -> codex app-server
       -> Workspace service -> authorized roots + managed Git resources
       -> Run orchestrator -> validate selected Workspace + execute/audit
```

The browser never receives local paths, credentials, app-server request IDs, or
raw JSON-RPC. Codex remains the owner of Thread/Turn, context, memory, tools,
skills, plugins, MCP, and multi-agent execution.

Workspaces are independent authorized execution roots. A Thread keeps its
current `cwd` in Codex; a Run references the Workspace selected for that
attempt. Neither object implicitly creates or owns a checkout.

## Requirements

- Node.js 20 or newer and npm
- Stable Rust toolchain
- PostgreSQL
- Git
- A Codex binary for real Runtime mode

## Local development

From the repository root, start the same-origin WebApp and Server together:

```bash
./scripts/run-local.sh --fake --background
```

Open `http://127.0.0.1:4800/web`. The current single-user flow creates an
implicit local Owner and Session, skips login and registration screens, and
opens the WebApp directly. Server-side Session, Organization, Profile and
resource authorization remain in use. No daemon or Gateway process is started.
Real Runtime mode is the default when `--fake` is omitted; see
[`../../docs/mvp-runbook.md`](../../docs/mvp-runbook.md) for configuration.

For frontend hot reload, keep the platform Server running and start Vite:

```bash
./scripts/run-local.sh --fake --background

# another terminal
cd apps/web
npm ci
npm run dev
```

Vite uses port 1420 by default and proxies `/api` and WebSocket upgrades to
`127.0.0.1:4800`.

## Single-host release deployment

From the repository root:

```bash
./scripts/deploy.sh
```

This validates deployment policy and delegates the optimized build and
health-checked service replacement to `run-local.sh`, the single build and
service-lifecycle owner. The Platform Server and Runtime components use exact
Cargo dependency fingerprints. The platform Server serves the built WebApp
directly at `http://127.0.0.1:4800/web`; Vite is not part of the deployed
topology. Deployment output is kept in
`.local/open-web-codex/logs/deploy.log`, and build or startup details are kept
in `.local/open-web-codex/logs/run-local.log`.

Without `DATABASE_URL` or a saved credential file, an interactive deployment
offers to connect to an existing PostgreSQL server or create an application
role and database. The database name cannot be changed from `open_web_codex`.
Passwords are not echoed or included in process arguments; the generated URL
is kept at `.local/open-web-codex/database-url` with mode `600`. A
non-interactive deployment fails before compilation when database configuration
is absent.

```bash
./scripts/deploy.sh --check
./scripts/deploy.sh --status
./scripts/deploy.sh --stop
```

The deployer reuses the same persistent Profile, Secret Store, Runner, PID and
server-log directories as `run-local.sh`. Repository build workflows use a
bounded sccache when available and separate `dev-small`, `ci-test` and release
profiles. Cargo target outputs are retained for incremental builds; repository
scripts do not apply an automatic storage watermark or profile cleanup. A
public deployment still requires an HTTPS reverse proxy, external Secret Store
key management, PostgreSQL backup/restore and an OS-level service supervisor.

## Validation

Run these commands from the repository root:

```bash
(cd apps/web && npm run check:no-desktop)
(cd apps/web && npm run lint)
(cd apps/web && npm run typecheck)
(cd apps/web && npm run test)
(cd apps/web && npm run build)
(cd apps/web && cargo fmt --all --check)
./scripts/test-web-rust.sh
(cd apps/web && npm run test:codex-harness)
```

PostgreSQL integration tests use `TEST_DATABASE_URL` and are ignored by default.
The real app-server smoke additionally requires a built Codex binary.

With an isolated real Server already running and the Codex/MCP test binaries
built, run the reproducible third-party Provider journey with a key file:

```bash
E2E_BASE_URL=http://127.0.0.1:4810 \
DEEPSEEK_API_KEY_FILE=/absolute/path/to/deepseek-key \
npm run test:e2e:real-platform
```

The harness creates its own managed Project and two Threads, then checks live
event timing, code execution, file preview, Provider add/switch/context updates,
real stdio MCP invocation, approval resolution, delayed Turn state, history
restoration and durable/live event ordering. It never prints the Provider key.

The real DeepSeek tool-capability gate is a separate opt-in check. It uses the
real `warehouse-network-copilot` Task entry and fixture upload, and configures
the exact model capability through the typed Provider API. A missing capability
or a missing structured call is a typed terminal result; the gate never retries
with a prompt variant or fabricates a Tool call. The forwarding probe records
only tool counts/names, `tool_choice`, structured call presence and canonical
terminal summaries. On a full-gate failure it also emits a bounded timeline of
thread/Turn, collaboration, MCP and map-producer state without keys, full
prompts, schemas or arguments. If a Provider returns a Tool name absent from
that request's visible tool set, the gate reports typed
`provider_tool_call_not_visible` instead of treating it as a Runtime success.
It reuses a registered
credential environment reference through a temporary Provider and removes that
Provider, Run, Workspace and Project in its terminal cleanup:

```bash
E2E_REAL_DEEPSEEK=1 \
E2E_BASE_URL=http://127.0.0.1:4810 \
E2E_REAL_DEEPSEEK_SOURCE_PROVIDER_ID=deepseek \
npm run test:e2e:real-deepseek
```

This gate requires the source Provider to already have a Platform-managed
credential. It does not print or persist the credential value.

The gate refreshes the temporary Provider's model catalog through the official
Provider API, then persists `supportsSearchTool=true` for the exact
`deepseek-v4-flash` model through the typed model metadata endpoint. It does
not write `model_catalog_json`, infer capabilities from the model name, or
retry with prompt variants. It first verifies the minimal
`tool_search → spawn_agent` path, then runs the full
`spawn_agent → Data/Network MCP → 12h/map` Task. A missing structured call or
missing canonical business Tool is returned as a typed failure. The Provider
capability, Run, Workspace and Project are restored or removed in terminal
cleanup.

## Layout

```text
src/WebApp.tsx             established browser WebApp UI
src/services/webClient.ts  narrow WebApp-to-Server compatibility seam
browser/client.ts          authenticated typed REST/WebSocket transport
browser/browser-entry.ts   implicit local Session and WebApp entry
server/                    Axum composition root and session-backed routes
crates/auth/               retained sessions, password hashing, and RBAC
crates/profile-host/       persistent CODEX_HOME and app-server lifecycle
crates/profile-registry/   single-owner Profile process registry
crates/provider-service/   authorized Provider orchestration
crates/secret-store/       encrypted credential persistence
crates/git-runtime/        mirrors and independent managed Workspaces
crates/run-orchestrator/   leases, recovery, cancellation, and execution
crates/approval-service/   durable app-server approvals
crates/platform-store/     PostgreSQL state and event bus
crates/platform-contracts/ browser-safe DTOs
migrations/                platform schema
scripts/                   contracts, smoke tests, and boundary checks
```

Canonical product, architecture, security, capability, roadmap and delivery
documents are routed by the repository-level
[`docs/README.md`](../../docs/README.md).
