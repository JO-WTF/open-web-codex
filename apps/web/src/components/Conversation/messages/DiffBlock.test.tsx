// @vitest-environment jsdom
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import DiffBlock from "./DiffBlock";

describe("DiffBlock", () => {
  it("shows an updating state and remains collapsible", () => {
    render(
      <DiffBlock
        title="2 files changed · +3 −1"
        updating
        lines={[{ type: "add", text: "const value = 1;" }]}
      />,
    );

    const toggle = screen.getByRole("button", { name: /2 files changed/ });
    expect(screen.getByText("Updating files…")).toBeTruthy();
    expect(screen.queryByText("const value = 1;")).toBeNull();
    fireEvent.click(toggle);
    expect(screen.getByText("const value = 1;")).toBeTruthy();
    expect(toggle.getAttribute("aria-expanded")).toBe("true");
  });

  it("shows completion after the turn finishes", () => {
    render(<DiffBlock title="1 file changed · +1 −0" lines={[]} />);

    expect(screen.getByText("Completed")).toBeTruthy();
  });
});
