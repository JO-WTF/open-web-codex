// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import UserInputCard from "./UserInputCard";

afterEach(cleanup);

describe("UserInputCard", () => {
  it("submits a clicked option using the protocol answer shape", () => {
    const onSubmit = vi.fn();
    const request = {
      workspace_id: "ws-1",
      request_id: 7,
      params: {
        thread_id: "thread-1",
        turn_id: "turn-1",
        item_id: "item-1",
        questions: [{
          id: "choice",
          header: "Choose",
          question: "What should I do?",
          options: [
            { label: "Fix first", description: "Apply fixes and review again." },
            { label: "Push now", description: "Accept the current risk." },
          ],
        }],
      },
    };
    render(<UserInputCard request={request} submitting={false} onSubmit={onSubmit} />);

    expect(screen.getByRole("button", { name: "Submit" }).hasAttribute("disabled")).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: /Fix first/ }));
    fireEvent.click(screen.getByRole("button", { name: "Submit" }));
    expect(onSubmit).toHaveBeenCalledWith(request, { answers: { choice: { answers: ["Fix first"] } } });
  });
});
