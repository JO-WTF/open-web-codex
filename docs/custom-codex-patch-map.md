# Custom Codex patch map

This file records the current product-specific seams under `codex/`. Generated schemas are
derivatives of these source changes and are not separate custom behavior.

The integrated official base is `5bed6447998c754d154dbd796517310b8f04d4ce`.

| Seam | Source paths | Decision | Removal condition |
| --- | --- | --- | --- |
| Provider catalog and active-provider selection | `app-server-protocol/src/protocol/v2/model.rs`, `app-server/src/request_processors/catalog_processor.rs`, adjacent request registration and capability declarations | retain | Official app-server exposes the required provider-scoped catalog, selection and generated capability contract used by the Web platform |
| Provider-compatible chat tool translation | `codex-api/src/chat_translate.rs` and focused tests | retain | Official translation preserves interrupted and mixed tool-call outputs for supported third-party wire APIs |
| Legacy raw response tool history | `app-server-protocol/src/protocol/legacy_response_tool_history.rs` plus the small integration points in `thread_history.rs` | retain as compatibility seam | Supported Profiles no longer contain legacy rollouts whose tools exist only as raw `ResponseItem` call/output pairs, or upstream materializes those pairs itself |

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
