# Codex upstream synchronization

## Goal

The `codex/` directory contains the customized runtime and tracks
`openai/codex/main`. To keep the monorepo clone compact, main contains subtree
snapshots rather than every imported commit. The sync tools fetch the customized
branch and official history before merging, restoring the common upstream base
without replacing the entire directory.

## Inspect

```bash
./scripts/codex-upstream-status.sh
./scripts/codex-customization-status.sh
```

The command configures local `codex-upstream` and `codex-fork` remotes, fetches
official main, and reports official commits beyond the recorded integrated base.
It does not modify source files.

The customization status command compares `HEAD:codex` directly with
`codex-upstream/main`. `docs/custom-codex-patch-map.md` is the authoritative
human-readable seam record and `.sync/codex-customization-inventory.json`
records the current machine-readable comparison and validation state. The
`customSource*` fields in `.sync/codex-upstream.json` are import provenance;
daily synchronization tracks only `openai/codex/main`.

## Apply

Start from clean `main`:

```bash
./scripts/sync-codex-upstream.sh --apply
```

The script creates `codex/sync-upstream-<sha>`, performs a squash subtree pull
into `codex/`, and records the integrated upstream commit. Push that branch and
review it like a normal runtime change.

## Conflict policy

Resolve by architectural layer:

1. Accept official file/module moves and public API shapes first.
2. Reapply retained seams in this order: Chat transport; Provider metadata,
   model discovery and cache; app-server Provider API; TUI Provider workflows;
   legacy history compatibility; Capability Manifest; generated artifacts.
3. Do not preserve a custom workaround when upstream now provides the behavior.
4. Keep protocol/schema generated files aligned with their Rust source.
5. Avoid mixing product Web changes into an upstream runtime sync.

If conflicts are broad, abort the subtree merge and split the sync into an
upstream-only checkpoint followed by small custom reapplication commits.

## Required validation

At minimum, follow `codex/AGENTS.md` and run:

```bash
cd codex/codex-rs
just fmt
just test -p codex-app-server-protocol
just test -p codex-app-server model_list
just test -p codex-models-manager
just test -p codex-model-provider
just test -p codex-tui
```

Then run:

```bash
cd apps/web
npm run check:codex-contracts
npm run smoke:codex-app-server -- --require-manifest
```

Update both `.sync/codex-upstream.json` and
`.sync/codex-customization-inventory.json` after the sync. A complete Codex
test suite still requires explicit approval under the imported Codex repository
rules.

## Release rule

Never point production at a moving upstream branch. Merge an inspected sync
branch, build a specific commit, publish its generated contract bundle and
binary digest, canary that digest, and keep the previous compatible binary for
rollback.
