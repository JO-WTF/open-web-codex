import { describe, expect, it } from "vitest";
import { buildWebThreadHistory } from "./webThreadHistory";

describe("buildWebThreadHistory", () => {
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
});
