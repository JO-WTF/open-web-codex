# Supply-chain planning contract

## Resource and Artifact lifecycle

Every calculation starts from immutable MCP Resources:

- `planning-dataset.v2`: typed market and source scope, city-period demand, warehouse-to-city
  coverage, combined city-lane facts, delivery baseline, data quality, reviewed candidates,
  and exact `network_input`.
- `network_snapshot.v1`: planning period, currency, service policy, demand points,
  facilities, and rate rules.
- `route_matrix.v1`: one provider/method and typed origin-city to destination-city
  route rows tied to exactly one snapshot. Multiple demand points in one city reuse
  the same lane.
- `network_scenario_result.v1`: allocations, issues, and metrics for one explicit
  facility set and calculation mode.
- `scenario_comparison.v1`: deltas between compatible scenario results.
- `facility_location_solution.v1`: target, selected candidates, exact-solver scope,
  assumptions, and its scenario-result reference.
- `financial_evaluation.v1`: compatible network inputs, opening investment, recurring
  savings, NPV, payback, and explicit financial assumptions.
- `risk_register.v1`: scored material risks, mitigations, triggers, and exact planning
  evidence references.

Copy each returned `data_ref` unchanged. It uses server `supply_chain_planner` and an
opaque `supply-chain://resources/...` URI. Data Agent handoffs use server
`supply_chain_data` and `supply-chain-data://resources/...`. Publishing is
content-addressed; changing source facts or assumptions creates a new Resource.
Resource files default to the owning Profile's `CODEX_HOME`; they are not shared
application or repository state.

For the Data Agent intake path, `publish_mapping_proposal` accepts only the
unchanged `source_profile.v1` `data_ref` as `source_profile_ref`. The Data MCP
loads and validates that Resource from its own store before proposing fields.
The source Profile must retain each source's nested `structure`; a flattened
copy is invalid, and an empty mapping candidate result is a failed Tool call.
Workspace `source_ref` values are file-inspection references and must not be
passed to the MCP Resource reader.

Every Resource-producing Tool also returns `resource_name` in its structured result.
Use that exact stable name when citing evidence; never expose or relabel the opaque
Resource URI as a human-readable name. `data_ref` is the Runtime handoff identity,
whereas `resource_name` is the report citation identity.

The Resource is the Runtime handoff. When a completed MCP Tool result contains a
supported Resource link, the Platform registers the same content as a Task-owned
Artifact:

1. the Artifact receives an independent ID and Task read grant;
2. Run, Thread, Turn and Item IDs are recorded only as producer provenance;
3. content is materialized through the official MCP Resource read path;
4. JSON syntax, MIME type, immutable source metadata and size are checked, while the
   enterprise E2E also asserts that content `schema_version` matches the Artifact
   schema;
5. browser DTOs expose the Artifact identity and authorized content URL, not the
   internal MCP Resource URI.

Artifact registration does not change the calculation contract. A child Agent still
passes the original `data_ref` unchanged inside Runtime; the Platform Artifact provides
durable product identity, authorization and recovery. Deleting a producing Run must not
delete the Artifact, its Task grant or provenance identity.

Current lifecycle states are:

```text
pending -> materializing -> ready
pending -> materializing -> failed
```

Replacement, invalidation, deletion, retention policy and authorized cross-Run reuse
remain separate Platform lifecycle work.

## Service time

For facility `f` in origin city `o` and demand point `d` in destination city `c`:

`end_to_end_seconds = order_cutoff_wait_seconds + facility.handling_seconds + route.travel_seconds + last_mile_buffer_seconds`

A unit is covered only when that value is less than or equal to
`max_delivery_seconds`. A “1-day” result is therefore meaningful only when the snapshot
policy explicitly encodes the intended one-day promise.

## Cost

The modeled variable unit cost is:

`facility.handling_cost_per_unit + lane.base_cost_per_unit + lane.distance_km * lane.distance_cost_per_km_per_unit`

The lane is selected exactly by `(o.city_id, c.city_id)`; lanes never use a demand-point
identifier. The scenario total is variable allocation cost plus fixed cost for every active
facility.
All costs use the snapshot currency and planning period.

## Allocation and location

Optimized scenarios use capacity-constrained min-cost flow. Demand and capacity are
integer planning units; demand may split across facilities. The primary objective is
maximum covered demand within the service limit, followed by minimum variable cost for
the active footprint.

Facility location keeps existing facilities open and enumerates all subsets of at most
14 supplied candidates. It chooses the smallest number of candidates that reaches the
target, then the lowest total modeled cost. This is exact for the finite input set, not
for arbitrary points on a map.

## Known MVP boundary

The Data MCP discovers only supported files in the trusted Turn Workspace and accepts
opaque source references from that discovery. It has no packaged source catalog, source-ID
fallback or arbitrary SQL interface. A separate Demo MCP may create one versioned synthetic
source set only after explicit user authorization and only in an empty Workspace; those files
still require the normal published-profile, mapping-confirmation and normalization flow.

The large Demo template has bounded city-grain sources: 24 city-period demand rows, six
facilities, 24 warehouse-to-city coverage rows and 144 combined city-to-city lanes. Demand
does not carry a duplicate coordinate or synthetic order identifier; coordinates are owned by
the City entity.

The contract does not yet model multi-echelon inventory, safety stock, SKU-specific
capacity, facility construction schedules, closure decisions, carrier step tariffs,
time-dependent traffic distributions, tax, carbon, or uncertainty. Extend the typed
contract and deterministic engine rather than embedding those semantics in a Skill
prompt.
