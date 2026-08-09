import { createInterface } from "node:readline";

const modeArgument = process.argv.find((value) => value.startsWith("--mode="));
const mode = modeArgument?.slice("--mode=".length) || "success";
const input = createInterface({ input: process.stdin, crlfDelay: Infinity });

function send(value) {
  process.stdout.write(`${JSON.stringify(value)}\n`);
}

input.on("line", (line) => {
  let message;
  try {
    message = JSON.parse(line);
  } catch {
    process.exit(3);
    return;
  }

  if (message.method === "initialize" && message.id === 1) {
    if (mode === "timeout") {
      return;
    }
    if (mode === "invalid-json") {
      process.stdout.write("not-json\n");
      return;
    }
    if (mode === "error") {
      send({ id: 1, error: { code: -32603, message: "fixture initialize failure" } });
      return;
    }
    if (mode === "exit") {
      process.exit(42);
      return;
    }
    const result = {
      userAgent: "fake-codex-app-server/1.0.0",
      codexHome: "/tmp/fake-codex-home",
      platformFamily: "test",
      platformOs: "test",
    };
    if (mode === "invalid-initialize") {
      delete result.userAgent;
    }
    send({ id: 1, result });
    return;
  }

  if (message.method === "initialized") {
    return;
  }

  if (message.method === "thread/list" && message.id === 2) {
    if (mode === "thread-list-error") {
      send({ id: 2, error: { code: -32603, message: "fixture thread/list failure" } });
      return;
    }
    send({
      id: 2,
      result: {
        data: [],
        nextCursor: null,
        backwardsCursor: null,
      },
    });
  }
});
