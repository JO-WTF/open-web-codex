# open-web-codex

`open-web-codex` is a self-hosted, browser-first Codex workbench. It combines the
CodexMonitor Web/host code with a customized Codex runtime in one repository.

The product keeps the responsibilities deliberately separate:

- `apps/web` owns users, projects, tasks, runs, permissions, worktrees, approvals,
  audit data, and the browser experience.
- `codex` owns model execution, Thread/Turn semantics, multi-agent behavior,
  memory, skills, plugins, MCP, and the app-server protocol.
- generated protocol artifacts are the integration boundary. The Web application
  must not reimplement Codex runtime behavior.

## Repository layout

```text
apps/web/                 CodexMonitor-derived Web and host application
codex/                    Customized Codex runtime subtree
docs/product-design.md    Canonical product requirements and release scope
docs/capability-baseline.md
docs/development-plan.md  Executable milestone plan
docs/codex-upstream-sync.md
scripts/                  Monorepo and upstream-sync tooling
```

## Get started

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
