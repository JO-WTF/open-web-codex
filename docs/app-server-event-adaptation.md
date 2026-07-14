# App-server event adaptation

This document records the current Web projection of every server-initiated app-server message.
Protocol names and payloads come from `codex-rs/app-server-protocol`; generated contracts remain
the source of truth.

Status legend:

- ✅ Fully adapted: projected into Web state or a purpose-built conversation component.
- 🟡 Partially adapted: delivered and retained through a generic lifecycle/fallback path, but a
  specialized interaction or incremental detail is still missing.
- ⬜ Not adapted: no Web behavior is currently attached. These events must not be shown as raw
  protocol JSON.
- N/A: platform/internal event that intentionally has no conversation row.

## Durable rollout history

Existing local rollouts currently contain the following record types. `ThreadHistoryBuilder`
reconstructs semantic event records first, then reconstructs unmatched Responses tool calls as
`DynamicToolCall`. A semantic item with the same call ID replaces the generic fallback, preventing
duplicate cards.

| Persisted type | Web behavior | Update model | Status |
|---|---|---|---|
| `session_meta` | Thread identity, workspace, source, and runtime metadata | thread-level state; not a message | N/A |
| `turn_context` | Model, sandbox, approval, and runtime context for replay | Turn metadata; not a message | N/A |
| `event_msg.user_message` | User message | append | ✅ |
| `event_msg.agent_message` | Assistant commentary/final message | append | ✅ |
| `event_msg.agent_reasoning` | Reasoning timeline item | append/merge adjacent reasoning | ✅ |
| `event_msg.task_started` | Opens the Turn | state transition | ✅ |
| `event_msg.task_complete` | Completes the Turn and enables history folding | state transition | ✅ |
| `event_msg.turn_aborted` | Failed/interrupted Turn state | state transition | ✅ |
| `event_msg.patch_apply_end` | File-change Diff card | update one item by call ID | ✅ |
| `event_msg.web_search_end` | Search tool card | update one item by call ID | ✅ |
| `event_msg.token_count` | Context usage | thread-level state; not a message | ✅ |
| `response_item.message` | User/assistant content and hook prompts | append | ✅ |
| `response_item.reasoning` | Model reasoning payload | append | ✅ |
| `response_item.web_search_call` | Search history (semantic event wins when present) | update one item | ✅ |
| `response_item.function_call` + `function_call_output` | Generic or command tool card with arguments/output | update one item by `call_id` | ✅ |
| `response_item.custom_tool_call` + `custom_tool_call_output` | Expandable generic tool; `apply_patch` becomes Diff | update one item by `call_id` | ✅ |

All `ThreadItem` variants returned by `thread/resume` and `thread/read` have a visible projection:
`userMessage`, `hookPrompt`, `agentMessage`, `plan`, `reasoning`, `commandExecution`, `fileChange`,
`mcpToolCall`, `dynamicToolCall`, `collabAgentToolCall`, `subAgentActivity`, `webSearch`, `imageView`,
`sleep`, `imageGeneration`, `enteredReviewMode`, `exitedReviewMode`, and `contextCompaction`.
Unknown future variants use an expandable generic card rather than disappearing.

## Server requests

| Method | Behavior | Web status | Missing behavior |
|---|---|---|---|
| `item/commandExecution/requestApproval` | Command approval card and response | ✅ | — |
| `item/fileChange/requestApproval` | File-change approval | 🟡 | Dedicated patch approval card |
| `item/tool/requestUserInput` | Clickable choices/free text/secret response | ✅ | — |
| `mcpServer/elicitation/request` | MCP asks user for structured input | ⬜ | Elicitation form and response |
| `item/permissions/requestApproval` | Additional permission approval | ⬜ | Permission scope card and response |
| `item/tool/call` | Client-executed dynamic tool | 🟡 | Browser-side tool executor; history card is complete |
| `account/chatgptAuthTokens/refresh` | External token refresh callback | N/A | Owned by authenticated platform session |
| `attestation/generate` | Client attestation callback | N/A | Not a conversation concern |
| `currentTime/read` | External clock callback | N/A | Not a conversation concern |
| legacy `applyPatchApproval` | Legacy patch approval | ⬜ | Legacy-only compatibility UI |
| legacy `execCommandApproval` | Legacy command approval | ⬜ | Legacy-only compatibility UI |

## Server notifications

