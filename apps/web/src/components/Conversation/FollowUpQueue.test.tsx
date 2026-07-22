// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import FollowUpQueue from "./FollowUpQueue";

afterEach(cleanup);

describe("FollowUpQueue", () => {
  it("stacks queued messages and supports steer and delete", () => {
    const onSteer = vi.fn();
    const onDelete = vi.fn();
    render(
      <FollowUpQueue
        items={[{ id: "q1", text: "Add a stop button" }, { id: "q2", text: "Then update tests" }]}
        canSteer
        steeringId={null}
        onSteer={onSteer}
        onDelete={onDelete}
      />,
    );

    expect(screen.getByText("Add a stop button")).toBeTruthy();
    expect(screen.getByText("Then update tests")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Steer now: Add a stop button" }));
    fireEvent.click(screen.getByRole("button", { name: "Delete queued message: Then update tests" }));
    expect(onSteer).toHaveBeenCalledWith("q1");
    expect(onDelete).toHaveBeenCalledWith("q2");
  });
});
