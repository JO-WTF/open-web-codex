// @vitest-environment jsdom
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import GoalBanner from "./GoalBanner";

describe("GoalBanner", () => {
  it("renders an active goal summary and expands usage details", () => {
    render(
      <GoalBanner
        goal={{
          objective: "Ship the browser conversation experience",
          status: "active",
          tokenBudget: 10_000,
          tokensUsed: 2_500,
          timeUsedSeconds: 125,
        }}
      />,
    );

    expect(screen.getByText("Ship the browser conversation experience")).toBeTruthy();
    expect(screen.getByText("Active")).toBeTruthy();
    expect(screen.getByText("25%")).toBeTruthy();
    expect(screen.queryByText("Time used")).toBeNull();

    fireEvent.click(screen.getByText("Ship the browser conversation experience"));
    expect(screen.getByText("Time used")).toBeTruthy();
    expect(screen.getByText("2m 5s")).toBeTruthy();
    expect(screen.getByText("2.5K")).toBeTruthy();
    expect(document.querySelector(".web-goal-detail-sub")?.textContent).toBe(" / 10.0K budget");
  });

  it("does not render when no goal is active", () => {
    const { container } = render(<GoalBanner goal={null} />);
    expect(container.innerHTML).toBe("");
  });
});
