from __future__ import annotations

import unittest

import maps_mcp.server as server
from maps_mcp.map_card import GeoJsonSource
from mcp.server.fastmcp.exceptions import ToolError
from mcp.types import CallToolResult
from pydantic import ValidationError


def geojson() -> dict[str, object]:
    return {"type": "FeatureCollection", "features": []}


def data_ref(uri: str = "maps-data://geojson/map-data-1234") -> dict[str, object]:
    return {
        "type": "mcp_resource",
        "server": "map_utils",
        "uri": uri,
        "format": "geojson",
        "profile": {
            "schema_version": "geojson-profile.v1",
            "feature_count": 1,
            "discriminator_property": None,
            "feature_types": [
                {
                    "value": "geometry:mixed",
                    "feature_count": 1,
                    "geometry_types": ["LineString", "Point"],
                    "properties": [
                        {"name": "distance", "types": ["number"]},
                        {"name": "name", "types": ["string"]},
                    ],
                    "sample_properties": {"distance": 10.0, "name": "Example"},
                }
            ],
        },
    }


def network_data_ref() -> dict[str, object]:
    return {
        "type": "mcp_resource",
        "server": "supply_chain",
        "uri": "supply-chain://resources/network_distribution_geojson.v1-digest",
        "format": "geojson",
        "profile": {
            "schema_version": "geojson-profile.v1",
            "feature_count": 61,
            "discriminator_property": "kind",
            "feature_types": [
                {
                    "value": "demand",
                    "feature_count": 50,
                    "geometry_types": ["Point"],
                    "properties": [
                        {"name": "kind", "types": ["string"], "enum_values": ["demand"]},
                        {"name": "city_name", "types": ["string"]},
                        {"name": "duration_hours", "types": ["number"]},
                    ],
                    "sample_properties": {
                        "kind": "demand",
                        "city_name": "Alpha",
                        "duration_hours": 8.0,
                    },
                },
                {
                    "value": "warehouse",
                    "feature_count": 11,
                    "geometry_types": ["Point"],
                    "properties": [
                        {"name": "kind", "types": ["string"], "enum_values": ["warehouse"]},
                        {"name": "warehouse_name", "types": ["string"]},
                        {"name": "warehouse_type", "types": ["string"]},
                    ],
                    "sample_properties": {
                        "kind": "warehouse",
                        "warehouse_name": "Center",
                        "warehouse_type": "center",
                    },
                },
            ],
        },
    }


