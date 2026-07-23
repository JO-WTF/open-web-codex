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

The platform startup path prepares one shared maps MCP Python environment before user Threads run.
By default `scripts/run-local.sh` creates or refreshes it under
`$OPEN_WEB_CODEX_DATA_DIR/tool-envs/maps-mcp` by calling `scripts/setup-maps-mcp-env.sh`, and then
exports `OPEN_WEB_CODEX_MAPS_MCP_VENV`/`MAPS_MCP_VENV` so every user conversation, workspace, and
Thread started by that local platform process reuses the same environment. This keeps virtualenv
creation and dependency downloads out of the user-visible MCP handshake.

For manual development from this directory you can still run:

```bash
python3 -m venv .venv
.venv/bin/pip install -e .
```

Google projects must enable Geocoding API v4 and Routes API. Mapbox requires an access token with
Geocoding, Directions, and Matrix access.

## Codex discovery

This directory is also a Codex plugin root. The checked-in `.codex-plugin/plugin.json` points to
`./.mcp.json` and `./skills/`, so Codex can discover the `workspace_maps` MCP server and route/map
Skill guidance from the selected workspace's capability roots instead of relying on `run-local.sh`
or hand-edited Profile `config.toml` entries. In the Web platform, the native Profile Host adapter
selects local plugin roots discovered under the source tree or workspace `tools/` directories when it starts
a new Thread; deployments may add extra absolute roots with `OPEN_WEB_CODEX_CAPABILITY_ROOTS`.

The plugin MCP config starts `./bin/maps-mcp-launcher` with `cwd="."`; the launcher resolves the
plugin root, verifies that the shared environment can import `maps_mcp` and `mcp`, keeps startup
logs on stderr so MCP stdio stdout remains JSON-RPC-only, and then execs
`python -m maps_mcp.server`. If the shared environment is missing or incomplete, the launcher fails
fast with instructions to run `scripts/setup-maps-mcp-env.sh` instead of doing dependency downloads
during a user conversation. Set `MAPS_MCP_AUTO_INSTALL=1` only for ad-hoc manual development. The
MCP client must advertise URL elicitation support. If the current browser surface cannot render the
key request, the tool fails safely instead of requesting the key in a model-visible form.

`scripts/setup-maps-mcp-env.sh` writes detailed setup logs to
`$OPEN_WEB_CODEX_LOG_DIR/maps-mcp-env.log` by default. The log records timestamps, Python/pip
versions, OS information, venv path, command exit context, and whether proxy variables are set
without printing proxy values or credentials. Use that file first when debugging server proxy,
dependency, Python version, or platform startup failures.

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
