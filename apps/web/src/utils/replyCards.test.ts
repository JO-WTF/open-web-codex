import { describe, expect, it } from "vitest";
import { parseReplyCards } from "./replyCards";

describe("parseReplyCards", () => {
  it("extracts open-web map card markers and preserves surrounding text", () => {
    const parts = parseReplyCards('Intro\n```open-web-card map.v1\n{"title":"Route","intent":"route","input_ref":"ref-1","points":[{"lat":31.2,"lng":121.5,"label":"上海"}]}\n```\nDone');

    expect(parts[0]).toEqual({ type: "text", content: "Intro" });
    expect(parts[1]).toMatchObject({
      type: "card",
      kind: "map.v1",
      title: "Route",
      intent: "route",
      inputRef: "ref-1",
      status: "ready",
      points: [{ latitude: 31.2, longitude: 121.5, label: "上海" }],
    });
    expect(parts[2]).toEqual({ type: "text", content: "\nDone" });
  });

  it("supports legacy widget map markers emitted by older map-card skills", () => {
    const parts = parseReplyCards('```widget\n{"id":"map-legacy","widget_type":"map","props":{"title":"Stored","input_ref":"artifact-1","use_stored_card":true}}\n```');

    expect(parts).toHaveLength(1);
    expect(parts[0]).toMatchObject({
      type: "card",
      id: "map-legacy",
      title: "Stored",
      inputRef: "artifact-1",
      status: "loading",
      summary: "地图数据已存储在服务端，等待平台 Artifact hydration。",
    });
  });
});
