# Codex Integration Contract

## Artifacts

| Artifact | Purpose |
| --- | --- |
| `policy/capability-ids.v1.json` | Stable V1 capability ID registry and ownership |
| `policy/compatibility-matrix.json` | Explicit Web/contract/server build compatibility and release order |
| `policy/feature-policy.v1.json` | Browser feature to capability ID mapping |
| `generated/contract-bundle.v1.json` | Signed-build contract bundle emitted from a Codex build |
| `capability-manifest.schema.json` | Machine-readable app-server Capability Manifest v1 |
| `fixtures/capability-manifest.v1.json` | Valid mixed-state Manifest used by Web contract tests |

## Negotiation Order

1. Parse JSON and validate the Manifest schema major.
2. Verify that the app-server protocol version intersects the Web-supported range.
3. Verify the server build against the deployment compatibility matrix.
4. Resolve every feature from a capability ID and status.
5. Apply declared limits before enabling inputs or scheduling Runs.
6. Treat missing and unknown capabilities as disabled; never probe an undeclared RPC optimistically.

## Capability Status

| Status | Meaning | Web behavior |
| --- | --- | --- |
| `supported` | The declared version, methods and limits are available | Enable only operations listed in `methods` |
| `unsupported` | The server intentionally does not provide the capability | Disable the module/action and show remediation when provided |
| `degraded` | A usable subset is available | Enable only declared methods and show the limitation |
| `incompatible` | The capability exists but cannot be consumed by this Web build | Block dependent actions and require a compatible version |
| `experimental` | Available without a stable compatibility guarantee | Keep disabled unless deployment policy explicitly enables it |

`unsupported`, `degraded` and `incompatible` require a structured `reason`. `experimental` requires `experimental: true`.

## Version Rules

- `schemaVersion` uses SemVer. A major change requires a new schema file and explicit Web support.
- `server.protocolVersion` identifies the app-server protocol contract.
- Every capability has an independent SemVer `version`.
- Adding an optional field or new capability ID is backward compatible.
- Removing or renaming a capability ID, method, required field, status or limit changes behavior and is breaking.
- Narrowing a numeric limit is operationally breaking for scheduling and must be called out in release metadata.
- Unknown fields are allowed for forward compatibility; consumers must ignore them unless a newer schema major is required.

## Request, Response And Fixture Rules

- Upstream protocol Schema files use the app-server method as identity; generated filenames replace `/` with `.` and append `.request`, `.response`, `.notification` or `.server-request`.
- A request and its response share the original JSON-RPC string or numeric ID without type conversion.
- A server request must receive exactly one response or an explicit protocol cancellation/expiry result.
- Notifications never receive a response and must be replay-safe or explicitly marked non-replayable in metadata.
- Every replay Fixture declares `fixtureVersion`, `protocolVersion`, `capabilityId`, `scenario`, server build identity and ordered messages.
- Fixture scenarios use stable lowercase names such as `success`, `permission_denied`, `timeout`, `unsupported`, `partial_failure` and `recovery`.
- Removing or changing an existing success/failure Fixture is breaking unless the capability major version changes.
- Fixtures contain deterministic IDs, timestamps and paths. They must not contain live credentials, user Prompt text or proprietary repository content.

## Error Rules

- Host/Web errors use `error.schema.json`; raw app-server errors may be retained under `cause` for authorized diagnostics.
- Error `code` and `category` are stable API fields. User-facing copy is not a protocol field and may be localized.
- `retryable` describes whether retrying the same operation can succeed without user/configuration changes.
- Compatibility errors identify `capabilityId` and required/observed versions in `details`.
- Authentication and authorization errors are distinct; the UI must not convert either into a generic runtime failure.
- Unknown upstream errors map to `codex.runtime.unknown` with `retryable: false` until classified.

## Compatibility Matrix

- A server build is schedulable only when it appears in `compatibility-matrix.json` with status `compatible` and its runtime Manifest also passes negotiation.
- `unverified`, `blocked` and `retired` builds cannot accept new Runs.
- Contract artifacts are published before Web code that consumes them.
- A rollback must preserve the existing Profile Home and use the last compatible server build; destructive Profile migration requires a separate migration contract.

## Method Declaration

Each capability declares only the protocol surfaces it owns:

- `clientRequests`: Web/Host requests sent to app-server.
- `serverRequests`: app-server requests requiring a Host/user response.
- `notifications`: app-server notifications consumed by Host/Web.

A method appearing in the global protocol but absent from a capability declaration is unavailable for that capability. Synthetic platform events are not app-server methods and must not be listed here.

## Limit Declaration

Limits are typed values with stable keys. Initial reserved keys are:

- `maxWorkspacesPerProfile`
- `maxConcurrentThreads`
- `maxAgentThreads`
- `maxAgentDepth`
- `maxSkillFiles`
- `maxSkillBytes`
- `maxPluginPackageBytes`
- `maxMcpServers`

Missing limits mean “not declared”, not “unlimited”. The Host must apply deployment policy and refuse scheduling when it cannot determine a safe bound.

## Ownership

- The Codex Rust project emits the Manifest and owns native capability semantics.
- CodexMonitor validates, snapshots and gates UI/API behavior from the Manifest.
- Neither project may redefine an existing capability ID with a different meaning without a major contract version.

## Cross-Project Bundle

- The Rust release publishes one JSON contract bundle matching `contract-bundle.schema.json`.
- CI supplies the bundle URL/local path and expected SHA-256 out of band.
- Remote downloads require HTTPS and do not follow redirects.
- The bundle and every decoded JSON file have independent SHA-256 checks.
- Bundle paths are restricted to `manifest/`, `schemas/` and `fixtures/`; traversal and absolute paths are rejected.
- Verified bundles are cached by content hash and populated atomically.
