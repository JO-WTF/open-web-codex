// @vitest-environment jsdom
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { InlineVisualizationArtifact } from "../../../utils/replyCards";
import AssistantMessage from "./AssistantMessage";

vi.mock("./ReplyCard", () => ({
  default: ({ card }: { card: { title: string } }) => (
    <div className="web-map-card" data-testid="inline-reply-card">{card.title}</div>
  ),
}));

const mapArtifact: InlineVisualizationArtifact = {
  ref: "map-one",
  rendererKind: "map.v3",
  card: {
    type: "card",
    kind: "map.v3",
    id: "map-one",
    title: "上海地图",
    intent: "show Shanghai",
    status: "ready",
    viewport: { mode: "fit" },
    sources: [],
    layers: [],
  },
};

const reportArtifact: InlineVisualizationArtifact = {
  ref: "report-one",
  rendererKind: "report.v1",
  card: {
    type: "card",
    kind: "report.v1",
    id: "report-one",
    title: "Network decision",
    status: "ready",
    source: {
      type: "artifact",
      format: "json",
      artifactId: "8e98ff2f-82ee-4cc9-a3e6-2974debf8666",
      mimeType: "application/json",
      url: "/api/artifacts/8e98ff2f-82ee-4cc9-a3e6-2974debf8666/content",
    },
  },
};

describe("AssistantMessage", () => {
  it("renders GitHub-flavored Markdown without rendering raw HTML", () => {
    render(<AssistantMessage text={'# Heading\n\n- **bold**\n\n`code`\n\n<script>alert(1)</script>'} />);
    expect(screen.getByRole("heading", { name: "Heading" })).toBeTruthy();
    expect(screen.getByText("bold").tagName).toBe("STRONG");
    expect(screen.getByText("code").tagName).toBe("CODE");
    expect(document.querySelector("script")).toBeNull();
  });

  it("opens workspace file links in the file manager", () => {
    const onOpenFile = vi.fn();
    render(<AssistantMessage text="Open [config](./src/config.ts)" onOpenFile={onOpenFile} />);
    fireEvent.click(screen.getByRole("link", { name: "config" }));
    expect(onOpenFile).toHaveBeenCalledWith("src/config.ts");
  });

  it("marks external links to open in a new window", () => {
    render(<AssistantMessage text="Read [the docs](https://example.com/docs)." />);

    const link = screen.getByRole("link", { name: "the docs" });
    expect(link.getAttribute("target")).toBe("_blank");
    expect(link.getAttribute("rel")).toBe("noopener noreferrer");
    expect(link.classList.contains("web-external-link")).toBe(true);
  });

  it("does not render a cursor inside streaming replies", () => {
    const view = render(<AssistantMessage text="Still working" streaming />);

    expect(view.container.querySelector(".web-streaming-cursor")).toBeNull();
  });

  it("does not flash an incomplete or unresolved Artifact directive while streaming", () => {
    const view = render(
      <AssistantMessage
        text={'Before\n::codex-inline-vis{artifact="map'}
        streaming
      />,
    );

    expect(view.container.textContent).toBe("Before");
    expect(screen.queryByText("Visualization unavailable")).toBeNull();
  });

  it("marks commentary as process content instead of a reply bubble", () => {
    const view = render(<AssistantMessage text="Checking the files" variant="commentary" />);

    expect(view.container.querySelector(".web-msg-commentary")).toBeTruthy();
    expect(view.container.querySelector(".web-msg-commentary-body")).toBeTruthy();
  });

  it("composes Markdown and an Artifact as ordered children of one reply container", () => {
    const view = render(
      <AssistantMessage
        text={'地图之前\n\n::codex-inline-vis{artifact="map-one"}\n\n地图之后'}
        inlineArtifacts={[mapArtifact]}
      />,
    );

    const body = view.container.querySelector(".web-msg-assistant-body");
    expect(body).toBeTruthy();
    expect(view.container.querySelectorAll(".web-msg-assistant-body")).toHaveLength(1);
    expect(Array.from(body!.children).map((child) => child.textContent)).toEqual([
      "地图之前",
      "上海地图",
      "地图之后",
    ]);
    expect(body!.querySelector(":scope > .web-map-card")).toBeTruthy();
    expect(view.container.querySelector(".web-msg-markdown-segment")).toBeNull();
  });

  it("keeps a standalone Artifact inside the rounded reply container", () => {
    const view = render(
      <AssistantMessage
        text={'::codex-inline-vis{artifact="map-one"}'}
        inlineArtifacts={[mapArtifact]}
      />,
    );

    const body = view.container.querySelector(".web-msg-assistant-body");
    expect(body?.children).toHaveLength(1);
    expect(body?.firstElementChild).toBe(
      view.container.querySelector("[data-testid='inline-reply-card']"),
    );
    expect(view.container.querySelector(".web-msg-assistant")?.classList).toContain(
      "has-inline-visualization",
    );
  });

  it("places a typed report Artifact at its referenced message position", () => {
    const view = render(
      <AssistantMessage
        text={[
          "Report follows.",
          '::codex-inline-vis{artifact="report-one"}',
        ].join("\n")}
        inlineArtifacts={[reportArtifact]}
      />,
    );

    const body = view.container.querySelector(".web-msg-assistant-body");
    expect(Array.from(body!.children).map((child) => child.textContent)).toEqual([
      "Report follows.",
      "Network decision",
    ]);
  });

  it("suppresses only duplicate Artifact refs while preserving the message and new deliveries", () => {
    const view = render(
      <AssistantMessage
        text={[
          "Deliveries follow.",
          '::codex-inline-vis{artifact="map-one"}',
          '::codex-inline-vis{artifact="report-one"}',
        ].join("\n")}
        inlineArtifacts={[mapArtifact, reportArtifact]}
        hiddenInlineArtifactRefs={["map-one"]}
      />,
    );

    expect(view.container.textContent).toContain("Deliveries follow.");
    expect(view.container.textContent).not.toContain("上海地图");
    expect(view.container.textContent).toContain("Network decision");
    expect(view.container.textContent).not.toContain("Visualization unavailable");
  });

  it("does not promote model-authored report JSON into a ReplyCard", () => {
    const view = render(
      <AssistantMessage
        text={'{"schema_version":"indonesia_decision_report.v1","markdown":"# Untrusted"}'}
      />,
    );

    expect(
      view.container.querySelector('[data-testid="inline-reply-card"]'),
    ).toBeNull();
    expect(screen.getByText(/indonesia_decision_report\.v1/)).toBeTruthy();
  });
});