| Method | Behavior | Web status | Missing behavior |
|---|---|---|---|
| `error` | Recoverable reconnect or terminal error notice | ✅ | — |
| `thread/started` | Initialize thread metadata | ✅ | — |
| `thread/status/changed` | Running/idle/error state | ✅ | — |
| `thread/archived` | Thread collection mutation | ⬜ | Refresh/remove sidebar row |
| `thread/deleted` | Thread collection mutation | ⬜ | Refresh/remove sidebar row |
| `thread/unarchived` | Thread collection mutation | ⬜ | Refresh sidebar row |
| `thread/closed` | Loaded-thread lifecycle | ⬜ | Clear active runtime state |
| `skills/changed` | Skills catalog invalidation | N/A | No conversation row |
| `thread/name/updated` | Sidebar title update | 🟡 | Immediate row update; periodic refresh still works |
| `thread/goal/updated` | Goal panel update | ✅ | — |
| `thread/goal/cleared` | Clear goal panel | ✅ | — |
| `thread/settings/updated` | Composer/thread settings state | ✅ | — |
| `thread/tokenUsage/updated` | Context usage state | ✅ | — |
| `turn/started` | Start live Turn and Working state | ✅ | — |
| `hook/started` | Hook execution lifecycle | 🟡 | Dedicated running hook card |
| `turn/completed` | Finish Turn, stop Working, fold execution history | ✅ | — |
| `hook/completed` | Hook execution lifecycle | 🟡 | Dedicated hook result/details card |
| `turn/diff/updated` | Aggregate Diff card and goal file statistics | ✅ | — |
| `turn/plan/updated` | Goal/plan step state | ✅ | — |
| `item/started` | Create the correct live item component | ✅ | — |
| `item/autoApprovalReview/started` | Guardian review progress | 🟡 | Dedicated risk/review presentation |
| `item/autoApprovalReview/completed` | Guardian decision | 🟡 | Dedicated decision detail presentation |
| `item/completed` | Update the existing item in place | ✅ | — |
| `rawResponseItem/completed` | Internal cloud payload | N/A | Intentionally not rendered |
| `item/agentMessage/delta` | Stream one assistant message in place | ✅ | — |
| `item/plan/delta` | Stream plan text | 🟡 | Plan item delta card; final plan is supported |
| `command/exec/outputDelta` | Standalone command session output | ⬜ | Standalone command panel |
| `process/outputDelta` | Spawned-process output | ⬜ | Process panel |
| `process/exited` | Spawned-process completion | ⬜ | Process panel |
| `item/commandExecution/outputDelta` | Append output to one command card | ✅ | — |
| `item/commandExecution/terminalInteraction` | Keep command active and record only a redacted interaction marker | ✅ | — |
| `item/fileChange/outputDelta` | Deprecated patch text stream | 🟡 | Deprecated; final `fileChange` item is supported |
| `item/fileChange/patchUpdated` | Update one Diff card | 🟡 | Incremental patch rendering |
| `serverRequest/resolved` | Remove/resolve pending interaction | ✅ | — |
| `item/mcpToolCall/progress` | MCP progress message | 🟡 | Incremental progress inside the existing MCP card |
| `mcpServer/oauthLogin/completed` | MCP authentication result | ⬜ | Authentication notice/action |
| `mcpServer/startupStatus/updated` | MCP server count/error state | ✅ | — |
| `account/updated` | Authentication state | N/A | Owned by platform account UI |
| `account/rateLimits/updated` | Rate-limit state | ✅ | — |
| `app/list/updated` | Apps catalog invalidation | N/A | No conversation row |
| `remoteControl/status/changed` | Remote-control state | N/A | No conversation row |
| `externalAgentConfig/import/progress` | Import progress | ⬜ | Settings progress UI |
| `externalAgentConfig/import/completed` | Import result | ⬜ | Settings completion UI |
| `fs/changed` | Watched filesystem invalidation | 🟡 | File tree refresh currently follows explicit operations |
| `item/reasoning/summaryTextDelta` | Stream reasoning summary in place | ✅ | — |
| `item/reasoning/summaryPartAdded` | Preserve reasoning summary boundary | ✅ | — |
| `item/reasoning/textDelta` | Stream reasoning content in place | ✅ | — |
| `thread/compacted` | Legacy context-compaction notice | 🟡 | New `contextCompaction` item is fully supported |
| `model/rerouted` | Model routing notice | ⬜ | Visible model change notice |
| `model/verification` | Model verification notice | ⬜ | Visible verification notice |
| `turn/moderationMetadata` | Moderation metadata | N/A | Must not expose raw metadata |
| `model/safetyBuffering/updated` | Safety buffering state | ⬜ | Non-intrusive status indication |
| `warning` | Runtime warning | 🟡 | Generic fallback only; dedicated severity styling |
| `guardianWarning` | Guardian warning | 🟡 | Generic fallback only; dedicated risk styling |
| `deprecationNotice` | Compatibility notice | 🟡 | Generic fallback only; settings-level presentation |
| `configWarning` | Configuration warning | 🟡 | Generic fallback only; settings action link |
| `fuzzyFileSearch/sessionUpdated` | File-search results | N/A | Owned by file-search surface |
| `fuzzyFileSearch/sessionCompleted` | File-search completion | N/A | Owned by file-search surface |
| `thread/realtime/started` | Realtime session state | ⬜ | Realtime UI |
| `thread/realtime/itemAdded` | Realtime item | ⬜ | Realtime UI |
| `thread/realtime/transcript/delta` | Realtime transcript streaming | ⬜ | Realtime UI |
| `thread/realtime/transcript/done` | Realtime transcript completion | ⬜ | Realtime UI |
| `thread/realtime/outputAudio/delta` | Realtime audio output | ⬜ | Audio player/buffer |
| `thread/realtime/sdp` | Realtime transport negotiation | N/A | Transport-owned |
| `thread/realtime/error` | Realtime error | ⬜ | Realtime error notice |
| `thread/realtime/closed` | Realtime close state | ⬜ | Realtime teardown UI |
| `windows/worldWritableWarning` | Windows sandbox warning | N/A | Platform-specific settings surface |
| `windowsSandbox/setupCompleted` | Windows sandbox setup result | N/A | Platform-specific settings surface |
| `account/login/completed` | Login result | N/A | Owned by platform account UI |

## Rendering and update rules

| Semantic category | Rule |
|---|---|
| User and assistant messages | Append in protocol order. Commentary remains inside the execution group; the last completed assistant message is the final answer. |
| Reasoning | Append distinct reasoning items; update deltas on the same item ID. |
| Command/process execution | Create at `item/started`, append output and terminal activity to the same card, finalize at `item/completed`. |
| File changes | One Diff card per item/call ID; patch updates mutate that card. |
| MCP/dynamic/collaboration/image/search tools | One icon-specific expandable card per item/call ID; completion replaces running state. |
| Turn execution group | Live items are top-level and visible. Only after final completion are process items folded; the final answer remains outside. |
| Unknown future item | Render a generic card with its type and status instead of dropping it; do not expose the raw protocol payload. |
