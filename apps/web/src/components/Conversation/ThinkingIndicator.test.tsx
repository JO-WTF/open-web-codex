// @vitest-environment jsdom
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import ThinkingIndicator from "./ThinkingIndicator";

describe("ThinkingIndicator", () => {
  it("announces the active turn as working", () => {
    render(<ThinkingIndicator />);

    expect(screen.getByRole("status").textContent).toBe("Working…");
  });
});
