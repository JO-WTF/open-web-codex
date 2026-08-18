# Indonesia tutorial data contract

This document owns the reproducible synthetic-data rules for the Indonesia
warehouse-network tutorial. It does not claim that synthetic customers, carrier
quotes, assignments, or warehouse capacities are observed business facts.

## Source boundary

- The current province owner is the 38-feature World Bank ADM1 release locked in
  `examples/indonesia-tutorial/source-lock.json`. Its SHA-256-verified geometry
  is downloaded into an ignored local cache only when generation is explicitly
  requested; it is redistributed under CC BY 4.0 by its source owner.
- geoBoundaries IDN ADM1 is an independent check. It contains the former
  34-province structure, so each of the four 2022 Papua splits is checked against
  its former Papua or West Papua parent instead of being treated as a missing
  province.
- The Indonesia Ministry of Agriculture 38-province service is used only to
  compare the complete province-name set and geometry envelope. It is not
  redistributed because the service does not state a license.
- Population is the BPS 2025 mid-year projection from Statistical Yearbook of
  Indonesia 2025, table 3.1.1. The checked-in CSV contains 38 province values
  displayed to the nearest 100 people. Those rounded rows sum to 284,438,600,
  while the same table reports an Indonesia total of 284,438,800. Both facts and
  the 200-person rounding difference remain explicit; no province is altered to
  force the displayed rows to equal the national total.

Every generated customer, warehouse, and candidate site must be inside its
current province polygon and inside the matching geoBoundaries polygon or former
Papua parent. A boundary mismatch is a generation failure, not a warning.

## Synthetic demand

The release contains 240,000 de-identified synthetic customer locations. Counts
are allocated with:

```text
weight = population_2025 ^ 0.78 * commercial_factor
```

This keeps population as the main driver without making customer counts directly
proportional to population. Commercial factors represent the tutorial company's
stronger urban and trade-corridor presence and weaker remote-market presence.
North Kalimantan, West Papua, and South Papua have zero active tutorial demand;
they remain in the province and boundary files.

Within a province, 85% of points are sampled around four deterministic interior
clusters and 15% are sampled across land polygons. This produces a plausible
business distribution without pretending to reproduce household-level
population density. Customer records contain no names, addresses, contact
details, or observed individuals.

## Existing network

The existing footprint has exactly three central warehouses and eight forward
warehouses:

| Type | Locations |
| --- | --- |
| Central | Bekasi, Sidoarjo, Makassar |
| Forward | Medan, Palembang, Semarang, Denpasar, Banjarmasin, Balikpapan, Manado, Jayapura |

Forward warehouses have one current central parent. Customer assignments are
mostly geographic but deliberately imperfect: 82% use the nearest current
forward warehouse, 13% retain a plausible second-nearest legacy assignment, and
5% follow a province-level legacy contract. This creates measurable improvement
room without fabricating a nonsensical baseline.

## Distance, time, and cost

Tutorial calculations use no navigation API:

```text
estimated_distance_km = haversine_distance_km * 1.25
driving_hours = estimated_distance_km / 45
service_days = max(1, ceil(driving_hours / 6))
```

The customer promise clock begins at the forward warehouse because the tutorial
assumes forward inventory is already stocked. Central-to-forward time is
reported as replenishment time but is not added to the customer delivery promise.
The full two-level cost includes both central-to-forward linehaul and
forward-to-customer last-mile demand.

Synthetic quote rows are deterministic functions of estimated distance, lane
type, island/remote factors, and bounded lane noise. Distance must remain strongly
positively correlated with quoted unit cost. The validation report records both
Pearson and Spearman correlations.

## Province ranking

Service Resources rank only provinces with positive demand. Two-day demand
coverage is the primary metric. Priority provinces sort by ascending coverage,
then descending demand-weighted average service days; best provinces use the
opposite directions. Province code is the deterministic final tie-breaker.

The typed `province_ranking_policy` records this method. The ordered
`priority_province_codes` and `best_province_codes` arrays are validated against
the province metrics before publication. Consumers must not silently replace
this ranking with an average-days or candidate-benefit ranking.

## Finite-candidate optimization

The optimization Resource evaluates every reviewed candidate under one
deterministic policy. `evaluated_candidate_count` records the complete set size,
while `target_met_candidate_count` records how many evaluations satisfy the
requested service target. Publication validation recomputes both counts from
`evaluations`, checks them against `status`, and verifies that the selected
candidate exists and satisfies the target when status is `target_met`.

Consumers use `target_met_candidate_count`; they must not ask a model to count
boolean evaluation rows.

## Files and lifecycle

`scripts/generate_indonesia_tutorial_data.py --download-missing-sources` writes
one reproducible local release under the ignored
`examples/indonesia-tutorial/releases/1.0.0/` directory:

- `dataset-manifest.json`: file roles, row counts, sizes, and SHA-256 digests;
- `province-boundaries.geojson`: normalized 38-province geometry;
- `customers.csv.gz`: customer points and annual normalized demand;
- `customer-assignments.csv.gz`: current forward assignment and distance facts;
- `warehouses.csv` and `warehouse-links.csv`: current two-level footprint;
- `candidate-locations.csv`: finite reviewed forward-warehouse candidates;
- `transport-quotes.csv`: current linehaul and last-mile unit-demand quotes;
- `planning-policy.json`: distance, time, service-clock, and cost assumptions;
- `validation-report.json`: source, geometry, distribution, quote, and network
  quality gates.

The model must receive the manifest, bounded summaries, and immutable Resource
references rather than raw customer rows. Domain MCP tools own streaming reads
and deterministic calculations. Browser and platform layers must not interpret
the business schema. Generated releases and downloaded source geometry are not
checked into the repository.
