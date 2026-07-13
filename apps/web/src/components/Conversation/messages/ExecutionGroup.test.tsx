// @vitest-environment jsdom
import { act, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import ExecutionGroup from "./ExecutionGroup";

describe("ExecutionGroup", () => {
  afterEach(() => vi.useRealTimers());

  it("shows elapsed time from the actual turn start", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-07-14T00:01:03Z"));
    render(
      <ExecutionGroup items={[]} active startedAt={Date.parse("2026-07-14T00:00:00Z")}>
        <div>Earlier activity</div>
      </ExecutionGroup>,
    );

    expect(screen.getByText("1:03")).toBeTruthy();
    act(() => vi.advanceTimersByTime(2_000));
    expect(screen.getByText("1:05")).toBeTruthy();
  });
});
