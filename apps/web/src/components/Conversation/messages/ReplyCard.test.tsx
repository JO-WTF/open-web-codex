// @vitest-environment jsdom

import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type {
  MapReplyCard,
  ReportReplyCard,
} from "../../../utils/replyCards";
import ReplyCard from "./ReplyCard";

vi.mock("./MapReplyCard", () => ({
  default: () => <div data-testid="map-renderer" />,
}));

vi.mock("./ReportReplyCard", () => ({
  default: () => <div data-testid="report-renderer" />,
}));

describe("ReplyCard", () => {
  it("dispatches each typed renderer kind to its owning component", () => {
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
    const reportCard: ReportReplyCard = {
      type: "card",
      kind: "report.v1",
      id: "report",
      title: "Report",
      status: "ready",
      source: {
        type: "artifact",
        format: "json",
        artifactId: "8e98ff2f-82ee-4cc9-a3e6-2974debf8666",
        mimeType: "application/json",
        url: "/api/artifacts/8e98ff2f-82ee-4cc9-a3e6-2974debf8666/content",
      },
    };

    const view = render(<ReplyCard card={mapCard} />);
    expect(screen.getByTestId("map-renderer")).toBeTruthy();

    view.rerender(<ReplyCard card={reportCard} />);
    expect(screen.getByTestId("report-renderer")).toBeTruthy();
  });
});
