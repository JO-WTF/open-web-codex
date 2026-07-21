// @vitest-environment jsdom
import { fireEvent, render, screen } from "@testing-library/react";
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

  it("renders map card markers without showing the raw fenced block", () => {
    render(<AssistantMessage text={'Intro\n```open-web-card map.v1\n{"title":"Route","intent":"route","input_ref":"ref-1"}\n```\nDone'} />);

    expect(screen.getByText("Intro")).toBeTruthy();
    expect(screen.getByText("Route")).toBeTruthy();
    expect(screen.getByText("Intent")).toBeTruthy();
    expect(screen.getByText("route")).toBeTruthy();
    expect(screen.getByText("Input ref")).toBeTruthy();
    expect(screen.getByText("ref-1")).toBeTruthy();
    expect(screen.getByText("Done")).toBeTruthy();
    expect(screen.queryByText(/open-web-card/)).toBeNull();
  });
});
