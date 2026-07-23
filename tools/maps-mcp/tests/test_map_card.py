from __future__ import annotations

import json
import unittest

import maps_mcp.server as server


class MapCardTests(unittest.IsolatedAsyncioTestCase):
    async def test_create_map_card_returns_open_web_marker(self) -> None:
        result = await server.create_map_card(
            title="上海点位",
            intent="location",
            points=[server.MapCardPoint(latitude=31.2304, longitude=121.4737, label="上海")],
            summary="一个可渲染点位",
        )

        self.assertEqual(result["type"], "open-web-card")
        self.assertEqual(result["kind"], "map.v1")
        marker = result["marker"]
        self.assertTrue(marker.startswith("```open-web-card map.v1\n"))
        self.assertTrue(marker.endswith("\n```"))
        payload = json.loads(marker.split("\n", 1)[1].rsplit("\n", 1)[0])
        self.assertEqual(payload["title"], "上海点位")
        self.assertEqual(payload["status"], "ready")
        self.assertEqual(payload["points"][0]["latitude"], 31.2304)
        self.assertEqual(payload["points"][0]["longitude"], 121.4737)

    async def test_create_map_card_supports_artifact_backed_loading_marker(self) -> None:
        result = await server.create_map_card(
            title="路线地图",
            intent="route",
            input_ref="tool-result-1",
            fallback_text="已生成路线地图。",
        )

        payload = result["card"]
        self.assertEqual(payload["status"], "loading")
        self.assertEqual(payload["input_ref"], "tool-result-1")
        self.assertIn('"input_ref":"tool-result-1"', result["marker"])


if __name__ == "__main__":
    unittest.main()
