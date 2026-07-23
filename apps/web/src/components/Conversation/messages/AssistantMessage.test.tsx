// @vitest-environment jsdom
import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import AssistantMessage from "./AssistantMessage";

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

  it("renders map card markers without showing the raw fenced block", () => {
    const view = render(<AssistantMessage text={'Intro\n```open-web-card map.v1\n{"title":"Route","intent":"route","input_ref":"ref-1","points":[{"lat":31.2,"lng":121.5,"label":"上海"}]}\n```\nDone'} />);

    expect(screen.getByText("Intro")).toBeTruthy();
    expect(screen.getByText("Route")).toBeTruthy();
    expect(screen.getByText("Intent")).toBeTruthy();
    expect(screen.getByText("route")).toBeTruthy();
    expect(screen.getByText("Input ref")).toBeTruthy();
    expect(screen.getByText("ref-1")).toBeTruthy();
    const map = screen.getByLabelText("Interactive Mapbox map");
    expect(map.getAttribute("data-map-engine")).toBe("mapbox-gl");
    expect(screen.getByTestId("map-placeholder-background")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Open map card fullscreen" }).textContent).toContain("全屏");
    expect(screen.getByText("Done")).toBeTruthy();
    expect(screen.queryByText(/open-web-card/)).toBeNull();
    expect(
      view.container.querySelector(".web-msg-assistant-body-map"),
    ).toBeTruthy();
  });

  it("opens Mapbox configuration from a card that has no token", () => {
    const view = render(<AssistantMessage text={'```open-web-card map.v1\n{"title":"Map setup","points":[{"lat":31.2,"lng":121.5}]}\n```'} />);
    const rendered = within(view.container);

    fireEvent.click(rendered.getByRole("button", { name: "配置 Mapbox Key" }));
    expect(
      rendered.getByRole("dialog", { name: "配置地图服务 Key" }),
    ).toBeTruthy();
    expect(rendered.getByRole("button", { name: "Mapbox" }).getAttribute("aria-pressed"))
      .toBe("true");
    expect(rendered.getByRole("button", { name: "Google" })).toBeTruthy();
    const input = rendered.getByLabelText("Mapbox public token");
    fireEvent.change(input, { target: { value: "sk.not-a-mapbox-public-token" } });
    fireEvent.click(rendered.getByRole("button", { name: "保存配置" }));
    expect(
      rendered.getByText(/请输入以 pk\. 开头/),
    ).toBeTruthy();
  });
});
