# Workspace Maps MCP

Python MCP server that exposes paid Google Maps and Mapbox operations without modifying
`codex-rs`:

- `batch_geocode`
- `batch_reverse_geocode`
- `get_route`
- `distance_matrix`
- `create_map_card`

Every tool accepts `provider="google"` or `provider="mapbox"`. Provider request limits are
handled inside the server. Batch geocoding is capped at 500 inputs and distance matrices at 2,500
billable origin/destination elements per tool call.

## Credential memory

The server deliberately does not read provider keys from environment variables. It reads the key
from this ignored workspace-local file:

```text
<workspace>/.codex/maps-tool-memory.json
```

If the selected provider has no stored key, the tool sends an MCP URL elicitation request. It opens
a single-use page bound to `127.0.0.1` with a random 256-bit path token and a password input. The
key travels directly from the browser to the local MCP process; it does not pass through the MCP
client or model context. Selecting **Remember in this workspace** writes the key with file mode
`0600` and directory mode `0700`. Keys are never included in tool results or error URLs.

This is a local credential memory owned by this MCP server, not Codex semantic memory. Do not put
API keys in `MEMORY.md`, instructions, prompts, or model-visible Tool arguments. The current Web
platform does not yet provide the production Secret Provider or durable elicitation boundary, so
this server is for local workspace use until those platform gates are complete.

## Install

From this directory:

```bash
python3 -m venv .venv
.venv/bin/pip install -e .
```

Google projects must enable Geocoding API v4 and Routes API. Mapbox requires an access token with
Geocoding, Directions, and Matrix access.

## Codex discovery

This directory is also a Codex plugin root. The checked-in `.codex-plugin/plugin.json` points to
`./.mcp.json`, so Codex can discover the `workspace_maps` MCP server from the selected workspace's
capability roots instead of relying on `run-local.sh` or hand-edited Profile `config.toml` entries.

The plugin MCP config starts `.venv/bin/python -m maps_mcp.server` with `cwd="."`; run the install
steps above before asking Codex to load the plugin. The MCP client must advertise URL elicitation
support. If the current browser surface cannot render the key request, the tool fails safely instead
of requesting the key in a model-visible form.

## Tests

Tests use fake HTTP responses and never call a paid provider:

```bash
PYTHONPATH=. python3 -m unittest discover -s tests -v
```

Map-card output:

`create_map_card` returns an `open-web-card map.v1` fenced marker. Put the returned `marker`
verbatim in the assistant final answer where the Web UI should render the map card. Use small
inline points/lines/polygons for previews, and prefer `input_ref` or `artifact_id` for large
GeoJSON.

Provider endpoints implemented:

- Google Geocoding API v4 address/location endpoints
- Google Routes API `computeRoutes` and `computeRouteMatrix`
- Mapbox Geocoding API v6 batch endpoint
- Mapbox Directions API v5 and Matrix API v1
