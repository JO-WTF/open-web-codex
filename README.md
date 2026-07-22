# open-web-codex

`open-web-codex` is a self-hosted, browser-first Codex workbench. It combines the
Self-hosted browser workbench and platform host with a narrowly customized
Codex runtime in one repository.

The product keeps the responsibilities deliberately separate:

- `apps/web` owns users, projects, tasks, runs, permissions, worktrees, approvals,
  audit data, and the browser experience.
- `codex` owns model execution, Thread/Turn semantics, multi-agent behavior,
  memory, skills, plugins, MCP, and the app-server protocol.
- generated protocol artifacts are the integration boundary. The Web application
  must not reimplement Codex runtime behavior.

## Repository layout

```text
apps/web/                 Browser client and authenticated platform server
codex/                    Customized Codex runtime subtree
docs/product-design.md    Canonical product requirements and release scope
docs/capability-baseline.md
docs/development-plan.md  Executable milestone plan
docs/codex-upstream-sync.md
scripts/                  Monorepo and upstream-sync tooling
```

## Get started

Run the restored standalone WebApp with the deterministic test Runtime:

```bash
./scripts/start-all.sh --fake
```

Then open `http://127.0.0.1:1421/web`. The WebApp calls the authenticated
platform Server on port `4800` directly through typed REST resources and
`/api/events/ws`; there is no separate Gateway process. Use
`./scripts/start-all.sh` for the repository Codex Runtime. See
[the MVP runbook](docs/mvp-runbook.md) for the browser flow, binary override and
known limitations.

Web application:

```bash
cd apps/web
npm ci
npm run typecheck
npm test
```

Codex runtime:

```bash
cd codex/codex-rs
just fmt --check
just test -p codex-app-server-protocol
```

Inspect the official Codex upstream status:

```bash
./scripts/codex-upstream-status.sh
```

Create a dedicated sync branch and merge the latest `openai/codex` main branch
into the `codex/` subtree:

```bash
./scripts/sync-codex-upstream.sh --apply
```

Read [the upstream sync runbook](docs/codex-upstream-sync.md) before resolving a
non-trivial sync conflict.

## Canonical documents

- [Product design](docs/product-design.md)
- [Capability baseline](docs/capability-baseline.md)
- [Development plan](docs/development-plan.md)
- [Architecture](docs/architecture.md)

The original component licenses remain in `apps/web/LICENSE` and `codex/LICENSE`.
See [LICENSES.md](LICENSES.md) for the repository licensing map.
