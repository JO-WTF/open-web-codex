# open-web-codex Agent Guide

All product and planning documentation must describe current live state. Do not
keep historical commentary in canonical documents.

## Project north star

Build a self-hosted, browser-first, multi-user Codex workbench that reuses the
official Codex runtime instead of reimplementing it. Each user must have an
isolated persistent Profile, including identity, `CODEX_HOME`, configuration,
Threads, memory, skills, plugins, MCP state and model/provider selection. Each
Run must have an authorized, isolated Git workspace. The browser reaches Codex
only through the authenticated Web platform and a versioned app-server bridge.

Codex remains the owner of Thread/Turn semantics, context compaction, memory,
multi-agent coordination, tools, skills, plugins and MCP. The Web platform owns
users, authorization, durable workflow state, Profile/Runner lifecycle, Git,
approvals, audit and browser projections. Preserve this boundary so `codex/`
can continue to synchronize with `openai/codex` using the smallest possible
product-specific seam.

## Repository scopes

- `apps/web/**`: follow `apps/web/AGENTS.md`. This area owns the browser product,
  platform persistence, authorization, Profile host, Runner, Git, and audit.
- `codex/**`: follow `codex/AGENTS.md`. This area owns the Codex runtime and
  app-server protocol. Preserve upstream conventions and keep custom changes
  small enough to rebase.
- `docs/**`, `scripts/**`, `.sync/**`: follow this root guide.

## Canonical documents

- Product: `docs/product-design.md`
- Architecture and ownership: `docs/architecture.md`
- Runtime capability truth: `docs/capability-baseline.md`
- Delivery status and order: `docs/development-plan.md`
- Official Codex synchronization: `docs/codex-upstream-sync.md`

Component documents may add implementation detail, but must not redefine product
scope, capability status, or milestone state.

Before planning or implementing work, read the relevant canonical documents.
Use `docs/capability-baseline.md` for what the checked-in runtime demonstrably
supports and `docs/development-plan.md` for what is complete or next; do not
infer delivery status from the target product design.

## Contract rules

1. Codex-generated JSON Schema and TypeScript types are the protocol truth.
2. Capability Manifest values must be generated from the Codex build, not copied
   by hand into the Web application.
3. Web feature policy maps product features to capability IDs and minimum
   versions; it does not claim that a server supports them.
4. Runtime capabilities remain disabled until generated contracts, offline
   fixtures, and a real app-server smoke test agree.
5. Never expose raw app-server request IDs, local paths, credentials, or the raw
   protocol as a public browser API.

## Upstream rules

- `codex/` tracks `https://github.com/openai/codex`, branch `main`, through Git
  subtree synchronization.
- Run `scripts/codex-upstream-status.sh` before modifying a high-churn upstream
  file.
- Use `scripts/sync-codex-upstream.sh --apply` for official updates. It creates a
  `codex/sync-upstream-*` branch; never sync directly on `main`.
- Resolve conflicts by preserving upstream structure first and reapplying the
  smallest product-specific seam.
- Regenerate app-server and config schemas after protocol/config changes.

## Validation

- Root docs/scripts: run `bash -n scripts/*.sh` and the upstream status command.
- `apps/web`: run `npm run typecheck` and relevant tests; run contract tests for
  integration changes.
- `codex`: follow `codex/AGENTS.md`, including `just fmt` and scoped `just test`.
- Cross-project protocol changes require both component checks and the real
  app-server smoke harness.

## Cursor Cloud specific instructions

The dependency-refresh update script runs `npm --prefix apps/web ci` and
`cargo fetch --manifest-path apps/web/Cargo.toml` on startup. Everything below
is startup/run guidance that is deliberately kept out of that script.

### Toolchain notes

- `apps/web` needs Rust >= 1.85 (its dependency tree uses `edition2024`). The VM
  default toolchain is stable (currently 1.97); the repo pin at
  `codex/codex-rs/rust-toolchain.toml` (1.95) only applies inside `codex/` and
  is auto-fetched by rustup. System build deps `libssl-dev` and `pkg-config`
  are required for the Rust build and are preinstalled in the snapshot.
- Node 22 / npm and pnpm 10.33 are preinstalled.

### PostgreSQL (required by the platform server at runtime)

- The Axum server (`apps/web/server`) connects to PostgreSQL on startup and runs
  `apps/web/migrations` when `--migrate` is set (the default). It builds fine
  without a DB (no compile-time-checked queries), but will not start without one.
- Postgres 16 is installed with a `open_web_codex` database owned by role
  `ubuntu`, and loopback TCP auth is set to `trust`, so the server's default
  connection string (`postgres://ubuntu@localhost:5432/open_web_codex`) works
  with no password. Override with `DATABASE_URL` if needed.
- The cluster is NOT auto-started on boot. Start it each session with:
  `sudo pg_ctlcluster 16 main start` (check with `sudo pg_lsclusters`).

### Running the stack

- `make mvp` (wraps `scripts/dev-mvp.sh`) builds and starts the platform server
  (:4800) and the Vite web client (`http://127.0.0.1:1420/web`). See
  `docs/mvp-runbook.md` and `apps/web/README.md` for the browser flow.
- `CODEX_MODE=fake make mvp` runs an in-memory Codex adapter with a demo
  workspace and simulated thread/turn events — no Codex runtime, daemon, or
  model credentials needed. This is the lightest way to exercise the full
  browser -> server -> adapter -> events flow.
- `CODEX_MODE=real` (the default) additionally requires building the Codex
  binary (`cd codex/codex-rs && cargo build -p codex-cli --bin codex`) and the
  loopback daemon (`cd apps/web/src-tauri && cargo build --no-default-features
  --bin codex_monitor_daemon`), and real Codex credentials to actually execute
  turns. `dev-mvp.sh` builds these automatically in real mode.

### Frontend gateway-URL gotcha

- The web client's default backend URL is `http://127.0.0.1:4733` (the loopback
  daemon), read from `localStorage` key `codexMonitorWebBaseUrl`; the
  `VITE_CODEX_MONITOR_WEB_API` env var set by `dev-mvp.sh` is only a fallback and
  is ignored once that key exists. In fake mode the backend is the platform
  server on :4800, so set the "Gateway URL" to `http://127.0.0.1:4800` via the
  in-app Settings modal (bottom-left gear -> "Save & Check"); it persists to
  `localStorage`.

### Known pre-existing check failure

- `npm run typecheck` (and therefore `npm run build`, which runs `tsc` first)
  fails on a type error in `apps/web/src/services/webClient.test.ts`. This is
  independent of environment setup. `npm run lint`, `npm test` (Vitest, 1099
  tests), and `npx vite build` all pass, and `npm run dev` runs fine, so use the
  dev server rather than the production `build` command for local runs.
