import { spawn } from "node:child_process";
import { resolve } from "node:path";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

function parseArgs(argv) {
  const options = {
    bin: process.env.CODEX_BIN || "codex",
    binArgs: [],
    timeoutMs: 15_000,
    clientVersion: "codex-monitor-contract-harness/1.0.0",
  };

  for (let index = 0; index < argv.length; index += 1) {
    const value = argv[index];
    if (value === "--bin") {
      options.bin = argv[index + 1];
      index += 1;
    } else if (value === "--bin-arg") {
      options.binArgs.push(argv[index + 1]);
      index += 1;
    } else if (value === "--timeout-ms") {
      options.timeoutMs = Number(argv[index + 1]);
      index += 1;
    } else if (value === "--client-version") {
      options.clientVersion = argv[index + 1];
      index += 1;
    } else {
      throw new Error(`Unknown argument: ${value}`);
    }
  }

  if (!options.bin) {
    throw new Error("--bin requires a value");
  }
  if (!Number.isInteger(options.timeoutMs) || options.timeoutMs < 1) {
    throw new Error("--timeout-ms must be a positive integer");
  }
  if (options.binArgs.some((value) => value === undefined)) {
    throw new Error("--bin-arg requires a value");
  }
  return options;
}

function writeJsonLine(child, value) {
  child.stdin.write(`${JSON.stringify(value)}\n`);
}

function requireOfficialInitializeResponse(result) {
  if (!result || typeof result !== "object" || Array.isArray(result)) {
    throw new Error("initialize response must be an object");
  }

  const fields = ["userAgent", "codexHome", "platformFamily", "platformOs"];
  for (const field of fields) {
    if (typeof result[field] !== "string") {
      throw new Error(`initialize response must contain string ${field}`);
    }
  }
}

function requireThreadListResponse(result) {
  if (!result || typeof result !== "object" || Array.isArray(result)) {
    throw new Error("thread/list response must be an object");
  }
  if (!Array.isArray(result.data)) {
    throw new Error("thread/list response must contain data array");
  }
  for (const field of ["nextCursor", "backwardsCursor"]) {
    if (!(field in result) || (result[field] !== null && typeof result[field] !== "string")) {
      throw new Error(`thread/list response must contain nullable string ${field}`);
    }
  }
}

export async function smokeAppServer(options) {
  const commandArgs = [...options.binArgs, "app-server"];
  const child = spawn(options.bin, commandArgs, {
    stdio: ["pipe", "pipe", "pipe"],
    windowsHide: true,
  });
  const stdout = createInterface({ input: child.stdout, crlfDelay: Infinity });
  let stderr = "";
  let settled = false;

  child.stderr.setEncoding("utf8");
  child.stderr.on("data", (chunk) => {
    if (stderr.length < 8192) {
      stderr += chunk.slice(0, 8192 - stderr.length);
    }
  });

  let result;
  try {
    result = await new Promise((resolve, reject) => {
    const finish = (error, value) => {
      if (settled) {
        return;
      }
      settled = true;
      clearTimeout(timer);
      if (error) {
        reject(error);
      } else {
        resolve(value);
      }
    };

    const timer = setTimeout(() => {
      finish(new Error(`official initialize/thread/list timed out after ${options.timeoutMs} ms`));
    }, options.timeoutMs);

    child.once("error", (error) => {
      finish(new Error(`failed to start ${options.bin}: ${error.message}`));
    });
    child.once("exit", (code, signal) => {
      if (!settled) {
        const diagnostic = stderr.trim() ? `; stderr: ${stderr.trim()}` : "";
        finish(new Error(`app-server exited before official initialize/thread/list completed (code=${code}, signal=${signal})${diagnostic}`));
      }
    });
    stdout.on("line", (line) => {
      if (!line.trim()) {
        return;
      }
      let message;
      try {
        message = JSON.parse(line);
      } catch (error) {
        finish(new Error(`invalid JSON from app-server: ${error.message}`));
        return;
      }
      if (message.id !== 1 && message.id !== 2) {
        return;
      }
      if (message.error) {
        const method = message.id === 1 ? "initialize" : "thread/list";
        finish(new Error(`${method} returned an error: ${JSON.stringify(message.error)}`));
        return;
      }
      if (!("result" in message)) {
        const method = message.id === 1 ? "initialize" : "thread/list";
        finish(new Error(`${method} response has neither result nor error`));
        return;
      }

      if (message.id === 1) {
        try {
          requireOfficialInitializeResponse(message.result);
        } catch (error) {
          finish(error);
          return;
        }
        writeJsonLine(child, { method: "initialized" });
        writeJsonLine(child, {
          id: 2,
          method: "thread/list",
          params: { limit: 1 },
        });
        return;
      }

      try {
        requireThreadListResponse(message.result);
      } catch (error) {
        finish(error);
        return;
      }
      finish(null, { threadListCount: message.result.data.length });
    });

    child.once("spawn", () => {
      writeJsonLine(child, {
        id: 1,
        method: "initialize",
        params: {
          clientInfo: {
            name: "codex_monitor_contract_harness",
            title: "Codex Monitor Contract Harness",
            version: options.clientVersion,
          },
        },
      });
    });
    });
  } finally {
    stdout.close();
    child.stdin.end();
    if (child.exitCode === null && child.signalCode === null) {
      child.kill();
    }
  }
  return result;
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  try {
    const result = await smokeAppServer(options);
    process.stdout.write(`${JSON.stringify({ ok: true, ...result })}\n`);
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 1;
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  await main();
}
