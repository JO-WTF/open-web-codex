// @vitest-environment jsdom
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import ReasoningBlock from "./ReasoningBlock";

describe("ReasoningBlock", () => {
  it("shows completed reasoning by default and can be collapsed", () => {
    render(<ReasoningBlock summary="Reviewing repository state" text="Checking the workspace before running git." />);

    expect(screen.getByText("Reviewing repository state")).toBeTruthy();
    expect(screen.getByText("Checking the workspace before running git.")).toBeTruthy();
    fireEvent.click(screen.getByRole("button"));
    expect(screen.queryByText("Checking the workspace before running git.")).toBeNull();
  });

  it("shows a working state while reasoning is streaming", () => {
    const view = render(<ReasoningBlock text="Inspecting the repository." streaming />);

    expect(screen.getByText("Inspecting the repository.")).toBeTruthy();
    expect(view.container.querySelector(".web-reasoning-working")).toBeTruthy();
  });

  it("uses actual reasoning content instead of a fixed completion label", () => {
    render(<ReasoningBlock text="Analyzing the responsive layout constraints." />);

    expect(screen.getByText("Analyzing the responsive layout constraints.")).toBeTruthy();
    expect(screen.queryByText("Reasoning completed")).toBeNull();
  });

  it("does not render a completed reasoning item without visible content", () => {
    const view = render(<ReasoningBlock text="Reasoning completed" />);

    expect(view.container.innerHTML).toBe("");
  });
});
