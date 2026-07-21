# Tauri Migration Inventory

## Snapshot

| Item | Count |
| --- | ---: |
| Commands registered by `tauri::generate_handler!` | 125 |
| Rust files containing the desktop backend | 96 |
| Total files under `src-tauri` | 190 |
| Frontend files importing `@tauri-apps/*` | 60 |
| Frontend `@tauri-apps/*` import occurrences | 115 |
| Lines in `src/services/tauri.ts` | 1,212 |
| Direct `invoke` calls in `src/services/tauri.ts` | 129 |
| Lines in `src/services/events.ts` | 370 |
| Desktop/mobile icon files | 53 |
| Generated Apple project files | 32 |

The command registry in `src-tauri/src/lib.rs` is the canonical source for the command count. Conditional real/stub command definitions are counted once at the registry boundary.

## Current blocking chain

The first blocking edge has been removed: the Platform Server's real adapter
now owns a native Profile Host connection and does not proxy the Tauri daemon.
The remaining compatibility chain is:

```text
Browser compatibility services
  -> raw /api/rpc and SSE Gateway
  -> transitional platform RPC adapter and Tauri compatibility commands
  -> src-tauri shared Provider/Git/Workspace cores
  -> native Profile Host for Codex; src-tauri for remaining local Git/desktop behavior
```

The removal path reverses that ownership. Native Profile Host, Git Runtime and
platform DTO services become the source of truth first; Tauri becomes a thin
compatibility client of those services; browser imports then move to
`PlatformClient` and authenticated WebSocket events; desktop-only features and
packaging are deleted last.

The monorepo root CI does not yet enforce `check:tauri-boundary`, and the script
currently assumes standalone `apps/web` Git paths. Make the guard prefix-aware
and wire it into root CI before beginning broad extraction, so Tauri references
can only decrease during the migration.

## Command Inventory

| Domain | Count | Registered commands | V1 disposition |
| --- | ---: | --- | --- |
| Settings | 3 | `get_app_settings`, `update_app_settings`, `get_codex_config_path` | Split into user, project, Profile and platform Web APIs; extract reusable validation |
| Files | 4 | `file_read`, `file_write`, `read_image_as_data_url`, `write_text_file` | Replace with authorized server APIs and Artifact transport; no arbitrary host paths |
| Codex | 40 | Doctor/update; Thread start/read/resume/fork/list/archive/compact/name/live; Turn send/steer/interrupt/review; approvals; model/features; Agents; account/login; Skills; Apps; MCP status; collaboration modes; generated metadata | Extract app-server transport and protocol adapter into Codex Host; preserve native Codex semantics; remove desktop updater ownership |
| Workspaces | 20 | List/add/clone/worktree/remove/rename/apply/settings/connect/files/open app/runtime args | Split into Project/Repository/Runner APIs; retain Git/Worktree core; delete arbitrary local folder and open-in-app behavior |
| Git | 26 | Status/init/GitHub/list roots/diffs/log/remote/stage/revert/commit/push/pull/fetch/sync/branches/PR operations | Extract to `git-runtime`; expose authorized server APIs; keep Commit/Push explicit and audited |
| Prompts | 7 | List/create/update/delete/move/workspace dir/global dir | Map to Profile or repository-owned content; remove browser-visible host paths |
| Terminal | 4 | Open/write/resize/close | V1 replaces with read-only command logs; interactive shell is deferred and removed from the initial Web path |
| Dictation | 8 | Model status/download/remove/start/permission/stop/cancel | Delete from V1 and remove `whisper-rs`, audio permission and model-download ownership |
| Notifications | 3 | macOS debug/build type/native fallback | Replace with in-app notifications and optional Web Push; delete native fallback |
| Tray | 2 | Recent Threads and usage | Delete tray integration; data remains available in Web navigation/dashboard |
| Menu | 1 | Accelerator updates | Replace with browser command palette and scoped keyboard shortcuts |
| Local usage | 1 | Local session usage snapshot | Replace with server-side usage/telemetry subject to Codex bridge and privacy rules |
| Tailscale | 5 | Status, daemon preview/start/stop/status | Remove from user client; deployment networking belongs to server operations |
| Runtime | 1 | `is_mobile_runtime` | Delete with mobile/Tauri runtime branches |

## Event Inventory

