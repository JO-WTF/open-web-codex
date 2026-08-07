import { afterEach, describe, expect, it, vi } from "vitest";
import { createBrowserId } from "./randomId";

describe("createBrowserId", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("falls back when randomUUID is unavailable or not callable", () => {
    vi.stubGlobal("crypto", { randomUUID: "unsupported" });

    expect(createBrowserId("request")).toMatch(/^request-\d+-/);
  });

  it("uses randomUUID when the browser provides it", () => {
    vi.stubGlobal("crypto", { randomUUID: () => "browser-uuid" });

    expect(createBrowserId("request")).toBe("browser-uuid");
  });
});
