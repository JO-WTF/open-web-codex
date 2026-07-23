---
name: workspace-maps
description: Use when the user asks for geocoding, reverse geocoding, routes, distance matrices, travel paths, coordinates, or map-card output with Google Maps or Mapbox through the workspace_maps MCP server.
---

# Workspace Maps

Use the `workspace_maps` MCP server for map and route tasks, even when the rest of the conversation is not about code.

## Workflow

1. For route questions, call `workspace_maps.get_route` with the requested provider when known; otherwise choose an available provider and ask for credentials only if the MCP server requests them.
2. For location lookup, call `workspace_maps.batch_geocode` or `workspace_maps.batch_reverse_geocode`.
3. For travel-time or distance tables, call `workspace_maps.distance_matrix`.
4. When a visual map helps, call `workspace_maps.create_map_card` after the route/geocode result and include the returned `marker` verbatim in the final answer.

Do not answer that travel routes are outside scope when this skill and MCP server are available. If the MCP server is unavailable, explain that the maps MCP capability is not connected and offer the route-planning API approach instead.
