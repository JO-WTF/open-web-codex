// @vitest-environment jsdom
import { act, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import InlineVisualizationFile from "./InlineVisualizationFile";

const { readInlineVisualization } = vi.hoisted(() => ({
  readInlineVisualization: vi.fn(),
}));

vi.mock("../../../../browser/session", () => ({
  platformClient: { readInlineVisualization },
}));

describe("InlineVisualizationFile", () => {
  beforeEach(() => {
    vi.stubGlobal("URL", {
      createObjectURL: vi.fn(() => "blob:visualization"),
      revokeObjectURL: vi.fn(),
    });
    readInlineVisualization.mockResolvedValue({
      blob: new Blob(["content"]),
      contentType: "text/html",
    });
  });

  afterEach(() => {
    vi.clearAllMocks();
    vi.unstubAllGlobals();
  });

  it("renders native HTML in a script-only sandbox", async () => {
    render(
      <InlineVisualizationFile
        threadId="0198ff2f-82ee-7cc9-a3e6-2974debf8666"
        file="chart.html"
        media="html"
      />,
    );

    const frame = await screen.findByTitle("chart.html visualization");
    expect(frame.getAttribute("sandbox")).toBe("allow-scripts");
    expect(frame.getAttribute("sandbox")).not.toContain("allow-same-origin");
    expect(frame.getAttribute("src")).toBe("blob:visualization");
  });

  it("renders static raster content as an image and revokes its URL", async () => {
    const view = render(
      <InlineVisualizationFile
        threadId="0198ff2f-82ee-7cc9-a3e6-2974debf8666"
        file="chart.png"
        media="image"
      />,
    );
    await screen.findByRole("img", { name: "chart.png visualization" });

    act(() => view.unmount());
    expect(URL.revokeObjectURL).toHaveBeenCalledWith("blob:visualization");
  });

  it("shows an explicit terminal error", async () => {
    readInlineVisualization.mockRejectedValueOnce(new Error("unavailable"));
    render(
      <InlineVisualizationFile
        threadId="0198ff2f-82ee-7cc9-a3e6-2974debf8666"
        file="chart.html"
        media="html"
      />,
    );
    await waitFor(() => expect(screen.getByRole("alert").textContent)
      .toContain("Visualization unavailable"));
  });
});
