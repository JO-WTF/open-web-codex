---
name: map-utils
description: Use the map_utils MCP server for geocoding, reverse geocoding, routes, travel paths, distance matrices, coordinates, or map-card output backed by one selected Google Maps or Mapbox provider. Use when a task needs location lookup, routing, travel distances or times, geographic coordinates, or an interactive map result.
---

# Map Utils

Use `map_utils` for paid geocoding and routing. All data tools use the single provider and key currently selected in global maps configuration; never pass a provider or API key as a tool argument.

## Workflow

1. Call the required data tool directly:
   - Use `map_utils.batch_geocode` for addresses.
   - Use `map_utils.batch_reverse_geocode` for coordinates.
   - Use `map_utils.get_route` for a route.
   - Use `map_utils.distance_matrix` for travel distances or times.
2. If configuration is missing, let the MCP server request it. The user can select Mapbox or Google; the new provider and key replace the prior configuration.
3. If the user declines configuration, or the provider rejects the key, returns an API error, times out, or has a network failure, report the error and stop. Do not retry by switching providers or asking for the key in chat.
4. Geocoding and route tools return `structuredContent.data_ref` and an MCP `resource_link` for the same MCP Resource:
   - For a map card, copy the complete `data_ref` object unchanged into `create_map_card.sources[].data`. Do not call `read_mcp_resource` merely to create the card.
   - If a downstream tool needs values inside the GeoJSON, such as coordinates for a route call, pass `data_ref.server` unchanged as `read_mcp_resource.server` and `data_ref.uri` unchanged as `read_mcp_resource.uri`.
   - The raw server ID is `map_utils`. Never pass the model-visible Tool namespace `mcp__map_utils` as the Resource server.
   - Do not copy GeoJSON into the card or assistant text.
5. Define one or more layers for each source. Choose `point`, `line`, or `polygon`, set the requested color, opacity, size, stroke, and dash style, then choose either a `fit` viewport or an explicit `camera` center and zoom.
6. Treat `create_map_card.structuredContent` as the complete card output. Continue with concise prose only; never reproduce the structured card JSON or Resource contents in the answer.

Example card arguments after a geocoding result returns
`data_ref = {"type":"mcp_resource","server":"map_utils","uri":"maps-data://geojson/map-data-...","format":"geojson"}`:

```json
{
  "title": "Locations",
  "sources": [
    {
      "id": "locations",
      "data": {
        "type": "mcp_resource",
        "server": "map_utils",
        "uri": "maps-data://geojson/map-data-...",
        "format": "geojson"
      }
    }
  ],
  "layers": [
    {
      "id": "location-points",
      "source": "locations",
      "geometry": "point",
      "label_property": "label",
      "style": {
        "color": "#ef4444",
        "opacity": 0.9,
        "radius": 8,
        "stroke_color": "#ffffff",
        "stroke_width": 2
      }
    }
  ],
  "viewport": {
    "mode": "fit",
    "padding": 48,
    "max_zoom": 14
  }
}
```

Do not expose keys in prompts, tool arguments, answers, logs, or map-card payloads. Do not claim maps are outside scope while `map_utils` is available. If the MCP server is unavailable, state that the map capability is not connected.
