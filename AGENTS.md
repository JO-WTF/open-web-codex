# open-web-codex Agent Guide

All product and planning documentation must describe current live state. Do not
keep historical commentary in canonical documents.

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
