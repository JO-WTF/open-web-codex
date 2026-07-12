# Capability baseline

## Snapshot

Captured on 2026-07-12 from the merged source snapshots:

| Component | Branch | Commit |
| --- | --- | --- |
| CodexMonitor | `codex/web` | `9839d5fedc1f1488b0e1edec338cdd1090d5eefb` |
| Customized Codex | `open-codex` | `0de018a81223aacb6306b4f19ef7a54ee3bfcf8a` |
| Common Codex upstream base | `openai/codex` | `f959e7fc9832dfa0ebfb6542ab1bbf829638ac24` |
| Upstream observed head | `openai/codex/main` | `9e552e9d15ba52bed7077d5357f3e18e330f8f38` |

The customized Codex snapshot is 38 custom commits above the common base and
the observed official branch is 336 commits beyond that base. This is a tracked
sync backlog, not evidence that all 336 commits affect this product.

## Verification evidence

- CodexMonitor contract validation, manifest parser tests, fixture replay and
  harness self-tests pass.
- The locally built customized `codex app-server` completes the real initialize
  handshake.
- Its initialize result contains `codexHome`, `platformFamily`, `platformOs` and
  `userAgent`.
- Requiring `capabilityManifest` fails because the server does not emit one.
- Codex already generates build-specific JSON Schema and TypeScript definitions.

The existing Web fixture is a desired contract test fixture. It is not an
observed manifest and must not be used as runtime truth.

## Source-level capability assessment

| Capability | State | Evidence and remaining gap |
| --- | --- | --- |
| Protocol schema | available | JSON/TypeScript generators and checked-in schema fixtures exist |
| Capability/version negotiation | missing | initialize has client opt-ins but no server manifest or compatible range |
| Thread lifecycle | available | start, read, resume, list, archive, search, turns/items and recovery surfaces exist |
| Turn lifecycle | available | start, steer, interrupt, review, plan, diff and item lifecycle exist |
| Durable Web approvals | runtime available, platform missing | command, file, permission, user-input and elicitation server requests exist |
| Profile multi-workspace | needs smoke validation | multiple cwd-bound Threads exist; explicit runtime workspace registry is not proven necessary |
| Memory lifecycle | partial | compaction, compacted notification, memory mode and reset exist; status/export/consolidation health are missing |
| Native Agent CRUD | missing | config/files exist but no stable dedicated Agent CRUD/validation contract |
| Multi-agent trajectory | experimental, substantial | parent Thread, role/nickname, collab tool items and multi-agent mode exist |
| Skills | partial | list, changed, config write, extra roots and filesystem APIs exist; safe CRUD/validation/test semantics are incomplete |
| Plugins | partial/under development | marketplace, list, read, install and uninstall exist; stable update/enable/permission-diff semantics remain |
| MCP and tools | partial, broadly usable | status, inventory, resource read, tool call, reload, OAuth and elicitation exist; secure config CRUD/error policy remains |
| Custom Providers | available on custom branch | add/edit/delete/select, provider-scoped model discovery, Responses/Chat wire APIs and context windows exist |
| Web platform | prototype | HTTP RPC/SSE preview exists; users, RBAC, durable data, WebSocket replay and production auth do not |

## Immediate contract work

1. Generate a Capability Manifest from the actual Codex method registry and
   experimental annotations.
2. Add build identity, protocol range, limits and structured error categories.
3. Add Provider/model-discovery capability IDs.
4. Generate the bundle in Codex CI and consume it by digest in Web CI.
5. Replace the desired Web fixture with generated fixtures plus a separate
   product feature-policy fixture.
6. Run real Profile restart, multi-cwd, multi-agent, Provider, MCP and approval
   smoke tests before declaring those capabilities supported.
