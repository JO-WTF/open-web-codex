from __future__ import annotations

import json
from pathlib import Path
import tempfile
import unittest

from mcp.types import CallToolResult, ResourceLink

import maps_mcp.server as server
from maps_mcp.data_refs import GeoJsonResourceStore


def inline_source() -> server.MapSource:
    return server.MapSource(
        id="locations",
        data=server.InlineMapData(
            geojson={
                "type": "FeatureCollection",
                "features": [
                    {
                        "type": "Feature",
                        "properties": {"label": "上海"},
                        "geometry": {
                            "type": "Point",
                            "coordinates": [121.4737, 31.2304],
                        },
                    }
                ],
            }
        ),
    )


class MapCardTests(unittest.IsolatedAsyncioTestCase):
    async def test_create_map_card_returns_map_v2_contract(self) -> None:
        result = await server.create_map_card(
            title="上海点位",
            intent="location",
            sources=[inline_source()],
            layers=[
                server.PointLayer(
                    id="points",
                    source="locations",
                    label_property="label",
                    style=server.PointStyle(
                        color="#ef4444",
                        opacity=0.8,
                        radius=9,
                        stroke_color="#ffffff",
                        stroke_width=2,
                    ),
                )
            ],
            viewport=server.CameraViewport(center=(121.4737, 31.2304), zoom=11),
            summary="一个可渲染点位",
        )

        self.assertIsInstance(result, CallToolResult)
        self.assertIsNotNone(result.structuredContent)
        structured_content = result.structuredContent
        assert structured_content is not None
        self.assertEqual(structured_content["type"], "open-web-card")
        self.assertEqual(structured_content["kind"], "map.v2")
        payload = structured_content["card"]
        self.assertEqual(payload["viewport"]["zoom"], 11)
        self.assertEqual(payload["layers"][0]["style"]["opacity"], 0.8)
        self.assertEqual(result.content[0].text, "Map card ready: 上海点位")

    async def test_create_map_card_references_prior_mcp_resource_uri(self) -> None:
        resource_uri = "maps-data://geojson/map-data-1234"
        result = await server.create_map_card(
            title="路线地图",
            intent="route",
            sources=[
                server.MapSource(
                    id="route",
                    data=server.McpResourceMapData(
                        server="map_utils",
                        uri=resource_uri,
                    ),
                )
            ],
            layers=[
                server.LineLayer(
                    id="route-line",
                    source="route",
                    style=server.LineStyle(
                        color="#2563eb",
                        width=5,
                        opacity=0.9,
                        dash=[2, 1],
                    ),
                )
            ],
            viewport=server.FitViewport(padding=48, max_zoom=14),
            fallback_text="已生成路线地图。",
        )

        assert result.structuredContent is not None
        payload = result.structuredContent["card"]
        self.assertEqual(payload["sources"][0]["data"]["server"], "map_utils")
        self.assertEqual(payload["sources"][0]["data"]["uri"], resource_uri)
        self.assertEqual(payload["viewport"]["mode"], "fit")

    async def test_create_map_card_advertises_and_validates_output_schema(self) -> None:
        tools = await server.mcp.list_tools()
        tool = next(tool for tool in tools if tool.name == "create_map_card")
        geocode_tool = next(tool for tool in tools if tool.name == "batch_geocode")

        self.assertIsNotNone(tool.outputSchema)
        assert tool.outputSchema is not None
        self.assertEqual(set(tool.outputSchema["required"]), {"type", "kind", "card"})
        assert tool.inputSchema is not None
        card_resource_schema = tool.inputSchema["$defs"]["McpResourceMapData"]
        self.assertIn("server", card_resource_schema["required"])
        self.assertEqual(
            card_resource_schema["properties"]["server"]["const"],
            "map_utils",
        )
        assert geocode_tool.outputSchema is not None
        output_resource_schema = geocode_tool.outputSchema["$defs"][
            "McpResourceMapData"
        ]
        self.assertIn("server", output_resource_schema["required"])
        self.assertEqual(
            output_resource_schema["properties"]["server"]["const"],
            "map_utils",
        )

        result = await server.mcp.call_tool(
            "create_map_card",
            {
                "title": "上海点位",
                "sources": [
                    {
                        "id": "locations",
                        "data": {
                            "type": "inline",
                            "format": "geojson",
                            "geojson": {"type": "FeatureCollection", "features": []},
                        },
                    }
                ],
                "layers": [
                    {
                        "id": "points",
                        "source": "locations",
                        "geometry": "point",
                        "style": {"color": "#ef4444"},
                    }
                ],
            },
        )
        self.assertIsInstance(result, CallToolResult)
        self.assertIsNotNone(result.structuredContent)

    async def test_geojson_tool_result_includes_resource_server_and_uri(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            store = GeoJsonResourceStore(Path(directory))
            geojson = {
                "type": "FeatureCollection",
                "features": [],
            }
            original_store = server._resource_store
            server._resource_store = store
            self.addCleanup(setattr, server, "_resource_store", original_store)

            result = server._resource_result("mapbox", "Geocoded 0 addresses.", geojson)
            structured = result.structuredContent
            assert structured is not None
            data_ref = structured["data_ref"]
            source = server.MapSource(id="locations", data=data_ref)

            self.assertEqual(source.data.type, "mcp_resource")
            self.assertEqual(data_ref["server"], "map_utils")
            resource_uri = data_ref["uri"]
            self.assertTrue(resource_uri.startswith("maps-data://geojson/"))

            contents = list(await server.mcp.read_resource(resource_uri))
            self.assertEqual(len(contents), 1)
            self.assertEqual(json.loads(contents[0].content), geojson)

    def test_geojson_resource_store_publishes_opaque_resource_link_data(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            store = GeoJsonResourceStore(Path(directory))
            geojson = {
                "type": "FeatureCollection",
                "features": [],
            }
            published = store.publish(geojson)
            link = server._resource_link(published, "test data")

            self.assertIsInstance(link, ResourceLink)
            self.assertEqual(link.name, published.resource_id)
            self.assertEqual(link.title, "Maps GeoJSON")
            self.assertEqual(str(link.uri), published.uri)
            self.assertEqual(link.mimeType, "application/geo+json")
            self.assertIsNone(link.meta)
            self.assertEqual(json.loads(store.read(published.resource_id)), geojson)


if __name__ == "__main__":
    unittest.main()
