# ADR 0002: Persistent Codex Profile

## Status

Accepted on 2026-07-12.

## Context

Codex identity, configuration, native Agents, personal Skills, Plugins, MCP configuration, Threads and Memory are scoped by Codex Home. Per-Run Codex Homes would lose continuity, duplicate credentials and make Memory unusable. A machine-global Home would mix users and prevent correct audit attribution.

## Decision

The isolation and persistence unit is a Codex Profile.

- Each member receives one default personal Profile.
- Each Profile has one persistent, private Codex Home.
- Different Profiles never share Home, credentials, Thread indexes or Memory.
- One Profile has at most one primary app-server process at a time.
- The primary process may register multiple authorized Workspaces and host multiple native Threads.
- A Task/Run records immutable Profile ID and Codex Thread ID mappings.
- Restart uses the same Profile Home and rebuilds Workspace registrations before recovery.
- A service Profile is a separate explicitly managed identity; V1 does not share mutable personal Profiles between team members.

## Single-Primary Coordination

The Host combines:

1. A database lease identifying Profile, Host node, process identity and expiry.
2. A filesystem/process lock inside the Profile runtime directory.
3. An in-process Session Registry keyed by Profile ID.

A start request is idempotent. Concurrent starts either join the initializing Session or receive the same terminal failure. A Host may replace an expired lease only after proving the old process is absent or fencing it from new requests.

## Workspace Registration

- Registration requires Profile, Project, Task and Workspace authorization.
- Registration stores the native Workspace identifier and current Session generation.
- Removing a Workspace does not delete the Profile, native Agent configuration, other Threads or Memory.
- A Thread is never routed by CWD alone when a persisted mapping exists.
- Global account notifications are scoped to the owning Profile, not broadcast across users.

## Lifecycle Operations

Health check, restart, identity logout, Memory reset, export and Profile deletion are independent operations. Each has separate permission, active-Turn checks, confirmation text, audit event and failure recovery.

## Consequences

- A user retains native Codex continuity across Tasks and Host restarts.
- Profile app-server capacity is a native runtime constraint and must be measured before adding transparent process sharding.
- Backup and rollback procedures must preserve Profile Home compatibility.
- Profile deletion is a high-risk data operation and cannot be implemented as ordinary Workspace cleanup.

## Rejected Alternatives

- One Codex Home and app-server per Run.
- One machine-global app-server for the organization.
- Multiple uncoordinated app-server processes writing the same Profile Home.
- Team Skills distributed by sharing a personal Profile; project `.agents/skills` is used instead.
