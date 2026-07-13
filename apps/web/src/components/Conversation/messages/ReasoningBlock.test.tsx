// @vitest-environment jsdom
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import ReasoningBlock from "./ReasoningBlock";

describe("ReasoningBlock", () => {
  it("shows completed reasoning by default and can be collapsed", () => {
    render(<ReasoningBlock text="Checking the workspace before running git." />);

    expect(screen.getAllByText("Checking the workspace before running git.")).toHaveLength(2);
    fireEvent.click(screen.getByRole("button"));
    expect(screen.getAllByText("Checking the workspace before running git.")).toHaveLength(1);
  });

  it("shows a working state while reasoning is streaming", () => {
    const view = render(<ReasoningBlock text="Inspecting the repository." streaming />);

    expect(screen.getAllByText("Inspecting the repository.")).toHaveLength(2);
    expect(view.container.querySelector(".web-reasoning-working")).toBeTruthy();
  });

  it("renders an empty completed reasoning item as a single row", () => {
    render(<ReasoningBlock text="Reasoning completed" />);

    expect(screen.getAllByText("Reasoning")).toHaveLength(1);
  });
});
