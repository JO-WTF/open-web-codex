# ADR 0004: Lifecycle State Machines

## Status

Accepted on 2026-07-12.

## Context

Browser disconnects, Server restarts, Host failures and Runner failures must not produce fabricated completion or indefinite active states. Profile, Workspace, Run, Approval and Control Lease have different ownership and cannot share one generic status.

## Decision

### Profile

```text
creating -> starting -> ready
                    -> auth_required
                    -> degraded
                    -> incompatible
                    -> stopped
```

- `ready` accepts new Runs.
- `degraded` accepts only operations allowed by Capability and policy.
- `auth_required`, `incompatible` and `stopped` reject new Runs.
- An unexpected process exit enters `starting` during bounded recovery, then `ready`, `degraded` or `stopped` with a reason.
- Deleting is a separate operation state and is forbidden while active Turns exist unless an audited force policy applies.

### Workspace

```text
provisioning -> ready -> registered -> in_use -> releasing -> released
       |          |          |           |            |
       +----------+----------+-----------+----------> error
```

- `error` is recoverable only through an idempotent retry/cleanup operation.
- Registration belongs to a Profile Session generation.
- `released` is terminal for that Workspace ID.

### Run

```text
queued -> provisioning -> running
                         -> waiting_approval -> running
                         -> waiting_input -> running
                         -> completed
                         -> failed
                         -> interrupted
                         -> cancelled
```

- `completed`, `failed`, `interrupted` and `cancelled` are terminal.
- Unknown execution outcome becomes `interrupted`, never `completed`.
- Continue creates a new Run and preserves the old terminal record.
- Browser connectivity changes do not change Run state.
- Turn interruption and Run cancellation are persisted requests; UI does not show the final state before server confirmation.

### Approval

```text
pending -> approved | rejected | expired | cancelled
```

- The first valid terminal decision wins under a database uniqueness/compare-and-set rule.
- A disconnected Runner/Profile freezes delivery but does not invent a rejection.
- Approval request ID is valid only for its Profile Session generation and Run.

### Control Lease

```text
available -> held -> transferring -> held
                  -> expired -> available
                  -> revoked -> available
```

- At most one active lease exists per Task.
- Lease ownership controls Agent messages and Run controls, not Approval, Commit or Push permissions.
- User drafts remain private during transfer.

## Recovery Rules

1. Reconcile database lease and actual process identity.
2. Rebuild Profile Session and Workspace registration.
3. Query native Thread/Turn state when supported.
4. Replay platform snapshot plus events for the browser projection.
5. Mark uncertain active Runs `interrupted` with a classified reason.
6. Never reuse expired request IDs or silently mutate a terminal Run.

## Consequences

- Every state change requires timestamp, reason and actor/system source.
- State transition functions are server-side and unit tested.
- UI states are projections; clients may show “request sent” but cannot predict terminal results.
- Lease and recovery jobs are required before multi-node operation.

## Rejected Alternatives

- Boolean `running` or `connected` flags shared across resources.
- Client-owned state transitions.
- Automatically marking stale Runs completed.
- Reusing old Approval IDs after app-server restart.
