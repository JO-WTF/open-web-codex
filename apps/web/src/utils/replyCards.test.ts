import { describe, expect, it } from "vitest";
import { parseReplyCards } from "./replyCards";

describe("parseReplyCards", () => {
  it("splits text around an open-web map card marker", () => {
    const parts = parseReplyCards('Before\n```open-web-card map.v1\n{"title":"Route","intent":"route","input_ref":"ref-1","fallback_text":"Fallback"}\n```\nAfter');

    expect(parts).toHaveLength(3);
    expect(parts[0]).toMatchObject({ type: "text", content: "Before" });
    expect(parts[1]).toMatchObject({
      type: "card",
      kind: "map.v1",
      title: "Route",
      intent: "route",
      inputRef: "ref-1",
      fallbackText: "Fallback",
      status: "loading",
    });
    expect(parts[2]).toMatchObject({ type: "text", content: "\nAfter" });
  });

  it("hydrates legacy stored map widget markers as map cards", () => {
    const parts = parseReplyCards('```widget\n{"widget_type":"map","id":"map-1","props":{"use_stored_card":true}}\n```');

    expect(parts).toHaveLength(1);
    expect(parts[0]).toMatchObject({
      type: "card",
      kind: "map.v1",
      id: "map-1",
      summary: "地图数据已存储在服务端，等待平台 Artifact hydration。",
    });
  });

  it("supports multiple cards in one reply", () => {
    const parts = parseReplyCards('A\n```open-web-card map.v1\n{"title":"One"}\n```\nB\n```open-web-card map.v1\n{"title":"Two"}\n```');

    expect(parts.filter((part) => part.type === "card")).toHaveLength(2);
    expect(parts.map((part) => part.type === "card" ? part.title : part.content.trim())).toEqual(["A", "One", "B", "Two"]);
  });

  it("normalizes point, line, polygon, bbox and GeoJSON payloads for one card", () => {
    const [part] = parseReplyCards('```open-web-card map.v1\n{"title":"Mixed","bbox":[0,0,3,4],"points":[{"lat":1,"lng":2}],"lines":[{"coordinates":[[2,1],[3,4]]}],"polygons":[{"coordinates":[[[0,0],[1,0],[1,1],[0,0]]]}],"geojson":{"type":"FeatureCollection","features":[]}}\n```');

    expect(part).toMatchObject({
      type: "card",
      title: "Mixed",
      status: "ready",
      bbox: [0, 0, 3, 4],
      points: [{ latitude: 1, longitude: 2 }],
      lines: [{ coordinates: [[2, 1], [3, 4]] }],
      polygons: [{ coordinates: [[[0, 0], [1, 0], [1, 1], [0, 0]]] }],
      geojson: { type: "FeatureCollection", features: [] },
    });
  });

  it("keeps invalid card fences as markdown text", () => {
    const text = '```open-web-card map.v1\n{"title":\n```';
    expect(parseReplyCards(text)).toEqual([{ type: "text", content: text }]);
  });
});
