# open-web-codex Agent Guide

All product and planning documentation must describe current live state. Do not
keep historical commentary in canonical documents.

## Project north star

Build a self-hosted, browser-first, multi-user Codex workbench that reuses the
official Codex runtime instead of reimplementing it. Each user must have an
isolated persistent Profile, including identity, `CODEX_HOME`, configuration,
Threads, memory, skills, plugins, MCP state and model/provider selection. Each
Thread/Chat must execute in an authorized Workspace that remains associated with
it across Turns and platform Runs. A Run is a scheduling and audit attempt: it
references the Thread Workspace and does not own or recreate a checkout. The
browser reaches Codex only through the authenticated Web platform and a
versioned app-server bridge.

Codex remains the owner of Thread/Turn semantics, context compaction, memory,
multi-agent coordination, tools, skills, plugins and MCP. The Web platform owns
users, authorization, durable workflow state, Profile/Runner lifecycle, Git,
approvals, audit and browser projections. Preserve this boundary so `codex/`
can continue to synchronize with `openai/codex` using the smallest possible
product-specific seam.

Follow official Codex workspace semantics unless an explicit, tested platform
security requirement demands a narrower policy. Managed worktrees are normally
Thread/Chat-scoped; a local or permanent Workspace may host multiple Threads
only through explicit platform selection and authorization. Do not reintroduce
per-Run Workspace provisioning or ownership.

## Codex customization convergence

The target is not a zero-diff Codex subtree. The target is a small, explicit,
replayable set of third-party Provider and TUI extensions that can be carried
through each official Codex subtree update. Treat all other product behavior as
Web-platform work unless the official runtime has no suitable boundary.

### Retained Runtime and TUI core

These are intentional, product-critical `codex/` changes. Do not remove,
replace with Web-only behavior, or broaden without preserving their tests and
the corresponding app-server contract:

- Third-party Chat Completions transport: request translation, streaming/SSE
  translation, tool-call translation, and the narrow Core transport dispatch.
- Provider metadata and configuration: `WireApi::Chat`, provider-scoped model
  metadata, provider identity, and selection semantics.
- Provider model discovery and caching: scoped catalog retrieval, refresh,
  normalization, and isolation between Providers.
- Versioned app-server Provider API: Provider listing, provider-scoped model
  listing, controlled configuration writes, model refresh, and selected
  Provider/model propagation.
- TUI Provider workflows: Provider selection, model selection, onboarding,
  configuration updates, refresh, and error states. TUI parity is a core
  requirement, not an optional Web migration artifact.

Keep this code concentrated in Provider-specific modules. In particular, place
Chat translation in `codex-api`, Provider facts in Provider crates, app-server
wire types in `app-server-protocol`, request handling in the app-server, and
TUI presentation in dedicated TUI Provider modules. `core` may contain only
the minimal transport-selection seam; it must not acquire Web, Profile,
authorization, or browser-state logic.

### Product boundaries and legacy code

- `apps/web` owns Profile lifecycle, credentials injection, authorization,
  Provider CRUD orchestration, browser DTOs, and all Web UI state. It may call
  typed app-server methods internally through the Host, but must not expose raw
  JSON-RPC or configuration key paths to the browser.
- Do not add Tauri, daemon proxy, Web route, platform persistence, or browser
  adaptation code under `codex/`. Prefer platform DTOs and the Profile Host.
- Do not hand-edit generated app-server Schema or TypeScript files; regenerate
  them from Rust protocol types.
- Do not preserve a local workaround when upstream provides equivalent behavior.
  Record an upstreamed replacement in the patch map and return to upstream code.
- `legacy_response_tool_history` is a compatibility seam for existing Profile
  rollout history, not a new feature surface. Keep it isolated, test reload
  behavior, and remove it once supported legacy histories are retired.
- The Capability Manifest is a platform compatibility seam. Its method facts
  must converge on generated protocol/build data rather than hand-maintained
  lists.

### Required workflow for Codex changes

1. Before changing a high-churn upstream file, run
   `scripts/codex-upstream-status.sh` and inspect
   `docs/custom-codex-patch-map.md`.
2. Classify every non-generated Codex difference as `retain-core`,
   `upstreamed`, `move-out`, or `drop`. Do not add an unclassified difference.
3. Make the smallest change at the owning layer; avoid scattering Provider
   behavior through `core`, generic TUI orchestration, or Web code.
4. For protocol changes, regenerate Schema and TypeScript, update offline
   fixtures, and validate a real app-server smoke.
5. For TUI changes, add or update snapshot coverage. For Provider changes,
   cover Provider switching, cache isolation, refresh, credentials failure,
   Chat tool calls, and interrupted-stream recovery.
6. During an official sync, accept upstream structure first, then reapply only
   the documented retained seams in this order: Provider metadata/Chat
   transport, model catalog/cache, app-server Provider API, TUI Provider
   workflows, generated artifacts, and Web contract smoke.

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
- Custom Codex seams: `docs/custom-codex-patch-map.md`

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
- Run `scripts/codex-customization-status.sh` before classifying, moving, or
  deleting a Codex difference. It compares `HEAD:codex` directly with
  `codex-upstream/main`; never use this repository's `main` branch as the
  convergence baseline.
- Use `scripts/sync-codex-upstream.sh --apply` for official updates. It creates a
  `codex/sync-upstream-*` branch; never sync directly on `main`.
- Resolve conflicts by preserving upstream structure first and reapplying the
  smallest product-specific seam.
- Regenerate app-server and config schemas after protocol/config changes.

## Validation

- Root docs/scripts: run `bash -n scripts/*.sh`,
  `scripts/codex-upstream-status.sh`, and
  `scripts/codex-customization-status.sh` when the change affects Codex
  convergence state.
- `apps/web`: run `npm run typecheck` and relevant tests; run contract tests for
  integration changes.
- `codex`: follow `codex/AGENTS.md`, including `just fmt` and scoped `just test`.
- Cross-project protocol changes require both component checks,
  `npm run check:codex-contracts`, and the real
  `npm run smoke:codex-app-server -- --require-manifest` harness.
