# Map Utils MCP

Python MCP server that exposes paid Google Maps and Mapbox operations without modifying
`codex-rs`:

- `batch_geocode`
- `batch_reverse_geocode`
- `get_route`
- `distance_matrix`
- `create_map_card`

The geocoding and routing tools use one active provider/key pair selected in
configuration; provider and credentials are never model-visible tool arguments.
A later configuration replaces the previous provider and key. Provider request
limits are handled inside the server. Batch geocoding is capped at 500 inputs
and distance matrices at 2,500 billable origin/destination elements per call.

## Credential configuration and local memory

The Web platform stores one active provider/key pair as an encrypted global configuration. A later
save replaces the previous provider and key. Google credentials remain server-only; an active
Mapbox public browser token is returned only because Mapbox GL needs it to render cards.

The MCP server deliberately does not read provider keys from environment variables. It can read a
locally delivered credential from this ignored owner-only file:

```text
<workspace>/.codex/maps-tool-memory.json
```

If no provider/key is configured, the tool sends an MCP URL elicitation request.
The fallback form selects Mapbox or Google and accepts one key. It is served on a single-use
`127.0.0.1` URL with a random 256-bit path token. The browser does not navigate to that page:
the platform presents its own in-app dialog, validates and encrypts the configuration, then posts
the selected provider/key from the Server directly to the local MCP process. Only after successful
delivery does the platform accept the Runtime elicitation.

The MCP writes the delivered active credential with file mode `0600` and directory mode `0700`.
This file is local MCP credential memory, not Codex semantic memory and not the future per-user
storage boundary. The platform's current global encrypted configuration remains the reusable
browser-facing source; future multi-user work will move it to user/Profile scope. Do not put API
keys in `MEMORY.md`, instructions, prompts, model-visible tool arguments, results, or logs.

## Install

This Tool source declares its Python project, Node project, hash lock, `map_utils` module entry and
typed environment bindings in `runtime.toml`. The SDK generic provisioner owns virtualenv creation,
pip/npm caches, staged non-editable installation, `npm ci --ignore-scripts`, timeouts and reuse.
Prepared environments live under the caller-provided output root, never inside this Tool, a Profile
or a Workspace. Python 3, Node.js and npm are explicit host adapters.

For manual repository development, invoke the same platform preparation contract from the
repository root:

```bash
PYTHONPATH=tools/copilot-sdk python3 -m copilot_sdk prepare . \
  --manifest apps/web/builtin/warehouse-network-copilot/copilot.toml \
  --output-root "$PWD/.local/open-web-codex/copilot-environment"
```

`scripts/run-local.sh` performs this generic preparation once before a real Platform Server starts.
It does not contain maps-specific installation logic. Runtime launch and user conversations never
download or install Tool dependencies.

Google projects must enable Geocoding API v4 and Routes API. Mapbox requires an access token with
Geocoding, Directions, and Matrix access.

## Codex discovery

This directory is a Tool source, not an authored Plugin transport root. The built-in Copilot
manifest identifies it as capability root `map_utils` and explicitly references `runtime.toml`.
The provisioner compiles an internal prepared descriptor; SDK `dev`/`test` create disposable
`.codex-plugin`/`.mcp.json` projections from it, while the Platform Server consumes the same
descriptor and projects the exact transport into the Profile Role. Neither path scans source or
writes hidden Profile configuration.

The Network Role keeps `default_tools_approval_mode` at `prompt` because geocoding, routing and
distance matrix calls reach a credentialed external provider and may be billable. Only
`create_map_card` is explicitly preapproved. This Role policy remains separate from environment
preparation and from provider credential elicitation. The MCP client must advertise URL elicitation
support; if the browser cannot render the request, the Tool fails safely instead of exposing a key
to the model.

## Tests

Tests use fake HTTP responses and never call a paid provider:

```bash
PYTHONPATH=. python3 -m unittest discover -s tests -v
```

Map-card output:

Geocoding and routing tools publish GeoJSON through a standard MCP `resource_link`. Their
schema-validated output contains the raw MCP server ID and Resource URI in `data_ref`. Copy the
complete object unchanged into `create_map_card.sources.<source-id>.data_ref` in the same Run and
Thread. When downstream work needs the GeoJSON contents, pass `data_ref.server` and `data_ref.uri`
unchanged to MCP `resources/read`; `mcp__map_utils` is a model-visible Tool namespace, not the
Resource server ID.

`create_map_card` accepts one Mapbox-style `map.v3` contract. `sources` contains
platform-managed GeoJSON and must use the Open Web `source.data_ref`; GeoJSON contents are never
passed through the model context. Standard GeoJSON source options are preserved.
`layers` is official Mapbox Style Specification Layer JSON and is validated by
`@mapbox/mapbox-gl-style-spec`; there is no separate layer, paint, layout, filter, or expression
whitelist. Official unknown-property diagnostics are warnings, while invalid known syntax fails.

Camera fields are the standard top-level `center`, `zoom`, `bearing`, and `pitch`; omitting
`center` and `zoom` fits all GeoJSON. Optional Open Web behavior lives under
`extensions.hover` and `extensions.legend`. There is no `style` wrapper, old `view` object,
layer-local hover, or top-level legend.

The Tool returns an `open-web-artifact` / `inline-visualization.v1` envelope with a typed
`map.v3` renderer. The host independently validates the browser DTO, resolves authorized
`data_ref` values to opaque Artifact URLs, and strips MCP Resource identity from public events.
The browser passes every layer to `map.addLayer` unchanged except for browser-local layer/source
IDs. Tool completion only creates the Artifact; it does not display the map. To display it,
Assistant messages copy only `structuredContent.embed.code` verbatim as a standalone paragraph
with a blank line before and after it. That paragraph may appear anywhere in the response where
the map should be shown and must not be wrapped in a code fence, blockquote, or list.

Provider endpoints implemented:

- Google Geocoding API v4 address/location endpoints
- Google Routes API `computeRoutes` and `computeRouteMatrix`
- Mapbox Geocoding API v6 batch endpoint
- Mapbox Directions API v5 and Matrix API v1
