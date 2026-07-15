import { describe, expect, it } from "vitest";
import type { LogEntry } from "../types/logEntry";
import { appendWebLogEntry } from "./webApprovalLog";

const approval = (overrides: Partial<LogEntry> = {}): LogEntry => ({
  id: "log-1",
  level: "info",
  text: "git push",
  kind: "approval",
  approvalId: "item-1",
  approvalRequestId: 42,
  approvalStatus: "pending",
  ...overrides,
});

describe("appendWebLogEntry", () => {
  it("updates a repeated approval instead of appending another card", () => {
    const result = appendWebLogEntry(
      [approval()],
      approval({ id: "log-2", text: "git push --force" }),
    );

    expect(result).toHaveLength(1);
    expect(result[0]).toMatchObject({ id: "log-1", text: "git push --force" });
  });

  it("falls back to request id and does not regress a resolved approval", () => {
    const existing = approval({ approvalId: undefined, approvalStatus: "accepted" });
    const repeated = approval({ id: "log-2", approvalId: undefined });

    expect(appendWebLogEntry([existing], repeated)[0]?.approvalStatus).toBe("accepted");
  });

  it("keeps command execution as a separate lifecycle card", () => {
    const command: LogEntry = {
      id: "command-1",
      level: "info",
      text: "git push",
      kind: "command_exec",
    };

    expect(appendWebLogEntry([approval()], command)).toHaveLength(2);
  });
});
