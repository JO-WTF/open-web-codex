# Supply Chain Network Planner

This Codex Plugin provides reviewed, read-only supply-chain MCP capabilities. Domain
calculations and Resource publication stay in this package; Codex Runtime owns Tool,
Skill, MCP and Agent execution.

## MCP boundaries

The package declares three independent MCP Servers:

| Server | Current owner and use |
| --- | --- |
| `supply_chain_data` | General bounded planning-source discovery and `planning-dataset.v2` publication |
| `supply_chain_planner` | General snapshot, route, scenario, facility, finance and risk Resources |
| `supply_chain_indonesia` | Exact Workspace Dataset Release access and the progressive Indonesia tutorials |

The current `enterprise-supervisor-copilot@3.10.0` uses only
`supply_chain_indonesia` plus the separate `map_utils` Plugin. The older general
planning Servers remain independently tested capabilities; they are not hidden fallbacks for
the Indonesia workflow.

## Indonesia tools

`supply_chain_indonesia` exposes:

- `inspect_indonesia_dataset_release`
- `evaluate_indonesia_service_baseline`
- `evaluate_indonesia_current_network`
- `evaluate_indonesia_candidate`
- `optimize_indonesia_new_warehouse`
- `prepare_indonesia_network_map`
- `prepare_indonesia_map_render`
- `prepare_indonesia_decision_report`
- `validate_indonesia_resource`

The service-baseline Tool intentionally returns current forward-to-customer service and
province metrics without linehaul, capacity or cost fields. The current-network Tool adds
the full two-level cost and capacity view. Candidate and optimization Tools use only the
twenty reviewed candidates.

Every producer validates its typed Resource and cross-field totals before publication.
Validation failure is a failed Tool call; correctness does not depend on the model choosing a
second validator call. Same-server analysis Tools consume exact `resource_name` values and let
the server resolve its own Resource Store; models do not copy or reconstruct internal URIs.
Resource-producing Tools still return an unchanged structured `data_ref` for durable provenance
and bounded direct reads. The decision-report Tool cross-checks the seven Resource identities it
owns and deterministically renders the business-report Markdown. The separate `map_utils` Tool
owns the browser Artifact and exact embed code; the Supervisor may only append that unchanged
code to the unchanged report.

## Current governed roles

| Release | Exact capability boundary |
| --- | --- |
| `enterprise-data-agent@3.1.0` | Atomic Indonesia Dataset inspection and bounded Resource reading |
| `enterprise-network-planning-agent@3.4.0` | Server-resolved Indonesia service, validated province rankings, current, candidate, optimization, bounded map preparation and deterministic business-report publication |
| `enterprise-visualization-agent@1.3.0` | Typed map-render preparation and exact matching Artifact-ID/embed-code handoff from `map_utils.create_map_card` |
| `enterprise-supervisor-copilot@3.10.0` | Dynamic coordination and constrained composition of the unchanged business report with the independently owned inline map |

The root Supervisor receives collaboration capabilities but no business MCP or shell. Child
roles receive only their exact MCP Server, Tool and capability-root inventory. Disabled sibling
Servers are not treated as missing dependencies.

Web-authored Agent Releases inherit one reviewed template and may narrow Artifact contracts.
They cannot add Tools through instructions. A Dataset-dependent Agent binds exact Workspace,
Release, Dataset, version and SHA-256 identities; Runtime receives no browser-supplied host path.

## Indonesia data

The reproducible release is under:

```text
examples/indonesia-tutorial/releases/1.0.0/
```

It contains 240,000 synthetic customers, 38 current provinces, three central warehouses,
eight forward warehouses, twenty reviewed candidates and 1,148 quote rows. The source lock,
generation policy and validation report are checked in beside the release.

The authoritative synthetic-data rules are in
[`references/indonesia-tutorial-data-contract.md`](references/indonesia-tutorial-data-contract.md).
The Web learning path begins at
[`docs/tutorials/README.md`](../../docs/tutorials/README.md).

## Resource and approval behavior

The Indonesia Server reads one exact platform-authorized Dataset Release through trusted Codex
Turn metadata. It never accepts a host path as a Tool argument, scans a Workspace, or returns raw
customer rows. Bounded JSON and GeoJSON Resources live in Profile-scoped MCP state.

The checked-in MCP declarations classify the complete current Server toolsets as reviewed,
read-only, deterministic and idempotent, so their default approval mode is `approve`. This is not
a global approval bypass. Commands, file writes, credentials, external effects and future
higher-risk Servers remain subject to their own Runtime and platform contracts.

## Development setup and validation

The real-mode startup scripts provision the shared MCP environment. For manual setup:

```bash
./bin/setup-env
```

Run deterministic tests with the project test Python:

```bash
PYTHONPATH=tools/supply-chain-network-planner \
  python3 -m pytest tools/supply-chain-network-planner/tests -q
```

Run the real stdio protocol smoke with the MCP environment:

```bash
PYTHONPATH=tools/supply-chain-network-planner \
  .local/open-web-codex/tool-envs/supply-chain-network-planner/bin/python \
  tools/supply-chain-network-planner/tests/stdio_smoke.py
```

The real Web/Runtime path is covered by:

```bash
scripts/smoke-enterprise-supervisor-copilot.sh
```

No Provider or navigation API key is owned or stored by this Plugin.
