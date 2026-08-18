// @vitest-environment jsdom
import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import ReasoningBlock from "./ReasoningBlock";

describe("ReasoningBlock", () => {
  it("shows completed reasoning collapsed by default and can be expanded", () => {
    render(<ReasoningBlock summary="Reviewing repository state" text="Checking the workspace before running git." />);

    expect(screen.getByText("Reviewing repository state")).toBeTruthy();
    expect(screen.queryByText("Checking the workspace before running git.")).toBeNull();
    fireEvent.click(screen.getByRole("button"));
    expect(screen.getByText("Checking the workspace before running git.")).toBeTruthy();
  });

  it("automatically collapses the corresponding reasoning when streaming completes", () => {
    const view = render(
      <ReasoningBlock
        summary="Reviewing repository state"
        text="Inspecting the repository."
        streaming
      />,
    );
    const reasoning = within(view.container);

    expect(reasoning.getByText("Reviewing repository state")).toBeTruthy();
    expect(reasoning.getByText("Inspecting the repository.")).toBeTruthy();
    expect(view.container.querySelector(".web-reasoning-working")).toBeTruthy();

    view.rerender(
      <ReasoningBlock
        summary="Reviewing repository state"
        text="Inspecting the repository."
      />,
    );

    expect(reasoning.queryByText("Inspecting the repository.")).toBeNull();
    expect(view.container.querySelector(".web-reasoning-working")).toBeNull();
    expect(reasoning.getByRole("button").getAttribute("aria-expanded")).toBe("false");

    fireEvent.click(reasoning.getByRole("button"));
    expect(reasoning.getByText("Inspecting the repository.")).toBeTruthy();
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
