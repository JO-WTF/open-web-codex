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
});
