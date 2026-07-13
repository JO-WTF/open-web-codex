import { describe, expect, it } from "vitest";
import { appendTerminalInteractionOutput, buildWebThreadHistory, unwrapWebRpcResult } from "./webThreadHistory";

describe("appendTerminalInteractionOutput", () => {
  it("ignores empty terminal polls", () => {
    expect(appendTerminalInteractionOutput("waiting\n", "")).toBe("waiting\n");
  });

  it("appends non-empty stdin to the command transcript", () => {
    expect(appendTerminalInteractionOutput("Password:", "secret\r\n")).toBe(
      "Password:\n[stdin]\nsecret\n",
    );
  });
});

describe("buildWebThreadHistory", () => {
  it("unwraps nested gateway and app-server result envelopes", () => {
    const thread = {
      id: "019f5b75-2266-7910-bc60-b4470041c4e7",
      status: { type: "systemError" },
      turns: [{
        items: [{
          id: "item-1",
          type: "userMessage",
          content: [{ type: "text", text: "1" }],
        }],
      }],
    };

    expect(unwrapWebRpcResult({
      result: {
        id: 84,
        result: { thread },
      },
    })).toEqual({ thread });
  });

  it("keeps reasoning and command execution items from a full turn payload", () => {
    let id = 0;
    const result = buildWebThreadHistory({
      turns: [{
        items: [
          { id: "item-1", type: "userMessage", content: [{ type: "text", text: "初始化 git" }] },
          { id: "item-2", type: "reasoning", content: [], summary: ["Checking the repository."] },
          {
            id: "call-1",
            type: "commandExecution",
            command: "/bin/zsh -lc 'git status'",
            aggregatedOutput: "fatal: not a git repository\n",
            cwd: "/tmp/test",
            exitCode: 128,
            durationMs: 0,
            status: "failed",
          },
        ],
      }],
    }, () => `log-${++id}`);

    expect(result).toMatchObject([
      { level: "user", text: "初始化 git" },
      { kind: "reasoning", text: "Checking the repository." },
      {
        kind: "command_exec",
        text: "/bin/zsh -lc 'git status'",
        cmdOutput: "fatal: not a git repository\n",
        cmdExitCode: 128,
        toolStatus: "failed",
      },
    ]);
  });

  it("shows a visible fallback notice for a systemError without details", () => {
    let id = 0;
    const result = buildWebThreadHistory({
      status: { type: "systemError" },
      turns: [{
        items: [{
          id: "item-1",
          type: "userMessage",
          content: [{ type: "text", text: "1" }],
        }],
      }],
    }, () => `log-${++id}`);

    expect(result).toMatchObject([
      { level: "user", text: "1" },
      {
        level: "error",
        text: "System error. The runtime did not provide any additional error details.",
      },
    ]);
  });
});
