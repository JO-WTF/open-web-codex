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

  it("keeps invalid card fences as markdown text", () => {
    const text = '```open-web-card map.v1\n{"title":\n```';
    expect(parseReplyCards(text)).toEqual([{ type: "text", content: text }]);
  });
});
