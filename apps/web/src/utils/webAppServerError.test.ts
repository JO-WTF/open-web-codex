import { describe, expect, it } from "vitest";
import { isWebAppServerRecoveryEvent, parseWebAppServerError } from "./webAppServerError";

describe("isWebAppServerRecoveryEvent", () => {
  it.each([
    "turn/started",
    "thread/status/changed",
    "item/started",
    "item/agentMessage/delta",
    "item/commandExecution/outputDelta",
    "turn/diff/updated",
    "serverRequest/resolved",
    "turn/completed",
  ])("treats %s as proof that the response stream recovered", (method) => {
    expect(isWebAppServerRecoveryEvent(method)).toBe(true);
  });

  it.each([
    "error",
    "thread/tokenUsage/updated",
    "account/rateLimits/updated",
  ])("does not clear the reconnect notice for %s", (method) => {
    expect(isWebAppServerRecoveryEvent(method)).toBe(false);
  });
});

describe("parseWebAppServerError", () => {
  it("turns response stream disconnects into a reconnect status", () => {
    expect(parseWebAppServerError({
      error: {
        additionalDetails: "stream disconnected before completion: failed to send websocket request: Connection closed normally",
        codexErrorInfo: { responseStreamDisconnected: { httpStatusCode: null } },
        message: "Reconnecting... 1/5",
      },
      threadId: "019f5b88-612b-7cc3-8309-45fc1a929d15",
    })).toEqual({ recoverable: true, text: "Connection interrupted. Reconnecting (1/5)…" });
  });

  it("keeps a concise message for terminal errors", () => {
    expect(parseWebAppServerError({ error: { message: "Authentication failed" } })).toEqual({
      recoverable: false,
      text: "Authentication failed",
    });
  });
});