class MapCardTests(unittest.IsolatedAsyncioTestCase):
    async def test_preserves_standard_mapbox_layers(self) -> None:
        layer = {
            "id": "routes",
            "type": "line",
            "source": "routes",
            "minzoom": 3,
            "layout": {"line-cap": "round"},
            "paint": {
                "line-color": [
                    "interpolate",
                    ["linear"],
                    ["zoom"],
                    4,
                    "#2563eb",
                    12,
                    "#ef4444",
                ],
                "line-width": 4,
            },
        }
        result = await server.create_map_card(
            title="路线",
            sources={
                "routes": GeoJsonSource(
                    type="geojson",
                    data_ref=data_ref(),
                    lineMetrics=True,
                )
            },
            layers=[layer],
            center=(114.0579, 22.5431),
            zoom=8,
            extensions={
                "hover": {
                    "layers": [
                        {
                            "layer": "routes",
                            "title_property": "name",
                            "fields": ["distance"],
                        }
                    ]
                },
                "legend": {"items": [{"label": "路线", "color": "#2563eb", "type": "line"}]},
            },
        )

        self.assertIsInstance(result, CallToolResult)
        assert result.structuredContent is not None
        renderer = result.structuredContent["artifact"]["renderer"]
        self.assertEqual(renderer["kind"], "map.v3")
        payload = renderer["payload"]
        self.assertEqual(payload["layers"], [layer])
        self.assertTrue(payload["sources"]["routes"]["lineMetrics"])
        self.assertEqual(payload["center"], [114.0579, 22.5431])
        self.assertEqual(
            payload["extensions"]["hover"]["layers"][0]["fields"],
            ["distance"],
        )

    async def test_resource_data_ref_is_managed_without_copying_geojson(self) -> None:
        uri = "maps-data://geojson/map-data-1234"
        result = await server.mcp.call_tool(
            "create_map_card",
            {
                "title": "路线",
                "sources": {
                    "route": {
                        "type": "geojson",
                        "data_ref": data_ref(uri),
                    }
                },
                "layers": [
                    {
                        "id": "route",
                        "type": "line",
                        "source": "route",
                        "paint": {"line-color": "#2563eb"},
                    }
                ],
            },
        )
        assert result.structuredContent is not None
        source = result.structuredContent["artifact"]["renderer"]["payload"]["sources"]["route"]
        self.assertEqual(source["data"]["uri"], uri)
        self.assertNotIn("profile", source["data"])

    async def test_rejects_inline_geojson_and_requires_data_ref(self) -> None:
        with self.assertRaises(ValidationError):
            GeoJsonSource(type="geojson", data=geojson())

        with self.assertRaisesRegex(ToolError, "data_ref"):
            await server.mcp.call_tool(
                "create_map_card",
                {
                    "title": "Inline data",
                    "sources": {"network": {"type": "geojson", "data": geojson()}},
                    "layers": [
                        {
                            "id": "network",
                            "type": "circle",
                            "source": "network",
                            "paint": {"circle-color": "#2563eb"},
                        }
                    ],
                },
            )

    async def test_accepts_reviewed_external_local_geojson_reference(self) -> None:
        uri = "supply-chain://resources/network_distribution_geojson.v1-digest"
        result = await server.mcp.call_tool(
            "create_map_card",
            {
                "title": "Network comparison",
                "sources": {
                    "network": {
                        "type": "geojson",
                        "data_ref": network_data_ref(),
                    }
                },
                "layers": [
                    {
                        "id": "network",
                        "type": "circle",
                        "source": "network",
                        "paint": {"circle-color": "#2563eb"},
                    }
                ],
            },
        )

        assert result.structuredContent is not None
        source = result.structuredContent["artifact"]["renderer"]["payload"]["sources"]["network"]
        self.assertEqual(source["data"]["server"], "supply_chain")
        self.assertEqual(source["data"]["uri"], uri)

    async def test_rejects_public_host_and_model_visible_resource_identities(
        self,
    ) -> None:
        for server_name, uri in [
            ("mcp__map_utils", "maps-data://geojson/map-data-1234"),
            ("map_utils", "file:///tmp/network.geojson"),
            ("map_utils", "https://example.com/network.geojson"),
        ]:
            with self.subTest(server_name=server_name, uri=uri):
                with self.assertRaises(ValidationError):
                    GeoJsonSource.model_validate(
                        {
                            "type": "geojson",
                            "data_ref": {
                                **data_ref(),
                                "server": server_name,
                                "uri": uri,
                            },
                        }
                    )

    async def test_result_requires_standalone_assistant_embed_paragraph(self) -> None:
        result = await server.create_map_card(
            title="路线",
            sources={
                "routes": GeoJsonSource(
                    type="geojson",
                    data_ref=data_ref(),
                )
            },
            layers=[
                {
                    "id": "routes",
                    "type": "line",
                    "source": "routes",
                    "paint": {"line-color": "#2563eb"},
                }
            ],
        )

        assert result.structuredContent is not None
        embed_code = result.structuredContent["embed"]["code"]
        self.assertEqual(len(result.content), 1)
        message = result.content[0]
        self.assertEqual(message.type, "text")
        assert message.text is not None
        self.assertIn("not displayed until", message.text)
        self.assertIn("standalone paragraph", message.text)
        self.assertIn("may appear anywhere", message.text)
        self.assertIn(f"\n\n{embed_code}\n\n", message.text)

    async def test_official_validator_warns_unknown_and_rejects_invalid_known_syntax(
        self,
    ) -> None:
        warning = await server.mcp.call_tool(
            "create_map_card",
            {
                "title": "Warning",
                "sources": {
                    "data": {"type": "geojson", "data_ref": data_ref()},
                },
                "layers": [
                    {
                        "id": "points",
                        "type": "circle",
                        "source": "data",
                        "paint": {
                            "circle-color": "#ef4444",
                            "unknown-property": 1,
                        },
                    }
                ],
            },
        )
        assert warning.structuredContent is not None
        self.assertEqual(
            warning.structuredContent["warnings"][0]["code"],
            "mapbox_style_warning",
        )

        with self.assertRaisesRegex(ToolError, "number expected"):
            await server.mcp.call_tool(
                "create_map_card",
                {
                    "title": "Invalid",
                    "sources": {
                        "data": {"type": "geojson", "data_ref": data_ref()},
                    },
                    "layers": [
                        {
                            "id": "points",
                            "type": "circle",
                            "source": "data",
                            "paint": {"circle-radius": "large"},
                        }
                    ],
                },
            )

    async def test_schema_exposes_raw_layers_sources_and_optional_extensions(self) -> None:
        tool = next(
            tool for tool in await server.mcp.list_tools() if tool.name == "create_map_card"
        )
        assert tool.inputSchema is not None
        self.assertEqual(tool.inputSchema["properties"]["layers"]["items"]["type"], "object")
        self.assertEqual(tool.inputSchema["properties"]["sources"]["type"], "object")
        self.assertIn("extensions", tool.inputSchema["properties"])
        self.assertIn("center", tool.inputSchema["properties"])
        self.assertNotIn("view", tool.inputSchema["properties"])
        self.assertNotIn("legend", tool.inputSchema["properties"])
        self.assertIn("data_ref", str(tool.inputSchema))
        assert tool.outputSchema is not None
        self.assertEqual(
            tool.outputSchema["$defs"]["Renderer"]["properties"]["kind"]["const"],
            "map.v3",
        )

    async def test_rejects_harbor_filters_that_do_not_exist_on_demand_points(self) -> None:
        with self.assertRaisesRegex(ToolError, "demand_city_id"):
            await server.mcp.call_tool(
                "create_map_card",
                {
                    "title": "Invalid coverage",
                    "sources": {
                        "network": {"type": "geojson", "data_ref": network_data_ref()}
                    },
                    "layers": [
                        {
                            "id": "covered-cities",
                            "type": "circle",
                            "source": "network",
                            "filter": [
                                "all",
                                ["has", "demand_city_id"],
                                ["<=", ["get", "duration_hours"], 12],
                            ],
                            "paint": {"circle-color": "#16A34A"},
                        }
                    ],
                },
            )

    async def test_validates_fields_within_selected_feature_kind(self) -> None:
        with self.assertRaisesRegex(ToolError, "warehouse_name"):
            await server.mcp.call_tool(
                "create_map_card",
                {
                    "title": "Invalid demand hover",
                    "sources": {
                        "network": {"type": "geojson", "data_ref": network_data_ref()}
                    },
                    "layers": [
                        {
                            "id": "demand",
                            "type": "circle",
                            "source": "network",
                            "filter": ["==", ["get", "kind"], "demand"],
                            "paint": {"circle-color": "#16A34A"},
                        }
                    ],
                    "extensions": {
                        "hover": {
                            "layers": [
                                {
                                    "layer": "demand",
                                    "title_property": "city_name",
                                    "fields": ["warehouse_name"],
                                }
                            ]
                        }
                    },
                },
            )

    async def test_rejects_line_layer_for_point_only_profile(self) -> None:
        with self.assertRaisesRegex(ToolError, "cannot render profiled geometries"):
            await server.mcp.call_tool(
                "create_map_card",
                {
                    "title": "Invalid lines",
                    "sources": {
                        "network": {"type": "geojson", "data_ref": network_data_ref()}
                    },
                    "layers": [
                        {
                            "id": "assignments",
                            "type": "line",
                            "source": "network",
                            "paint": {"line-color": "#94A3B8"},
                        }
                    ],
                },
            )


if __name__ == "__main__":
    unittest.main()
