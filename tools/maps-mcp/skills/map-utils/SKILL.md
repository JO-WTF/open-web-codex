---
name: map-utils
description: Use the map_utils MCP server for geocoding, reverse geocoding, routes, travel paths, distance matrices, coordinates, or map-card output backed by one selected Google Maps or Mapbox provider. Use when a task needs location lookup, routing, travel distances or times, geographic coordinates, or an interactive map result.
---

# Map Utils

Use `map_utils` for paid geocoding and routing. All data tools use the single
provider and key currently selected in global maps configuration; never pass a
provider or API key as a tool argument.

## Workflow

1. Call the required data tool directly:
   - Use `map_utils.batch_geocode` for addresses.
   - Use `map_utils.batch_reverse_geocode` for coordinates.
   - Use `map_utils.get_route` for a route.
   - Use `map_utils.distance_matrix` for travel distances or times.
2. If configuration is missing, let the MCP server request it. The user can
   select Mapbox or Google; the new provider and key replace the prior
   configuration.
3. If the user declines configuration, or the provider rejects the key,
   returns an API error, times out, or has a network failure, report the error
   and stop. Do not retry by switching providers or asking for the key in chat.
4. When a visual map helps, call `map_utils.create_map_card` after obtaining
   coordinates or route geometry. Include its returned `marker` verbatim in the
   final answer.

Do not expose keys in prompts, tool arguments, answers, logs, or map-card
payloads. Do not claim maps are outside scope while `map_utils` is available.
If the MCP server is unavailable, state that the map capability is not
connected.
