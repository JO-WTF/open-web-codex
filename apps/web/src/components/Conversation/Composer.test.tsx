// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import Composer from "./Composer";

afterEach(cleanup);

describe("Web Composer context usage", () => {
  it("renders real context usage and hover details from token usage", () => {
    render(
      <Composer
        draft=""
        onDraftChange={vi.fn()}
        onSend={vi.fn()}
        onStop={vi.fn()}
        running={false}
        stopping={false}
        busy={false}
        disabled={false}
        tokenUsage={{
          total: {
            totalTokens: 42_000,
            inputTokens: 38_000,
            cachedInputTokens: 0,
            outputTokens: 4_000,
            reasoningOutputTokens: 0,
          },
          last: {
            totalTokens: 32_000,
            inputTokens: 30_000,
            cachedInputTokens: 0,
            outputTokens: 2_000,
            reasoningOutputTokens: 0,
          },
          modelContextWindow: 128_000,
        }}
      />,
    );

    const indicator = screen.getByLabelText(
      "Context used 25%: 32,000 of 128,000 tokens",
    );
    expect(indicator).not.toBeNull();
    const ringStyle = indicator
      .querySelector(".web-composer-activity-ring")
      ?.getAttribute("style") ?? "";
    expect(ringStyle).toContain("--context-used: 25");
    expect(ringStyle).toContain("--context-color: #82e63e");
    expect(screen.getByText("32,000 / 128,000 tokens")).not.toBeNull();
    expect(screen.getByText("Input 30,000 · Output 2,000")).not.toBeNull();
  });

  it("does not invent usage when the context window is unavailable", () => {
    render(
      <Composer
        draft=""
        onDraftChange={vi.fn()}
        onSend={vi.fn()}
        onStop={vi.fn()}
        running={false}
        stopping={false}
        busy={false}
        disabled={false}
        tokenUsage={null}
      />,
    );

    expect(screen.getByLabelText("Context usage unavailable")).not.toBeNull();
    expect(screen.getByText("Waiting for token usage data")).not.toBeNull();
  });

  it("does not send when Enter confirms an IME composition", () => {
    const onSend = vi.fn();
    const view = render(
      <Composer
        draft="English"
        onDraftChange={vi.fn()}
        onSend={onSend}
        onStop={vi.fn()}
        running={false}
        stopping={false}
        busy={false}
        disabled={false}
        tokenUsage={null}
      />,
    );

    const input = view.container.querySelector("textarea");
    expect(input).not.toBeNull();
    if (!input) return;
    fireEvent.compositionStart(input);
    fireEvent.keyDown(input, { key: "Enter", code: "Enter", keyCode: 13 });
    expect(onSend).not.toHaveBeenCalled();

    fireEvent.compositionEnd(input);
    fireEvent.keyDown(input, { key: "Enter", code: "Enter", keyCode: 13 });
    expect(onSend).toHaveBeenCalledTimes(1);
  });

  it("turns the send action into a working stop button", () => {
    const onSend = vi.fn();
    const onStop = vi.fn();
    const view = render(
      <Composer
        draft="queued follow-up"
        onDraftChange={vi.fn()}
        onSend={onSend}
        onStop={onStop}
        running
        stopping={false}
        busy={false}
        disabled={false}
        tokenUsage={null}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Stop" }));
    expect(onStop).toHaveBeenCalledTimes(1);
    expect(onSend).not.toHaveBeenCalled();

    view.rerender(
      <Composer
        draft="queued follow-up"
        onDraftChange={vi.fn()}
        onSend={onSend}
        onStop={onStop}
        running
        stopping
        busy={false}
        disabled={false}
        tokenUsage={null}
      />,
    );
    expect(screen.getByRole("button", { name: "Stopping" }).hasAttribute("disabled")).toBe(true);
  });

  it("queues Enter input while the stop button remains independent", () => {
    const onSend = vi.fn();
    const onStop = vi.fn();
    render(
      <Composer
        draft="Follow up after this task"
        onDraftChange={vi.fn()}
        onSend={onSend}
        onStop={onStop}
        running
        stopping={false}
        busy={false}
        disabled={false}
        tokenUsage={null}
      />,
    );

    fireEvent.keyDown(screen.getByRole("textbox"), { key: "Enter", code: "Enter" });
    expect(onSend).toHaveBeenCalledTimes(1);
    expect(onStop).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Stop" }));
    expect(onStop).toHaveBeenCalledTimes(1);
  });
});
