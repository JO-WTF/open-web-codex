import { describe, expect, it } from "vitest";
import { parseStructuredMapReplyCard } from "./replyCards";

describe("parseStructuredMapReplyCard", () => {
  it("normalizes the Server-projected MCP structuredContent contract", () => {
    const card = parseStructuredMapReplyCard({
      type: "open-web-card",
      kind: "map.v2",
      card: {
        title: "Batch geocode",
        intent: "visualization",
        status: "ready",
        fallback_text: "Three locations",
        viewport: { mode: "camera", center: [-122.08, 37.42], zoom: 10 },
        sources: [{
          id: "locations",
          data: {
            type: "inline",
            format: "geojson",
            geojson: { type: "FeatureCollection", features: [] },
          },
        }],
        layers: [{
          id: "points",
          source: "locations",
          geometry: "point",
          label_property: "label",
          style: {
            color: "#ef4444",
            opacity: 0.8,
            radius: 9,
            stroke_color: "#ffffff",
            stroke_width: 2,
          },
        }],
      },
    });

    expect(card).toMatchObject({
      type: "card",
      kind: "map.v2",
      title: "Batch geocode",
      fallbackText: "Three locations",
      status: "ready",
      viewport: { mode: "camera", center: [-122.08, 37.42], zoom: 10 },
      layers: [{
        geometry: "point",
        labelProperty: "label",
        style: {
          color: "#ef4444",
          opacity: 0.8,
          radius: 9,
          strokeColor: "#ffffff",
          strokeWidth: 2,
        },
      }],
    });
  });

  it("does not interpret MCP text or another card kind as structured map.v2", () => {
    expect(parseStructuredMapReplyCard({
      content: [{
        type: "text",
        text: "{\"type\":\"open-web-card\",\"kind\":\"map.v2\",\"card\":{}}",
      }],
    })).toBeNull();
    expect(parseStructuredMapReplyCard({
      type: "open-web-card",
      kind: "chart.v1",
      card: {},
    })).toBeNull();
  });
});
