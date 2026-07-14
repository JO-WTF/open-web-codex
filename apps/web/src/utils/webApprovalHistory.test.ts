// @vitest-environment jsdom

import { beforeEach, describe, expect, it } from "vitest";
import { loadWebApprovalHistory, resolveStoredWebApproval, saveWebApproval } from "./webApprovalHistory";

beforeEach(() => window.localStorage.clear());

describe("web approval history", () => {
  it("restores pending requests as non-actionable history after refresh", () => {
    saveWebApproval("workspace-1", "thread-1", {
      id: "approval-1",
      level: "info",
      text: "git push",
      kind: "approval",
      approvalId: "item-1",
      approvalRequestId: 42,
      approvalStatus: "pending",
    });

    expect(loadWebApprovalHistory("workspace-1", "thread-1")).toEqual([{
      id: "approval-1",
      level: "info",
      text: "git push",
      kind: "approval",
      approvalId: "item-1",
      approvalRequestId: 42,
      approvalStatus: "resolved",
    }]);
  });

  it("persists decisions and keeps workspaces and threads isolated", () => {
    saveWebApproval("workspace-1", "thread-1", {
      id: "approval-1",
      level: "info",
      text: "git push",
      kind: "approval",
      approvalRequestId: "request-1",
      approvalStatus: "pending",
    });
    resolveStoredWebApproval("workspace-1", "request-1", "accepted");

    expect(loadWebApprovalHistory("workspace-1", "thread-1")[0]?.approvalStatus).toBe("accepted");
    expect(loadWebApprovalHistory("workspace-1", "thread-2")).toEqual([]);
    expect(loadWebApprovalHistory("workspace-2", "thread-1")).toEqual([]);
  });
});
