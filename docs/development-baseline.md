# CodexMonitor V1 Development Baseline

## Snapshot

| Field | Value |
| --- | --- |
| Captured at | 2026-07-12, Asia/Shanghai |
| Git branch | `codex/web` |
| Git commit | `c8996a46b27098bdbf854eb95ca9f02e6fc3573c` |
| Repository package | `codex-monitor@0.7.68` |
| Operating system | Windows NT `10.0.26200.0`, x64 |
| PowerShell | `7.6.3` |
| Node.js | `v24.14.1` |
| npm | `11.11.0` |
| pnpm | `10.33.0` |
| Rust compiler | `rustc 1.95.0 (59807616e 2026-04-14)` |
| Cargo | `cargo 1.95.0 (f2d3ce0bd 2026-03-21)` |
| Codex package | `OpenAI.Codex 26.707.3748.0` |
| PostgreSQL CLI | Not installed |

## Frontend Versions

Versions are resolved from `package-lock.json` lockfile version 3.

| Dependency | Version |
| --- | --- |
| React | `19.2.3` |
| Vite | `7.3.1` |
| TypeScript | `5.8.3` |
| Tauri CLI | `2.10.1` |

## Rust Application Versions

Versions are declared by `src-tauri/Cargo.toml`.

| Dependency | Version requirement |
| --- | --- |
| Rust edition | `2021` |
| Tauri | `2.10.3` |
| Tauri Build | `2.5.6` |
| Tokio | `1` |
| git2 | `0.20.3` |

## Baseline Notes

- The Codex executable resolves to the installed Windows App package at `C:\Program Files\WindowsApps\OpenAI.Codex_26.707.3748.0_x64__2p2nqsd0c76g0\app\resources\codex.exe`.
- The current shell cannot execute that packaged binary directly because Windows denies access. The package version is recorded above; the exact app-server protocol/build version must be captured by the Stage 0 contract harness once it can initialize the server.
- PostgreSQL client tools are not installed on this workstation. PostgreSQL integration tasks must provide a containerized or otherwise reproducible test dependency before database baselines can run.
- At capture time, the PRD and development plan were untracked files. No unrelated tracked worktree changes were present.

## Frontend Verification Baseline

Dependencies were installed with `npm ci` from `package-lock.json` before verification.

| Check | Result | Baseline detail |
| --- | --- | --- |
| `npm ci` | Passed | 541 packages installed; npm reported 14 vulnerabilities: 3 low, 4 moderate, 6 high, 1 critical |
| `npm run typecheck` | Passed | `tsc --noEmit` completed without errors |
| `npm run lint` | Passed with warnings | 0 errors and 5 `react-hooks/exhaustive-deps` warnings |
| `npm test -- --run` | Failed | 999 tests: 993 passed, 6 failed, 0 pending; 294 of 298 suites passed |
| `npm run build` | Passed with warnings | Vite transformed 3,249 modules and produced `dist`; large chunk and mixed static/dynamic Tauri opener imports were reported |

### Existing Lint Warnings

- `src/features/app/hooks/useTrayRecentThreads.ts`: one missing `useMemo` dependency.
- `src/features/composer/components/ComposerInput.tsx`: three missing `useCallback` dependencies.
- `src/features/threads/hooks/useThreadTurnEvents.ts`: one missing `useCallback` dependency.

### Existing Test Failures

- `src/features/app/hooks/useTraySessionUsage.test.tsx`: five failures. The Windows host uses a Chinese locale, so formatted reset labels such as `2小时后` and `后天` do not match hard-coded English expectations.
- `src/features/home/components/Home.test.tsx`: one failure because the rendered date label does not contain the hard-coded `Jan 20` expectation under the current locale/time formatting.

These failures are the pre-migration baseline. Later changes must not add failures; locale-sensitive tests should be made deterministic in a dedicated fix task before a zero-failure CI gate is enforced.

## Rust Verification Baseline

| Check | Result | Baseline detail |
| --- | --- | --- |
| `cargo check` | Failed | Default `app-runtime` build fails in `whisper-rs 0.12.0` with 72 `E0609` errors against generated `whisper_full_params` bindings |
| `cargo check --no-default-features` | Passed with warnings | Core, daemon and Tauri stub paths compile; existing unused import, variable and dead-code warnings remain |
| `cargo test --no-default-features` | Passed with warnings | 389 tests passed across library, binaries and Tauri configuration; 0 failed |
| Real `codex app-server` smoke | Blocked | The only discovered Codex binary is inside the Windows App package and process creation is denied by the package ACL |

### Rust Test Breakdown

- Library tests: 204 passed.
- Main binary tests: 0 tests.
- Daemon tests: 156 passed.
- Daemon control tests: 28 passed.
- Tauri configuration integration tests: 1 passed.
- Doc tests: 0 tests.

The non-default-feature path is the usable core baseline for extraction work. The default desktop speech feature and the app-server executable access problem are separate baseline blockers and must not be attributed to future Web changes.
