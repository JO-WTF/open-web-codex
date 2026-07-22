// @vitest-environment jsdom
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import CommandExecutionCard from "./CommandExecutionCard";

describe("CommandExecutionCard", () => {
  it("updates one running command card as output and completion arrive", () => {
    const view = render(
      <CommandExecutionCard command="sleep 5 && echo done" status="inProgress" />,
    );

    expect(screen.getByText("running")).toBeTruthy();
    expect(view.container.querySelector(".web-cmdex-status")?.getAttribute("aria-live")).toBe("polite");
    expect(view.container.querySelector(".web-cmdex-card.is-running")).toBeTruthy();

    view.rerender(
      <CommandExecutionCard
        command="sleep 5 && echo done"
        status="inProgress"
        output={"done\n"}
      />,
    );
    expect(view.container.querySelector(".web-cmdex-output")?.textContent).toBe("done\n");

    view.rerender(
      <CommandExecutionCard
        command="sleep 5 && echo done"
        status="completed"
        output={"done\n"}
        exitCode={0}
        durationMs={5000}
      />,
    );
    expect(screen.getByText("✓ OK")).toBeTruthy();
    expect(screen.queryByText("running")).toBeNull();
    expect(view.container.querySelector(".web-cmdex-card.is-completed")).toBeTruthy();
  });

  it("does not show an invalid zero duration", () => {
    render(<CommandExecutionCard command="ls -la" exitCode={0} durationMs={0} />);
    expect(screen.queryByText("0ms")).toBeNull();
  });
});
