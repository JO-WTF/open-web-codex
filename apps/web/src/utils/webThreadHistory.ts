import type { LogEntry } from "../WebApp";

export function unwrapWebRpcResult(value: unknown): unknown {
  let current = value;
  const visited = new Set<object>();

  for (let depth = 0; depth < 8; depth += 1) {
    if (!current || typeof current !== "object") break;
    if (visited.has(current)) break;
    visited.add(current);

    const record = current as Record<string, unknown>;
    if (!("result" in record)) break;
    current = record.result;
  }

  return current;
}

function asText(value: unknown) {
  return typeof value === "string" ? value : "";
}

export function commandText(value: unknown) {
  return Array.isArray(value)
    ? value.map((part) => String(part)).join(" ")
    : asText(value);
}

export function appendTerminalInteractionOutput(output: string, stdin: string) {
  if (!stdin) return output;
  const normalized = stdin.replace(/\r\n/g, "\n");
  const suffix = normalized.endsWith("\n") ? "" : "\n";
  return `${output}\n[stdin]\n${normalized}${suffix}`.slice(-200_000);
}

function messageText(item: Record<string, unknown>) {
  if (asText(item.text)) return asText(item.text);
  if (!Array.isArray(item.content)) return "";
  return item.content
    .map((part) => asText((part as Record<string, unknown>)?.text))
    .filter(Boolean)
    .join("\n");
}

export function buildWebThreadHistory(
  thread: Record<string, unknown>,
  createId: () => string,
): LogEntry[] {
  const turns = Array.isArray(thread.turns) ? thread.turns : [];
  const entries = turns.flatMap((turn) => {
    const items = Array.isArray((turn as Record<string, unknown>)?.items)
      ? ((turn as Record<string, unknown>).items as Record<string, unknown>[])
      : [];
    return items.flatMap((item): LogEntry[] => {
      const type = asText(item.type);
      if (type === "commandExecution") {
        const command = commandText(item.command);
        if (!command) return [];
        return [{
          id: createId(),
          level: "info",
          text: command,
          kind: "command_exec",
          toolStatus: asText(item.status) || undefined,
          cmdOutput: asText(item.aggregatedOutput) || undefined,
          cmdExitCode: typeof item.exitCode === "number" ? item.exitCode : undefined,
          cmdDurationMs: typeof item.durationMs === "number" ? item.durationMs : undefined,
          cmdCwd: asText(item.cwd) || undefined,
          cmdActions: Array.isArray(item.commandActions)
            ? item.commandActions.map((action) => {
                const record = action as Record<string, unknown>;
                return { type: asText(record.type), path: asText(record.path) };
              })
            : [],
        }];
      }
      if (type === "reasoning") {
        const text = Array.isArray(item.summary)
          ? item.summary.map(String).join("\n\n")
          : asText(item.summary);
        return text ? [{ id: createId(), level: "system", text, kind: "reasoning" }] : [];
      }
      const text = messageText(item);
      if (!text) return [];
      if (type === "userMessage") return [{ id: createId(), level: "user", text }];
      if (type === "agentMessage") return [{ id: createId(), level: "assistant", text }];
      return [];
    });
  });

  const status = thread.status;
  const statusType = status && typeof status === "object"
    ? asText((status as Record<string, unknown>).type)
    : "";
  if (statusType === "systemError" && !entries.some((entry) => entry.level === "error")) {
    entries.push({
      id: createId(),
      level: "error",
      text: "System error. The runtime did not provide any additional error details.",
    });
  }

  return entries;
}
