# Codex Capability Gap Baseline

## Audit Scope

| Field | Value |
| --- | --- |
| CodexMonitor commit | `c8996a46b27098bdbf854eb95ca9f02e6fc3573c` |
| Existing protocol reference | `docs/app-server-events.md` |
| Outgoing request source | `src-tauri/src/shared/codex_core.rs` |
| Process and routing source | `src-tauri/src/backend/app_server.rs` |
| Frontend event allowlist | `src/utils/appServerEvents.ts` |
| Frontend event router | `src/features/app/hooks/useAppServerEvents.ts` |

The Codex source repository referenced by `docs/app-server-events.md` is not present beside this workspace, and the installed Windows App binary cannot be executed from the current shell. This baseline therefore distinguishes verified local behavior from upstream methods that still require confirmation by the parallel Codex Rust project.

## Current Initialize Contract

CodexMonitor starts `codex app-server`, sends `initialize`, waits up to 15 seconds, and then sends the `initialized` notification.

```json
{
  "clientInfo": {
    "name": "codex_monitor",
    "title": "Codex Monitor",
    "version": "<application version>"
  },
  "capabilities": {
    "experimentalApi": true
  }
}
```

Current limitations:

- The initialize response is checked only for success; no typed capability data is persisted.
- There is no Capability Manifest, protocol version range, feature limit or stable compatibility state.
- The synthetic `codex/connected` event reports only the Workspace ID.
- Version incompatibility is generally discovered when a request fails, not before a Run is scheduled.

## Current Outgoing Requests

The following app-server requests are verified in the current Rust source:

| Domain | Methods |
| --- | --- |
| Session | `initialize`, followed by `initialized` notification |
| Thread | `thread/start`, `thread/resume`, `thread/read`, `thread/fork`, `thread/list`, `thread/archive`, `thread/compact/start`, `thread/name/set` |
| Turn | `turn/start`, `turn/steer`, `turn/interrupt`, `review/start` |
| Discovery | `model/list`, `experimentalFeature/list`, `collaborationMode/list`, `mcpServerStatus/list` |
| Account | `account/login/start`, `account/login/cancel`, `account/rateLimits/read`, `account/read` |
| Skills and Apps | `skills/list`, `app/list` |

Configuration writes for feature flags, Agents and files are currently performed by local/daemon Rust code against `config.toml` or the Codex Home. They are not app-server client requests and cannot be treated as an upstream Web contract.

## Current Incoming Server Requests

The backend forwards JSON-RPC messages with a method and request ID to the frontend. The UI has explicit product handling for:

| Method | Current handling |
| --- | --- |
| `item/commandExecution/requestApproval` | Converted to an approval item and answered with the original request ID |
| `item/fileChange/requestApproval` | Converted to an approval item and answered with the original request ID |
| `item/permissions/requestApproval` | Converted to an approval item and answered with the original request ID |
| `item/tool/requestUserInput` | Converted to structured user input and answered with the original request ID |

Verified gaps from the pinned v2 reference:

- `item/tool/call` has no platform handler.
- `account/chatgptAuthTokens/refresh` has no platform handler.
- `mcpServer/elicitation/request` has no platform handler.
- Requests are held in frontend/task state rather than a multi-user durable Approval store.
- Request ownership is Workspace-based; there is no Profile/Run lease validation.

## Current Notification Surface

`SUPPORTED_APP_SERVER_METHODS` currently contains 32 methods:

- Account and app: `account/login/completed`, `account/rateLimits/updated`, `account/updated`, `app/list/updated`.
- Thread: `thread/started`, `thread/status/changed`, `thread/name/updated`, `thread/tokenUsage/updated`, `thread/archived`, `thread/unarchived`, `thread/closed`.
- Turn: `turn/started`, `turn/plan/updated`, `turn/diff/updated`, `turn/completed`.
- Item lifecycle: `item/started`, `item/completed`.
- Item deltas: Agent message, command output/interaction, file output, plan, reasoning text/summary.
- Hooks and errors: `hook/started`, `hook/completed`, `error`.
- User input: `item/tool/requestUserInput`.
- Synthetic bridge events: `codex/connected`, `codex/backgroundThread`, `codex/event/skills_update_available`.

Context compaction reaches the UI through `item/started` and `item/completed` with `item.type = "contextCompaction"`. Memory consolidation Threads are detected by the backend and hidden as synthetic background Threads; V1 does not yet expose a durable Memory health or governance contract.

Multi-agent relationships are partially recovered from Thread IDs and collaboration item payloads. There is no versioned contract declaring complete parent/child lifecycle, native limits or recovery guarantees.

## V1 Capability Gaps

