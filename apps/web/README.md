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

From the repository root:

```bash
./scripts/run-local.sh --fake
```

The script builds the browser and server, applies PostgreSQL migrations, and
serves everything at `http://127.0.0.1:4800/`. Real Runtime mode is the default;
see [`../../docs/mvp-runbook.md`](../../docs/mvp-runbook.md) for configuration.

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

## Layout

```text
src/platform/              browser application and typed platform client
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
