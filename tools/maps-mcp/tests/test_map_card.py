from __future__ import annotations

import json
import unittest

from mcp.types import CallToolResult
from pydantic import ValidationError

import maps_mcp.server as server


class MapCardTests(unittest.IsolatedAsyncioTestCase):
    async def test_create_map_card_returns_open_web_marker(self) -> None:
        result = await server.create_map_card(
            title="上海点位",
            intent="location",
            points=[server.MapCardPoint(latitude=31.2304, longitude=121.4737, label="上海")],
            summary="一个可渲染点位",
        )

        self.assertIsInstance(result, CallToolResult)
        self.assertIsNotNone(result.structuredContent)
        structured_content = result.structuredContent
        assert structured_content is not None
        self.assertEqual(structured_content["type"], "open-web-card")
        self.assertEqual(structured_content["kind"], "map.v1")
        marker = structured_content["marker"]
        self.assertTrue(marker.startswith("```open-web-card map.v1\n"))
        self.assertTrue(marker.endswith("\n```"))
        payload = json.loads(marker.split("\n", 1)[1].rsplit("\n", 1)[0])
        self.assertEqual(payload["title"], "上海点位")
        self.assertEqual(payload["status"], "ready")
        self.assertEqual(payload["points"][0]["latitude"], 31.2304)
        self.assertEqual(payload["points"][0]["longitude"], 121.4737)
        self.assertEqual(result.content[0].text, json.dumps(structured_content, ensure_ascii=False, separators=(",", ":")))

    async def test_create_map_card_supports_artifact_backed_loading_marker(self) -> None:
        result = await server.create_map_card(
            title="路线地图",
            intent="route",
            input_ref="tool-result-1",
            fallback_text="已生成路线地图。",
        )

        self.assertIsNotNone(result.structuredContent)
        payload = result.structuredContent["card"]
        self.assertEqual(payload["status"], "loading")
        self.assertEqual(payload["input_ref"], "tool-result-1")
        self.assertIn('"input_ref":"tool-result-1"', result.structuredContent["marker"])

    async def test_create_map_card_advertises_and_validates_its_output_schema(self) -> None:
        tools = await server.mcp.list_tools()
        tool = next(tool for tool in tools if tool.name == "create_map_card")

        self.assertIsNotNone(tool.outputSchema)
        assert tool.outputSchema is not None
        self.assertEqual(set(tool.outputSchema["required"]), {"type", "kind", "marker", "card"})

        result = await server.mcp.call_tool(
            "create_map_card",
            {
                "title": "上海点位",
                "points": [{"latitude": 31.2304, "longitude": 121.4737}],
            },
        )
        self.assertIsInstance(result, CallToolResult)
        self.assertIsNotNone(result.structuredContent)

    def test_map_card_result_rejects_a_marker_that_does_not_match_the_card(self) -> None:
        card = server.MapCardPayload(
            title="上海点位",
            intent="location",
            status="ready",
            points=[server.MapCardPoint(latitude=31.2304, longitude=121.4737)],
        )

        with self.assertRaises(ValidationError):
            server.MapCardToolResult(
                type="open-web-card",
                kind="map.v1",
                marker="```open-web-card map.v1\n{}\n```",
                card=card,
            )


if __name__ == "__main__":
    unittest.main()
