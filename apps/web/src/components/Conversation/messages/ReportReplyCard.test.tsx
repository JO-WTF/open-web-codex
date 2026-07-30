// @vitest-environment jsdom

import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ReportReplyCard as ReportReplyCardData } from "../../../utils/replyCards";
import ReportReplyCard from "./ReportReplyCard";

const { readReplyArtifact } = vi.hoisted(() => ({
  readReplyArtifact: vi.fn(),
}));

vi.mock("../../../../browser/session", () => ({
  platformClient: { readReplyArtifact },
}));

const artifactId = "8e98ff2f-82ee-4cc9-a3e6-2974debf8666";
const card: ReportReplyCardData = {
  type: "card",
  kind: "report.v1",
  id: "indonesia-decision-report",
  title: "Indonesia warehouse-network decision",
  status: "ready",
  source: {
    type: "artifact",
    format: "json",
    artifactId,
    mimeType: "application/json",
    url: `/api/artifacts/${artifactId}/content`,
  },
};

async function markdownSha256(markdown: string) {
  const digest = await crypto.subtle.digest(
    "SHA-256",
    new TextEncoder().encode(markdown),
  );
  return Array.from(new Uint8Array(digest), (byte) => (
    byte.toString(16).padStart(2, "0")
  )).join("");
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((resolvePromise) => {
    resolve = resolvePromise;
  });
  return { promise, resolve };
}

describe("ReportReplyCard", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(cleanup);

  it("shows an explicit loading state while authorized content is pending", () => {
    const request = deferred<Record<string, unknown>>();
    readReplyArtifact.mockReturnValue(request.promise);

    render(<ReportReplyCard card={card} />);

    expect(screen.getByRole("status").textContent).toBe("Loading report…");
    expect(
      screen.getByRole("article", {
        name: "Report: Indonesia warehouse-network decision",
      }).getAttribute("aria-busy"),
    ).toBe("true");
  });

  it("loads authorized JSON and safely renders its GFM Markdown", async () => {
    const markdown = [
      "# Network decision",
      "",
      "| Site | Cost |",
      "| --- | ---: |",
      "| Bekasi | 10 |",
      "",
      "[unsafe](javascript:alert(1))",
      "",
      "<script>window.reportOwned = true</script>",
    ].join("\n");
    readReplyArtifact.mockResolvedValue({
      schema_version: "indonesia_decision_report.v1",
      markdown,
      markdown_sha256: await markdownSha256(markdown),
    });

    const view = render(<ReportReplyCard card={card} />);

    expect(
      await screen.findByRole("heading", { name: "Network decision" }),
    ).toBeTruthy();
    expect(screen.getByRole("table")).toBeTruthy();
    expect(readReplyArtifact).toHaveBeenCalledTimes(1);
    expect(readReplyArtifact).toHaveBeenCalledWith(card.source.url);
    expect(view.container.querySelector("script")).toBeNull();
    expect(
      screen.getByText("unsafe").closest("a")?.getAttribute("href"),
    ).toBe("");
    expect(screen.getByText("Ready")).toBeTruthy();
  });

  it("rejects report Markdown whose declared digest does not match", async () => {
    readReplyArtifact.mockResolvedValue({
      schema_version: "indonesia_decision_report.v1",
      markdown: "# Tampered decision",
      markdown_sha256: "a".repeat(64),
    });

    render(<ReportReplyCard card={card} />);

    expect((await screen.findByRole("alert")).textContent).toContain(
      "Report content failed its integrity check.",
    );
    expect(
      screen.queryByRole("heading", { name: "Tampered decision" }),
    ).toBeNull();
    expect(screen.getByText("Failed")).toBeTruthy();
  });

  it("shows a contract error instead of rendering malformed Artifact JSON", async () => {
    readReplyArtifact.mockResolvedValue({
      schema_version: "another-report.v1",
      markdown: "# Untrusted report",
      markdown_sha256: "a".repeat(64),
    });

    render(<ReportReplyCard card={card} />);

    expect((await screen.findByRole("alert")).textContent).toContain(
      "Report content did not match the supported contract.",
    );
    expect(screen.queryByRole("heading", { name: "Untrusted report" })).toBeNull();
  });

  it("shows an explicit load error when the authorized read fails", async () => {
    readReplyArtifact.mockRejectedValue(new Error("forbidden"));

    render(<ReportReplyCard card={card} />);

    await waitFor(() => {
      expect(screen.getByRole("alert").textContent).toContain(
        "Report content could not be loaded.",
      );
    });
    expect(screen.getByText("Failed")).toBeTruthy();
  });
});
