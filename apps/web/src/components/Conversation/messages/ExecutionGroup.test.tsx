// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import ExecutionGroup from "./ExecutionGroup";

describe("ExecutionGroup", () => {
  afterEach(() => {
    cleanup();
    vi.useRealTimers();
  });

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

  it("collapses completed history in the same render without an expanded frame", () => {
    const items = [{
      id: "tool-1",
      level: "info" as const,
      kind: "tool" as const,
      text: "batch_geocode",
    }];
    const view = render(
      <ExecutionGroup items={items} active timelineItemCount={1}>
        <div>Historical tool details</div>
      </ExecutionGroup>,
    );

    expect(screen.getByText("Historical tool details")).toBeTruthy();
    view.rerender(
      <ExecutionGroup items={items} active={false} timelineItemCount={1}>
        <div>Historical tool details</div>
      </ExecutionGroup>,
    );

    expect(screen.queryByText("Historical tool details")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "1 tool call, 0 messages" }));
    expect(screen.getByText("Historical tool details")).toBeTruthy();
  });

  it("shows the official completed Turn duration after the collapsed counts", () => {
    render(
      <ExecutionGroup
        items={[
          {
            id: "tool-1",
            level: "info",
            kind: "tool",
            text: "assess_facility_change",
          },
          {
            id: "message-1",
            level: "assistant",
            text: "Running the assessment.",
          },
        ]}
        active={false}
        durationMs={48_318}
        timelineItemCount={2}
      >
        <div>Completed activity</div>
      </ExecutionGroup>,
    );

    expect(screen.getByRole("button", {
      name: "1 tool call, 1 message · 0:48",
    })).toBeTruthy();
  });

  it("adds the live Turn elapsed time to an active execution summary", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-07-14T00:01:03Z"));
    render(
      <ExecutionGroup
        items={[{
          id: "tool-1",
          level: "info",
          kind: "tool",
          text: "assess_facility_change",
        }]}
        active
        startedAt={Date.parse("2026-07-14T00:00:00Z")}
        timelineItemCount={1}
      >
        <div>Live activity</div>
      </ExecutionGroup>,
    );

    expect(screen.getByRole("button", {
      name: "1 tool call, 0 messages · 1:03",
    })).toBeTruthy();
  });

  it("keeps the working indicator beside a live activity card", () => {
    render(
      <ExecutionGroup
        items={[]}
        active
        activeItem={<div>Tool is running</div>}
      >
        {null}
      </ExecutionGroup>,
    );

    expect(screen.getByText("Tool is running")).toBeTruthy();
    expect(screen.getByText("Working…")).toBeTruthy();
  });

  it("does not count approval cards as tool calls", () => {
    render(
      <ExecutionGroup
        items={[
          {
            id: "tool-1",
            level: "info",
            kind: "tool",
            text: "batch_geocode",
          },
          {
            id: "approval-1",
            level: "info",
            kind: "approval",
            text: "Allow batch_geocode?",
            approvalStatus: "resolved",
          },
        ]}
        active={false}
        timelineItemCount={2}
      >
        <div>Timeline</div>
      </ExecutionGroup>,
    );

    expect(screen.getByRole("button", { name: "1 tool call, 0 messages" })).toBeTruthy();
  });
});
