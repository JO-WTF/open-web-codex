# Custom Codex patch map

This is the current, replay-oriented record of product-specific seams under
`codex/`. It is the authority for deciding which local changes are reapplied
after an official subtree update. Generated schemas, TypeScript definitions,
fixtures, and snapshots are derivatives of the source seams and are not
independent custom behavior.

The integrated official base is `5bed6447998c754d154dbd796517310b8f04d4ce`.
The target is a small, explicit set of Provider Runtime and TUI seams; it is
not a zero-diff Codex subtree.

## Long-term acceptance criteria

1. Every non-generated `codex/` difference is classified as `retain-core`,
   `upstreamed`, `move-out`, or `drop`.
2. Only the retained seams below remain after convergence. `core` keeps only
   the minimum transport dispatch required by the Provider Runtime.
3. The Web platform owns Profiles, credentials injection, authorization,
   Provider CRUD orchestration, and browser DTOs. It never exposes raw
   app-server JSON-RPC or configuration paths.
4. A current official Codex update can be replayed in the documented order,
   followed by generated-contract validation, focused Runtime/TUI tests, and a
   real Provider app-server smoke.

## Current short-term objective

The active task is the source-difference inventory. The retained core seams
below are recorded now; all remaining non-generated differences must be
classified before they are moved or removed. Do not infer that a difference is
unnecessary solely because it is outside the Provider paths: first verify
whether current upstream already supplies its behavior.

Use `scripts/codex-customization-status.sh` as the inventory input. It compares
`HEAD:codex` directly with the current `codex-upstream/main` tree; this
repository's `main` branch is never the convergence baseline.
`.sync/codex-customization-inventory.json` records the latest comparison
commit, counts, and classification progress. Refresh it whenever the inventory
is updated or an official sync changes the target tree.

## Retained seams

| ID | Seam and source paths | Reason to retain | Replay order | Required validation | Removal condition |
| --- | --- | --- | --- | --- | --- |
| `provider-chat-transport` | `codex-api/src/chat_translate.rs`, `chat_translate_tests.rs`, `common.rs`, `endpoint/chat.rs`, `sse/chat.rs`; minimal dispatch in `core/src/client.rs` | Translates third-party Chat Completions requests, streams, and mixed/interrupted tool calls into Codex semantics. | 1 | `just test -p codex-api`; focused interrupted and tool-call translation tests | Upstream provides equivalent supported third-party wire translation, including the same stream and tool behavior. |
| `provider-metadata-models` | `model-provider-info/src/lib.rs`, `PROVIDER_MODELS.md`, `model-provider/src/provider.rs`, `models_endpoint.rs`, `models-manager/src/manager.rs` | Defines `WireApi::Chat`, Provider-scoped model metadata, model discovery, selection, normalization, and cache isolation. | 2 | `just test -p codex-model-provider`; `just test -p codex-models-manager`; Provider switch/cache-isolation smoke | Upstream exposes equivalent Provider metadata, scoped catalog, and cache semantics. |
| `provider-app-server-api` | `app-server-protocol/src/protocol/v2/model.rs`, `app-server/src/request_processors/catalog_processor.rs`, `app-server/src/models.rs`, request registration, generated capability declarations | Provides versioned Provider listing, Provider-scoped model listing, controlled selection/configuration, and forced refresh for both TUI and Platform Host. | 3 | `just test -p codex-app-server-protocol`; `just test -p codex-app-server model_list`; generated Schema/TypeScript; real app-server Provider smoke | Upstream provides the required stable API and generated contract. |
| `provider-tui-workflows` | `tui/src/chatwidget/provider_popups.rs`, `provider_sections.rs`, `model_popups.rs`, `settings.rs`, `slash_dispatch.rs`, `config_update.rs`, `onboarding/auth.rs`, `slash_command.rs` | TUI Provider selection, model selection, onboarding, refresh, configuration, and error UX are product-critical client behavior. | 4 | `just test -p codex-tui`; Provider workflow snapshots | Upstream TUI provides equivalent Provider and model workflows, or the product explicitly retires TUI parity. |
| `legacy-response-tool-history` | `app-server-protocol/src/protocol/legacy_response_tool_history.rs`, narrow integration in `thread_history.rs` | Existing Profiles can contain raw `ResponseItem` tool-call/output pairs that official semantic history projection does not materialize. | 5 | Protocol tests plus reload fixture containing raw call/output pairs | Supported Profiles no longer contain this rollout format, or upstream materializes it. |
| `capability-manifest` | `app-server-protocol/src/capability_manifest.rs`, `initialize_processor.rs`, generated Schema/TypeScript, `apps/web/contracts/codex/**` fixtures | Lets the Platform Host gate product features against the actual Runtime build instead of assuming support. | 6 | Protocol tests, generated artifacts, fixture replay, `--require-manifest` smoke | Method facts and experimental state are generated from the official protocol/build and an upstream equivalent contract exists. |

## Required boundaries

- Keep Chat translation in `codex-api`, Provider facts in Provider crates,
  Provider wire types in `app-server-protocol`, request handling in
  `app-server`, and presentation in dedicated TUI Provider modules.
- Do not add Web routes, Tauri commands, Profile lifecycle, authorization,
  browser state, or raw RPC proxies under `codex/`.
- Do not hand-edit generated Schema or TypeScript files.
- Do not broaden `core/src/client.rs`; extract Provider transport behavior into
  Provider-specific modules and keep only the dispatch seam in Core.
- Do not preserve an implementation that upstream now supplies. Mark it
  `upstreamed`, return to upstream code, and remove it from this table.

## Sync and progress protocol

For every official sync:

1. Run `scripts/codex-upstream-status.sh` and compare each retained seam with
   the new upstream implementation.
   Run `scripts/codex-customization-status.sh` to record the exact source-tree
   difference set against that official commit.
2. Apply seams in the `Replay order` column.
3. Regenerate protocol artifacts after each protocol/configuration change.
4. Run the seam validations, then the Web contract checks and real smoke.
5. Update this file with the current paths, symbols, validation evidence, and
   removal conditions. Do not retain historical status narratives here.

For every convergence change:

1. Add its source files to the inventory with one of the four classifications.
2. Link any retained behavior to a seam above, or create a narrowly scoped new
   seam with a reason, validation command, and removal condition.
3. Move Web/platform behavior to `apps/web` before deleting its Runtime
   counterpart.

## Legacy response tool history audit

Official paginated rollout history persists semantic `ItemCompleted(TurnItem)` records and
`thread_history_projection.rs` intentionally ignores raw `ResponseItem` records. Existing Profiles
can still contain older rollouts with only `FunctionCall`/`CustomToolCall` and matching output
records. Without the compatibility seam those tool calls disappear after reload.

The seam is restricted to protocol projection:

- it correlates raw calls and outputs by `call_id`;
- a richer semantic item with the same ID always wins;
- an unmatched call is closed as failed when its Turn ends;
- it does not change model-visible context, tool execution or Thread/Turn ownership;
- it is isolated in one module so an upstream replacement can remove it atomically.

No additional Codex changes are required for browser event replay. Live notification durability,
cursor replay and UI reconciliation are owned by `apps/web`.
