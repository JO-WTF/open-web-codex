# Product policy contracts

Hand-maintained Web product policy lives here. These files describe what the
browser may expose when a capability is negotiated; they do not claim that a
particular Codex build supports a method.

| File | Purpose |
| --- | --- |
| `capability-ids.v1.json` | Stable capability ID registry and ownership |
| `compatibility-matrix.json` | Web/contract/server build compatibility |
| `feature-policy.v1.json` | UI feature to capability ID mapping |

Runtime-generated artifacts belong under `../generated/` and are published as
the signed contract bundle described by `../contract-bundle.schema.json`.
