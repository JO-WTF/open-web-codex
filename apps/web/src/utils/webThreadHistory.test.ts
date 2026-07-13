import { describe, expect, it } from "vitest";
import { appendTerminalInteractionOutput, buildWebThreadHistory, isUserThreadItem, mergeWebThreadHistory, unwrapWebRpcResult } from "./webThreadHistory";

describe("isUserThreadItem", () => {
  it("recognizes both live and persisted user message shapes", () => {
    expect(isUserThreadItem({ type: "userMessage" })).toBe(true);
    expect(isUserThreadItem({ type: "message", role: "user" })).toBe(true);
    expect(isUserThreadItem({ type: "agentMessage" })).toBe(false);
  });
});

describe("mergeWebThreadHistory", () => {
  it("preserves messages sent while historical turns are loading", () => {
    expect(mergeWebThreadHistory(
      [{ id: "old", level: "assistant", text: "Earlier response" }],
      [{ id: "optimistic", level: "user", text: "New request" }],
    )).toEqual([
      { id: "old", level: "assistant", text: "Earlier response" },
      { id: "optimistic", level: "user", text: "New request" },
    ]);
  });

  it("does not duplicate an optimistic user message already persisted", () => {
    expect(mergeWebThreadHistory(
      [{ id: "persisted", level: "user", text: "New request" }],
      [{ id: "optimistic", level: "user", text: "New request" }],
    )).toHaveLength(1);
  });
});

describe("appendTerminalInteractionOutput", () => {
  it("ignores empty terminal polls", () => {
    expect(appendTerminalInteractionOutput("waiting\n", "")).toBe("waiting\n");
  });

  it("records terminal interaction without persisting sensitive stdin", () => {
    expect(appendTerminalInteractionOutput("Password:", "secret\r\n")).toBe(
      "Password:\n[stdin supplied]\n",
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

  it("restores reasoning content when the persisted summary is generic", () => {
    const [entry] = buildWebThreadHistory({ turns: [{ items: [{
      id: "reasoning-content",
      type: "reasoning",
      summary: ["Reasoning completed"],
      content: ["Comparing the current layout with the target screenshot."],
    }] }] }, () => "log-1");

    expect(entry).toMatchObject({
      kind: "reasoning",
      text: "Comparing the current layout with the target screenshot.",
    });
    expect(entry.reasoningSummary).toBeUndefined();
  });

  it("restores persisted dynamic tools as commands, diffs, and expandable tool cards", () => {
    let id = 0;
    const result = buildWebThreadHistory({
      turns: [{
        items: [
          { id: "user-1", type: "userMessage", content: [{ type: "text", text: "检查项目" }] },
          {
            id: "call-command",
            type: "dynamicToolCall",
            tool: "exec_command",
            arguments: { cmd: "git status" },
            status: "completed",
            contentItems: [{ type: "inputText", text: "Process exited with code 0\nOutput:\nclean" }],
          },
          {
            id: "call-patch",
            type: "dynamicToolCall",
            tool: "apply_patch",
            arguments: "*** Begin Patch\n*** Update File: src/a.ts\n-old\n+new\n*** End Patch",
            status: "completed",
            contentItems: [],
          },
          {
            id: "call-goal",
            type: "dynamicToolCall",
            tool: "update_goal",
            arguments: { status: "complete" },
            status: "completed",
            contentItems: [{ type: "inputText", text: "Goal updated" }],
          },
          {
            id: "call-code-mode",
            type: "dynamicToolCall",
            tool: "exec",
            arguments: "const result = await tools.exec_command({ cmd: 'git status' });",
            status: "completed",
            contentItems: [{ type: "inputText", text: "Nested tool result" }],
          },
          { id: "final", type: "agentMessage", text: "完成。" },
        ],
      }],
    }, () => `log-${++id}`);

    expect(result).toMatchObject([
      { level: "user", text: "检查项目" },
      { kind: "command_exec", text: "git status", cmdExitCode: 0 },
      { kind: "diff", diffTitle: "File changes" },
      { kind: "tool", toolTitle: "update_goal", toolOutput: "Goal updated" },
      { kind: "tool", toolTitle: "exec", toolOutput: "Nested tool result" },
      { level: "assistant", text: "完成。" },
    ]);
  });

  it("keeps every known thread item type visible in restored history", () => {
    let id = 0;
    const result = buildWebThreadHistory({
      turns: [{ items: [
        { id: "mcp", type: "mcpToolCall", server: "docs", tool: "search", status: "completed", arguments: { q: "api" }, result: { content: [] } },
        { id: "search", type: "webSearch", query: "Codex docs" },
        { id: "plan", type: "plan", text: "1. Inspect" },
        { id: "image", type: "imageView", path: "diagram.png" },
        { id: "sleep", type: "sleep", durationMs: 1000 },
        { id: "compact", type: "contextCompaction" },
        { id: "collab", type: "collabAgentToolCall", tool: "spawnAgent", status: "completed", prompt: "Review" },
        { id: "sub", type: "subAgentActivity", kind: "message", agentPath: "reviewer" },
        { id: "review", type: "enteredReviewMode", review: "Reviewing" },
      ] }],
    }, () => `log-${++id}`);

    expect(result).toHaveLength(9);
    expect(result.every((entry) => entry.kind === "tool")).toBe(true);
  });

  it("redacts sensitive dynamic tool arguments", () => {
    let id = 0;
    const [entry] = buildWebThreadHistory({ turns: [{ items: [{
      id: "stdin",
      type: "dynamicToolCall",
      tool: "write_stdin",
      arguments: { session_id: 1, chars: "password" },
      status: "completed",
      contentItems: [],
    }] }] }, () => `log-${++id}`);

    expect(entry.toolDetail).toContain("[redacted]");
    expect(entry.toolDetail).not.toContain("password");
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
