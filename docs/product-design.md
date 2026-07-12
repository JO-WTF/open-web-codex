# Product design

## Product

`open-web-codex` is a self-hosted Web workbench for trusted software teams. A
user opens a project in a browser, creates a coding Task, observes Codex and its
subagents, handles approvals, reviews changes, and commits or pushes the result.
No desktop client, local browser extension, or user-side Codex process is
required.

The product is a harness around Codex, not a replacement Agent runtime.

## V1 users

| Role | Primary responsibility |
| --- | --- |
| Organization owner | Members, security defaults and platform health |
| Project administrator | Repository, project members and execution policy |
| Developer | Create and control Tasks, review and deliver changes |
| Reviewer | Observe Tasks and resolve authorized approvals |
| Platform administrator | Profiles, Runners, capacity and recovery |

V1 supports one organization per deployment. The schema retains
`organization_id`, but public signup, billing and public multi-tenant SaaS are
out of scope.

## Core objects

- **Project:** an authorized Git repository and its policy.
- **Task:** the long-lived user-visible objective and stable Codex Thread mapping.
- **Run:** one schedulable execution attempt for a Task.
- **Profile:** a user's persistent Codex identity, `CODEX_HOME`, Provider,
  configuration, Threads, memory and integrations.
- **Workspace:** one Run's isolated Git worktree.
- **Control lease:** the temporary right to steer or stop a running Task.
- **Approval:** a durable platform record of a Codex server request and decision.

## Primary workflow

1. An invited user signs in and obtains project membership.
2. An administrator imports a Git repository and verifies credentials.
3. The user creates a Task, chooses a branch, Profile, Provider/model and policy.
4. The platform creates a queued Run and an isolated worktree.
5. Profile Host starts or reuses the user's app-server and creates/resumes the
   Codex Thread against that worktree.
6. The browser receives ordered activity through an authenticated WebSocket.
7. Codex approvals and structured input requests become durable platform
   approvals before a decision is returned to app-server.
8. The user reviews the resulting diff and explicitly commits or pushes it.
9. The Run reaches a terminal state; retention policy later removes the worktree
   without deleting the Profile or Thread.

## Product modules

### Task workbench

- Thread activity, plans, tool calls, command output and subagent trajectory.
- Queue, provisioning, running, waiting, interrupted and terminal states.
- Composer with send, steer, queue, cancel and resume semantics.
- Durable approvals, reconnect replay and clear controller ownership.
- Diff, files, test artifacts, commit and push.

### Codex Studio

Studio modules are independently capability-gated:

1. Profiles and identity health.
2. Model Providers and provider-scoped model catalogs.
3. MCP servers, OAuth, elicitation, tools and resources.
4. Plugins and marketplaces.
5. Memory health and controlled reset/export.
6. Native Agents and multi-agent configuration.
7. Personal and project Skills.

An unavailable runtime capability is shown as unavailable with a remediation;
the platform does not implement a fallback runtime.

## Release scope

### Alpha

Single administrator deployment with repository import, one persistent Profile,
provider/model selection, Task/Run, worktree, Thread/Turn, realtime activity,
approval, cancel/resume, diff and commit. Alpha proves the vertical execution
loop and recovery after browser refresh and Host restart.

### Beta

Invitation-only multi-user operation with RBAC, per-user Profiles, control
leases, durable approvals/events, audit, rootless execution isolation and push.
Provider management and the MCP read/OAuth/elicitation path are included.

### V1 GA

Adds production backup/restore, capacity and security validation, compatible
Codex upgrade/rollback, and the Studio capabilities that pass their individual
gates. Tauri is removed only after the Web product has completed a stable Beta
release cycle.

## Explicit non-goals

- A new Agent planner, scheduler, memory engine, Skill interpreter, Plugin
  runtime or MCP runtime.
- Browser access to a user's local repositories or arbitrary server paths.
- A general online IDE or unrestricted interactive shell.
- Automatic commit, force push, merge or remote branch deletion.
- Billing, public signup, anonymous access, or cross-region active-active V1.
- Compatibility with arbitrary unpinned Codex binaries.

## V1 acceptance

- Two users can run separate Tasks without Profile, event, secret or workspace
  crossover.
- Browser refresh, network reconnect and service restart recover an explicit
  Task/Run state and the corresponding Codex Thread.
- Every writable Run has its own worktree and all high-risk decisions are
  auditable.
- Provider selection and model discovery are Profile-scoped and survive restart.
- Multi-agent relationships visible in Codex are rendered without a platform
  scheduler.
- Missing capabilities remain disabled and cannot silently fall back.
- A pinned Codex build passes generated schema, manifest, fixture and real smoke
  checks before deployment.
- The production user flow requires only a browser.
