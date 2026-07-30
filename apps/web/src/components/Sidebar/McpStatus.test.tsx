// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import McpStatus from "./McpStatus";

describe("McpStatus", () => {
  afterEach(cleanup);

  it("exposes the server list through a keyboard-operable disclosure button", () => {
    render(
      <McpStatus
        servers={{
          map_utils: {
            name: "map_utils",
            status: "ready",
          },
        }}
      />,
    );

    const toggle = screen.getByRole("button", { name: /MCP Servers/ });
    expect(toggle.getAttribute("aria-expanded")).toBe("false");
    expect(screen.queryByText("map_utils")).toBeNull();

    fireEvent.click(toggle);

    expect(toggle.getAttribute("aria-expanded")).toBe("true");
    expect(screen.getByText("map_utils")).toBeTruthy();
    expect(toggle.getAttribute("aria-controls")).toBe(
      screen.getByText("map_utils").parentElement?.parentElement?.id,
    );
  });
});
