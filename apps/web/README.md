# open-web-codex Web platform

This directory contains the browser client and the authenticated platform that
hosts the official Codex runtime. It has no desktop application, local daemon,
or browser-to-Codex protocol bridge.

## Runtime shape

```text
Browser
  -> versioned REST resources + authenticated WebSocket
  -> open-web-codex-server
       -> PostgreSQL authorization and durable workflow state
       -> Profile Host -> codex app-server
       -> Run orchestrator -> private Git mirror + per-Run workspace
```

The browser never receives local paths, credentials, app-server request IDs, or
raw JSON-RPC. Codex remains the owner of Thread/Turn, context, memory, tools,
skills, plugins, MCP, and multi-agent execution.

## Requirements

- Node.js 20 or newer and npm
- Stable Rust toolchain
- PostgreSQL
- Git
- A Codex binary for real Runtime mode

## Local development

From the repository root, start the 1421 WebApp and the 4800 Server together:

```bash
./scripts/start-all.sh --fake
```

Open `http://127.0.0.1:1421/web`. The WebApp calls the authenticated Server
directly; no daemon or Gateway process is started. Real Runtime mode is the
default when `--fake` is omitted; see
[`../../docs/mvp-runbook.md`](../../docs/mvp-runbook.md) for configuration.

To run the two development processes separately:

```bash
npm ci
npm run dev

# another terminal
cargo run -p open-web-codex-server -- --codex-mode fake
```

Vite proxies `/api` and WebSocket upgrades to `127.0.0.1:4800`.

## Validation

```bash
npm run check:no-desktop
npm run check:main-ui-parity
npm run lint
npm run typecheck
npm run test
npm run build
cargo fmt --all --check
cargo test --workspace --locked
npm run check:codex-contracts
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

## Layout

```text
src/WebApp.tsx             restored 1421 WebApp UI
src/services/webClient.ts  narrow WebApp-to-Server compatibility seam
browser/client.ts          authenticated typed REST/WebSocket transport
browser/browser-entry.ts   authentication and WebApp entry
server/                    Axum composition root and authenticated routes
crates/auth/               sessions, password hashing, and RBAC
crates/profile-host/       persistent CODEX_HOME and app-server lifecycle
crates/profile-registry/   single-owner Profile process registry
crates/provider-service/   authorized Provider orchestration
crates/secret-store/       encrypted credential persistence
crates/git-runtime/        mirrors and isolated Run workspaces
crates/run-orchestrator/   leases, recovery, cancellation, and execution
crates/approval-service/   durable app-server approvals
crates/platform-store/     PostgreSQL state and event bus
crates/platform-contracts/ browser-safe DTOs
migrations/                platform schema
scripts/                   contracts, smoke tests, and boundary checks
```

Canonical product, architecture, capability, and delivery status live under
the repository-level `docs/` directory.
