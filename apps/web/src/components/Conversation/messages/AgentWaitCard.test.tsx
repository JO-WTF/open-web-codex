// @vitest-environment jsdom
import { cleanup, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import AgentWaitCard from "./AgentWaitCard";

afterEach(cleanup);

describe("AgentWaitCard", () => {
  it("renders an active Runtime fallback as running rather than starting", () => {
    render(
      <AgentWaitCard
        status="inProgress"
        agentUpdates={[{
          threadId: "child-thread",
          agentLabel: "Wanwan",
          text: "No recorded History item yet.",
          status: "active",
          kind: null,
        }]}
      />,
    );

    const history = screen.getByLabelText("Latest Agent History items");
    expect(within(history).getByText("Running")).toBeTruthy();
    expect(within(history).queryByText("Starting")).toBeNull();
  });

  it("holds a live reasoning Item at Thinking until it completes", () => {
    render(
      <AgentWaitCard
        status="inProgress"
        agentUpdates={[{
          threadId: "child-thread",
          agentLabel: "Wanwan",
          text: "Reasoning: partial streamed text must not be shown.",
          status: "running",
          kind: "reasoning",
        }]}
      />,
    );

    const history = screen.getByLabelText("Latest Agent History items");
    expect(within(history).getAllByText("Thinking")).toHaveLength(2);
    expect(within(history).queryByText(/partial streamed text/)).toBeNull();
  });
});