| Event | Current owner | V1 disposition |
| --- | --- | --- |
| `app-server-event` | Rust Event Sink and remote backend | Replace Tauri Event with authenticated WebSocket events and cursor replay |
| `terminal-output` | Rust Event Sink | Replace with bounded Run log events and Artifact references |
| `terminal-exit` | Rust Event Sink | Replace with command/run terminal state event |
| `dictation-download` | Dictation runtime | Delete |
| `dictation-event` | Dictation runtime | Delete |
| `updater-check` | Native application menu | Delete; server deployment owns upgrades |
| `tray-open-thread` | Native tray | Delete; use normal Web route navigation |
| `menu-new-agent` | Native menu | Replace with Web command action |
| `menu-new-worktree-agent` | Native menu | Replace with Web Task creation action |
| `menu-new-clone-agent` | Native menu | Replace with Project/Task action |
| `menu-add-workspace` | Native menu | Replace with Project creation action |
| `menu-add-workspace-from-url` | Native menu | Replace with Repository connection action |
| `menu-open-settings` | Native menu | Replace with Web route action |
| `menu-toggle-projects-sidebar` | Native menu | Replace with UI state and shortcut |
| `menu-toggle-git-sidebar` | Native menu | Replace with Inspector state and shortcut |
| `menu-toggle-debug-panel` | Native menu | Remove from production UI; retain authorized diagnostics |
| `menu-toggle-terminal` | Native menu | Remove from V1 interactive shell path |
| `menu-next-agent` / `menu-prev-agent` | Native menu | Replace with Task navigation shortcuts |
| `menu-next-workspace` / `menu-prev-workspace` | Native menu | Replace with Project navigation shortcuts |
| `menu-composer-cycle-model` | Native menu | Replace with Composer control |
| `menu-composer-cycle-access` | Native menu | Replace with Composer control |
| `menu-composer-cycle-reasoning` | Native menu | Replace with Composer control |
| `menu-composer-cycle-collaboration` | Native menu | Replace with Composer control |
| `tauri://focus` / `tauri://blur` | Tauri Window | Replace with Page Visibility and browser focus events |
| Native drag/drop | Tauri Window | Replace with browser drag/drop and upload APIs |

## Tauri Plugin Inventory

### Rust Plugins

| Plugin | V1 disposition |
| --- | --- |
| `tauri-plugin-updater` | Delete; deployment pipeline owns upgrades |
| `tauri-plugin-window-state` | Delete; retain allowed panel preferences in Web storage |
| `tauri-plugin-liquid-glass` | Delete |
| `tauri-plugin-opener` | Replace external URL operations with safe browser navigation; remove host reveal/open |
| `tauri-plugin-dialog` | Replace with Web Dialog/Drawer and file upload/download |
| `tauri-plugin-process` | Delete from frontend; server process control stays behind authorized APIs |
| `tauri-plugin-notification` | Replace with in-app notification and optional Web Push |
| Tauri tray/window/menu APIs | Delete or replace with Web Shell controls |

### Frontend Packages

| Package | Import occurrences | V1 disposition |
| --- | ---: | --- |
| `@tauri-apps/api/app` | 1 | Replace with server build/version API |
| `@tauri-apps/api/core` | 21 | Replace `invoke` with `PlatformClient`; replace asset conversion with server URLs |
| `@tauri-apps/api/dpi` | 8 | Delete native menu positioning dependency |
| `@tauri-apps/api/event` | 4 | Replace with WebSocket Event Store |
| `@tauri-apps/api/menu` | 8 | Replace with Web menus |
| `@tauri-apps/api/webview` | 1 | Replace with browser behavior |
| `@tauri-apps/api/window` | 23 | Replace focus/visibility; delete drag, caption and native effects |
| `@tauri-apps/plugin-dialog` | 15 | Replace with Web confirmation, upload and download flows |
| `@tauri-apps/plugin-notification` | 4 | Replace with notification service |
| `@tauri-apps/plugin-opener` | 23 | Keep safe HTTP navigation; remove host path reveal/open |
| `@tauri-apps/plugin-process` | 3 | Delete relaunch path |
| `@tauri-apps/plugin-updater` | 4 | Delete desktop updater UI |

## Desktop Build And Release Inventory

