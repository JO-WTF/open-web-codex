import { describe, expect, it } from "vitest";
import type { PlatformApproval } from "../services/platformTypes";
import { parseUserInputFromApproval } from "./parseUserInputFromApproval";

function approval(overrides: Partial<PlatformApproval> = {}): PlatformApproval {
  return {
    id: "appr-1",
    run_id: "run-1",
    request_type: "item/tool/requestUserInput",
    request_payload: {
      questions: [{ id: "q1", header: "Name", question: "What is your name?", options: [] }],
    },
    status: "pending",
    codex_request_id: "req-1",
    workspace_id: "ws-1",
    thread_id: "thread-1",
    decision: null,
    decided_by: null,
    decided_at: null,
    created_at: "2026-07-15T00:00:00Z",
    expires_at: null,
    ...overrides,
  };
}

describe("parseUserInputFromApproval", () => {
  it("parses pending requestUserInput approvals", () => {
    const request = parseUserInputFromApproval(approval());
    expect(request).toMatchObject({
      request_id: "req-1",
      workspace_id: "ws-1",
      params: {
        questions: [expect.objectContaining({ id: "q1", question: "What is your name?" })],
      },
    });
  });

  it("returns null for non-user-input approvals", () => {
    expect(parseUserInputFromApproval(approval({ request_type: "item/commandExecution/requestApproval" }))).toBeNull();
  });

  it("returns null for resolved approvals", () => {
    expect(parseUserInputFromApproval(approval({ status: "approved" }))).toBeNull();
  });

  it("returns null when workspace or request id is missing", () => {
    expect(parseUserInputFromApproval(approval({ workspace_id: null }))).toBeNull();
    expect(parseUserInputFromApproval(approval({ codex_request_id: null }))).toBeNull();
  });
});