| Gap ID | Required capability | Verified current state | Upstream task | Web module blocked | Required evidence |
| --- | --- | --- | --- | --- | --- |
| GAP-CAP-001 | Versioned Capability Manifest | Missing | `CR-001`, `CR-002`, `CR-006` | Capability gate, all Studio modules | Manifest Fixture and incompatible-version test |
| GAP-CAP-002 | Generated request/response/event Schema | Ad hoc TypeScript/Rust values | `CR-003`, `CR-005`, `CR-007` | Generated contracts and CI | Published Schema package and consistency CI |
| GAP-CAP-003 | Stable structured errors | Mostly strings and raw responses | `CR-004` | API error mapping and retry rules | Error Fixture matrix |
| GAP-PRO-001 | Multi-Workspace registration contract | Local Session maps multiple Workspace roots without an explicit upstream capability | `CR-101`, `CR-102` | Profile Host registration | Two-Workspace real Smoke Test |
| GAP-PRO-002 | Thread read/recovery guarantees | `thread/read` is used; compatibility and recovery semantics are undeclared | `CR-103`, `CR-108` | Thread recovery | Restart and resume Fixture/Smoke |
| GAP-MEM-001 | Compaction and consolidation lifecycle | Compaction items visible; consolidation is hidden | `CR-104`, `CR-105` | Memory status UI | Lifecycle event Fixtures |
| GAP-MEM-002 | Memory status, export and reset | Missing | `CR-106`, `CR-107` | Memory diagnostics/Danger Zone | Read/export/reset integration tests |
| GAP-AGT-001 | Native Agent CRUD via app-server | Current writes are local Codex Home operations | `CR-201`, `CR-202` | Native Agents Studio | CRUD/reload Fixtures |
| GAP-AGT-002 | Native multi-agent settings and limits | Local config fields exist; no versioned bridge | `CR-203`, `CR-205` | Multi-agent settings | Limits and error Fixtures |
| GAP-AGT-003 | Complete parent/child collaboration events | Partial relation inference exists | `CR-204`, `CR-207` | Multi-agent trajectory | Real multi-agent replay and Smoke |
| GAP-AGT-004 | Agent configuration snapshot | Missing stable Run reference | `CR-206` | Run details/history | Snapshot Schema and replay |
| GAP-SKL-001 | Skill read/write/delete | Only `skills/list` plus local file operations | `CR-301`, `CR-302` | Skills editor/publish | CRUD Fixtures |
| GAP-SKL-002 | Native validation and safe path errors | Platform has file policy but no native Skill validation bridge | `CR-303`, `CR-304` | Validate action | Validation/security matrix |
| GAP-SKL-003 | Reload, discovery and isolated test | Synthetic update signal only | `CR-305`, `CR-306` | Publish/test flow | Reload and test hook Smoke |
| GAP-SKL-004 | Scope, source and version metadata | Partial paths only | `CR-307`, `CR-308` | Scope/version UI | Scope and override Fixtures |
| GAP-PLG-001 | Plugin list/read/Manifest | `app/list` is not a Plugin lifecycle contract | `CR-401`, `CR-403`, `CR-405` | Plugin Manager read views | Manifest and capability Fixtures |
| GAP-PLG-002 | Install/update/enable/disable/uninstall | Missing | `CR-402`, `CR-406` | Plugin write actions | Full lifecycle integration test |
| GAP-PLG-003 | Permission diff and integrity | Missing | `CR-404`, `CR-407` | Permission confirmation | Upgrade permission Fixtures |
| GAP-MCP-001 | MCP configuration CRUD and reload | Status list only; reload missing | `CR-501`, `CR-502` | MCP configuration | CRUD/reload integration test |
| GAP-MCP-002 | Tool discovery and permission contract | Partial MCP call events; no management contract | `CR-503` | Tools list/policy | Discovery and permission Fixtures |
| GAP-MCP-003 | OAuth lifecycle | Missing request and completion routing | `CR-504` | OAuth flow | State/callback/replay tests |
| GAP-MCP-004 | Elicitation lifecycle | Server request is unhandled | `CR-505` | Structured elicitation | Request/response/expiry Fixtures |
| GAP-MCP-005 | Disable/delete and structured errors | Missing | `CR-506`, `CR-507`, `CR-508` | MCP lifecycle and diagnostics | Failure and active-Turn tests |

## Web-Side Work That Does Not Require New Runtime Semantics

- Replace Tauri Event transport with authenticated WebSocket transport.
- Persist platform Task/Run/Profile/Approval mappings and audit records.
- Implement RBAC, Control Lease, Secret references, Repository and Worktree isolation.
- Build UI only for capabilities declared by the Manifest.
- Preserve Codex as the source of truth for Thread, Turn, multi-agent decisions and Memory.

## Audit Corrections

- `thread/read` is already sent by `src-tauri/src/shared/codex_core.rs`; it must be listed as supported rather than missing in the existing protocol reference.
- Existing Agent writes, feature flag writes and file writes must be labeled local/daemon operations, not app-server capabilities.
- `app/list` must not be presented as equivalent to the required Plugin install/read/update/uninstall contract.
