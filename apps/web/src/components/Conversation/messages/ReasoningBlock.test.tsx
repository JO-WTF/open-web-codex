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
});