| Surface | Files or scripts | V1 disposition |
| --- | --- | --- |
| Tauri configuration | `tauri.conf.json`, Windows/Linux/iOS overrides, capabilities | Delete after pure Web gate |
| Desktop packages | Tauri API, plugins and CLI in `package.json` | Freeze now; remove in final migration stage |
| npm scripts | `tauri*`, `pretauri*`, `build:appimage` | Freeze now; remove after Web parity |
| Rust packaging | `src-tauri/build.rs`, Cargo Tauri dependencies | Extract core first, then delete |
| Desktop assets | ICO, ICNS, PNG, tray, Windows Store assets | Delete after release migration |
| Mobile assets | Android icons, iOS icons and generated Apple project | Delete; no V1 mobile client |
| Release workflow | `.github/workflows/release.yml` | Replace desktop bundles/signing with Web/Server/Runner images |
| Mobile scripts | iOS build/run and TestFlight scripts | Delete |
| Desktop documentation | Local workspace, tray, updater, mobile and installer instructions | Rewrite for server deployment |

## Migration Worklist

### Execution Order

1. [x] Implement native Profile Host app-server stdio transport, single-owner
   lock, request correlation, bounded event queue and Manifest handshake in
   platform crates. Replace the daemon-backed `RealCodexAdapter`. A durable
   multi-Profile registry and database lease remain part of the next platform
   slice.
2. Move Provider CRUD/Secret orchestration and model refresh from
   `src-tauri/src/shared/codex_core.rs` into the Profile Host/provider service.
   Keep Chat wire translation, Provider model semantics and cache isolation in
   the retained Codex Runtime seams.
3. Extract Repository/Worktree/Diff/Commit/Push behavior into `git-runtime` and
   route Task/Run APIs through authorized platform DTOs.
4. Replace `src/services/tauri.ts` and `src/services/events.ts` consumers with a
   typed `PlatformClient` and authenticated WebSocket Event Store with cursor
   replay. Tauri adapters call the same services until Web parity is proven.
5. Delete dictation, native window/menu/tray/updater, local-path operations,
   interactive PTY, Tailscale client management, mobile projects and desktop
   release pipelines after the Web Beta stability gate.

### Extract And Reuse

- App-server process transport, request correlation, event parsing and protocol DTOs.
- Agent configuration validation and native Codex configuration operations.
- Git repository, Worktree, Diff, Commit and Push core logic.
- Safe path normalization, process-tree termination and bounded output helpers.
- Existing React message, plan, tool, Diff and Composer presentation components after transport removal.

### Replace With Web/Server Modules

- `src/services/tauri.ts` -> typed `PlatformClient` resources and commands.
- `src/services/events.ts` -> authenticated WebSocket Event Store with replay cursor.
- Local Workspace CRUD -> Project, Repository, Task, Run and Workspace APIs.
- Shared local Codex process -> Profile-scoped Codex Host Session Registry.
- Native dialogs -> Web forms, Drawer, Dialog, upload and download flows.
- Native menus and shortcuts -> App Shell menus and command palette.
- Native notifications -> in-app notifications and optional Web Push.
- Tauri focus events -> browser focus and Page Visibility events.
- Local paths and `convertFileSrc` -> authorized file and Artifact URLs.

### Delete From V1

- Window chrome, dragging, liquid-glass and native caption controls.
- Tray integration and native application menus.
- Desktop updater, relaunch and installer UI.
- Finder/Explorer reveal, open in local editor and arbitrary local path registration.
- Dictation, microphone permission, Whisper model download and `whisper-rs`.
- Interactive terminal UI and PTY commands.
- Tailscale daemon management in the user client.
- iOS/mobile runtime, generated Apple project and mobile setup wizard.
- AppImage, MSI/NSIS, DMG, TestFlight and desktop signing/release outputs.

### Deferred Beyond V1

- Browser interactive shell with a dedicated high-risk protocol and authorization review.
- Web Push if in-app notifications satisfy the initial deployment.
- GitHub App/PR automation beyond the V1 Git provider decision.
- Multi-node Runner scheduling beyond the initial capacity threshold.

## Migration Guardrails

- New frontend `@tauri-apps/*` imports are forbidden outside the frozen compatibility boundary.
- New `#[tauri::command]` registrations are forbidden.
- New desktop/mobile packaging files and scripts are forbidden.
- Existing Tauri references may only decrease or move into an explicitly approved compatibility adapter.
- Codex Runtime gaps must be implemented in the Codex Rust project, not hidden inside a Tauri or Web compatibility path.
