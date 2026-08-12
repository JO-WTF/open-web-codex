// @vitest-environment jsdom

import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { MapReplyCard } from "../../../utils/replyCards";
import ReplyCard from "./ReplyCard";

vi.mock("./MapReplyCard", () => ({
  default: () => <div data-testid="map-renderer" />,
}));

describe("ReplyCard", () => {
  it("renders the retained map card", () => {
    const mapCard: MapReplyCard = {
      type: "card",
      kind: "map.v3",
      id: "map",
      title: "Map",
      intent: "show a map",
      status: "ready",
      viewport: { mode: "fit" },
      sources: [],
      layers: [],
    };
    render(<ReplyCard card={mapCard} />);
    expect(screen.getByTestId("map-renderer")).toBeTruthy();
  });
});
