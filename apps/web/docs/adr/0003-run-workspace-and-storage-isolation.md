# ADR 0003: Run Workspace And Storage Isolation

## Status

Accepted on 2026-07-12.

## Context

The browser cannot safely register arbitrary server paths. Concurrent Agent Runs must not modify the same checkout, inherit unrelated credentials or retain access after cleanup. Profile continuity must remain separate from code execution isolation.

## Decision

Repository, Profile and Run storage use different ownership and lifecycle roots. The production layout is rooted under a configured absolute `DATA_ROOT`; IDs are validated opaque identifiers, never raw user path fragments.

```text
DATA_ROOT/
  profiles/<profile-id>/home/             persistent, private Codex Home
  profiles/<profile-id>/runtime/          locks, sockets and transient process metadata
  repositories/<repository-id>/mirror.git read-mostly Git mirror
  runs/<run-id>/workspace/                isolated writable Git Worktree
  runs/<run-id>/runtime/                  container/process temporary data
  artifacts/<organization-id>/<run-id>/   bounded outputs and reports
```

### Repository And Worktree

- Projects are created from authorized Git URLs and Credential references.
- Repository mirrors are updated by a dedicated Git operation; Runs do not write them.
- Every Run creates a new Worktree inside its Task boundary.
- A recovery Run may create a successor Worktree; it does not silently reuse an uncertain writable directory.
- No writable Worktree is shared by different Tasks or concurrent Runs.
- Commit and Push are explicit authorized operations; Force Push is disabled in V1.

### Container And Process

- Beta/GA Runs execute in a rootless container or equivalent sandbox.
- CPU, memory, process, disk, duration and outbound-network policy are explicit.
- Only the Run Worktree and approved temporary paths are writable.
- Docker socket, Profile Home and other Run directories are not mounted into the execution container unless a reviewed Codex Host protocol requires a narrower mediated operation.
- Turn cancellation terminates the target process tree without killing the Profile app-server or other Runs.

### Secret Boundary

- Database rows store encrypted Secret references and metadata, not plaintext.
- Secret values are resolved server-side for the minimum operation lifetime.
- Git, Codex and MCP credentials use separate scopes.
- Secret values never enter browser responses, URLs, ordinary events, logs, analytics or Artifact metadata.
- Cleanup checks that temporary Secret material and process environment are no longer accessible.

### Path Boundary

- Canonicalization occurs before authorization.
- Path traversal, absolute user paths, alternate Windows namespaces, unsafe symlinks and repository escape are rejected.
- File APIs operate on resource ID plus repository-relative path.
- Artifact downloads use authorized IDs and bounded response headers, not filesystem paths.

## Consequences

- Profile persistence does not weaken code isolation.
- Workspace cleanup can run independently of Profile retention.
- Disk capacity and cleanup failure become production metrics and alerts.
- Interactive shell access remains out of V1 because it needs a separate high-risk protocol.

## Rejected Alternatives

- Browser-provided local or server filesystem paths.
- Shared mutable checkout per Project.
- Mounting Profile Home into every Run container.
- Passing long-lived credentials in environment variables without lifecycle tracking.
