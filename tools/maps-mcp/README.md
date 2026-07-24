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

The platform startup path prepares one shared maps MCP Python environment before user Threads run.
By default `scripts/run-local.sh` creates or refreshes it under
`$OPEN_WEB_CODEX_DATA_DIR/tool-envs/maps-mcp` by calling `scripts/setup-maps-mcp-env.sh`, and then
exports `OPEN_WEB_CODEX_MAPS_MCP_VENV`/`MAPS_MCP_VENV` so every user conversation, workspace, and
Thread started by that local platform process reuses the same environment. The launcher uses the
same repo-level shared path as its fallback even when the MCP child receives a sanitized
environment. This keeps virtualenv creation and dependency downloads out of the user-visible MCP
handshake and avoids accidentally falling back to a stale plugin-local `.venv`.

For manual development from this directory you can still run:

```bash
python3 -m venv .venv
.venv/bin/pip install -e .
```

Google projects must enable Geocoding API v4 and Routes API. Mapbox requires an access token with
Geocoding, Directions, and Matrix access.

## Codex discovery

This directory is also a Codex plugin root. The checked-in `.codex-plugin/plugin.json` points to
`./.mcp.json` and `./skills/`, so Codex can discover the `map_utils` MCP server and route/map
Skill guidance from the selected workspace's capability roots instead of relying on `run-local.sh`
or hand-edited Profile `config.toml` entries. In the Web platform, the native Profile Host adapter
selects local plugin roots discovered under the source tree or workspace `tools/` directories when it starts
a new Thread; deployments may add extra absolute roots with `OPEN_WEB_CODEX_CAPABILITY_ROOTS`.

The plugin MCP config starts `./bin/maps-mcp-launcher` with `cwd="."` and requests the shared maps
environment/logging variables and standard uppercase/lowercase proxy variables via `env_vars`. The
launcher resolves the plugin root and repository root, verifies that the shared environment can
import `maps_mcp` and `mcp`, keeps startup logs on stderr so MCP stdio stdout remains JSON-RPC-only,
and then execs `python -m maps_mcp.server`. Remote Google and Mapbox calls use the inherited
`HTTP_PROXY`, `HTTPS_PROXY` and `NO_PROXY` settings; `ALL_PROXY` is the fallback for HTTP and HTTPS
when protocol-specific settings are absent. TLS certificate and hostname validation use the system
defaults. If the shared environment is missing or incomplete,
the launcher fails fast with instructions to run `scripts/setup-maps-mcp-env.sh` instead of doing
dependency downloads during a user conversation. Set `MAPS_MCP_AUTO_INSTALL=1` only for ad-hoc manual
development. The MCP client must advertise URL elicitation support. If the current browser surface
cannot render the key request, the tool fails safely instead of requesting the key in a model-visible
form.

`scripts/setup-maps-mcp-env.sh` writes detailed setup logs to
`$OPEN_WEB_CODEX_LOG_DIR/maps-mcp-env.log` by default. The log records timestamps, Python/pip
versions, OS information, venv path, command exit context, and whether proxy variables are set
without printing proxy values or credentials. Use that file first when debugging server proxy,
dependency, Python version, or platform startup failures.

`bin/maps-mcp-launcher` writes per-start handshake diagnostics to
`$OPEN_WEB_CODEX_LOG_DIR/maps-mcp-launcher.log` by default. It records cwd, launcher args, selected
repo root, selected venv, Python version, proxy-variable presence, import-check status including the
first import error summary, and the MCP server stderr stream. When Codex reports
`connection closed: initialize response`, inspect this launcher log together with `maps-mcp-env.log`
to distinguish missing shared env, import errors, Python runtime errors, and FastMCP startup
exceptions.

## Tests

Tests use fake HTTP responses and never call a paid provider:

```bash
PYTHONPATH=. python3 -m unittest discover -s tests -v
```

Map-card output:

Geocoding and routing tools publish GeoJSON through a standard MCP `resource_link`. Their
schema-validated output contains the raw MCP server ID and Resource URI in `data_ref`. Copy the
complete object unchanged into a later `create_map_card` source in the same Run and Thread. When
downstream work needs the GeoJSON contents, pass `data_ref.server` and `data_ref.uri` unchanged to MCP
`resources/read`; `mcp__map_utils` is a model-visible Tool namespace, not the Resource server ID.
`create_map_card` returns an `open-web-artifact` / `inline-visualization.v1` envelope with a
typed `map.v2` renderer in MCP `structuredContent`. The host validates and registers the
Artifact without displaying it. Assistant messages copy only `structuredContent.embed.code`
onto its own line at the desired position and must not reproduce renderer JSON or GeoJSON.
`map.v2` supports fit or camera viewports, Mercator rendering, and styled point, line, and
polygon layers. Point layers support built-in shapes or HTTPS raster icons; line and polygon
borders support solid or dash arrays. Every geometry can declare a safe, text-only hover view
over selected GeoJSON properties. Inline GeoJSON is supported for small data; large data is
read lazily from the referenced MCP Resource.

Provider endpoints implemented:

- Google Geocoding API v4 address/location endpoints
- Google Routes API `computeRoutes` and `computeRouteMatrix`
- Mapbox Geocoding API v6 batch endpoint
- Mapbox Directions API v5 and Matrix API v1
