// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import GoalBanner from "./GoalBanner";

afterEach(cleanup);

describe("GoalBanner", () => {
  it("renders goal progress and expands plan steps", () => {
    const view = render(
      <GoalBanner
        goal={{
          objective: "Ship the browser conversation experience",
          status: "active",
          tokenBudget: 10_000,
          tokensUsed: 2_500,
          timeUsedSeconds: 125,
          steps: [
            { step: "Inspect event fields", status: "completed" },
            { step: "Implement the goal banner", status: "inProgress" },
            { step: "Run tests", status: "pending" },
          ],
          fileCount: 5,
          additions: 89,
          deletions: 9,
        }}
      />,
    );

    expect(screen.getByText("Step 2 / 3")).toBeTruthy();
    expect(screen.getByText("· 5 files changed")).toBeTruthy();
    expect(screen.getByText("+89")).toBeTruthy();
    expect(screen.getByText("-9")).toBeTruthy();
    expect(screen.queryByText("Inspect event fields")).toBeNull();

    fireEvent.mouseEnter(view.container.querySelector(".web-goal-banner")!);
    expect(screen.getByText("Inspect event fields")).toBeTruthy();
    expect(screen.getByText("Implement the goal banner")).toBeTruthy();
    expect(screen.getByText("Run tests")).toBeTruthy();
  });

  it("renders an empty panel when no goal is active", () => {
    render(<GoalBanner goal={null} />);
    const button = screen.getByRole("button", { name: "No active goal" });
    expect(button.getAttribute("aria-disabled")).toBe("true");
    expect(button.getAttribute("aria-expanded")).toBe("false");
    expect(screen.queryByRole("list", { name: "Goal steps" })).toBeNull();
  });
});
