import type { ConversationItem } from "../types";
import { parseCollabToolCallItem } from "./threadItems.collab";
import { asNumber, asString } from "./threadItems.shared";

type ThreadItemSource = "live" | "history";

const SENSITIVE_FIELD =
  /(?:authorization|cookie|credential|password|secret|token|api[_-]?key|stdin|chars)/i;

function redactSensitive(value: unknown, key = ""): unknown {
  if (SENSITIVE_FIELD.test(key)) {
    return "[redacted]";
  }
  if (Array.isArray(value)) {
    return value.map((entry) => redactSensitive(entry));
  }
  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value as Record<string, unknown>).map(([entryKey, entryValue]) => [
        entryKey,
        redactSensitive(entryValue, entryKey),
      ]),
    );
  }
  return value;
}

function jsonText(value: unknown) {
  if (value == null || value === "") {
    return "";
  }
  if (typeof value === "string") {
    return value;
  }
  try {
    return JSON.stringify(redactSensitive(value), null, 2);
  } catch {
    return String(value);
  }
}

function contentItemsText(value: unknown) {
  if (!Array.isArray(value)) {
    return "";
  }
  return value
    .map((entry) => {
      if (!entry || typeof entry !== "object") {
        return "";
      }
      const record = entry as Record<string, unknown>;
      if (record.type === "inputText") {
        return asString(record.text);
      }
      if (record.type === "inputImage") {
        return asString(record.imageUrl ?? record.image_url);
      }
      return "";
    })
    .filter(Boolean)
    .join("\n");
}

function extractImageInputValue(input: Record<string, unknown>) {
  const value =
    asString(input.url ?? "") ||
    asString(input.path ?? "") ||
    asString(input.value ?? "") ||
    asString(input.data ?? "") ||
    asString(input.source ?? "");
  return value.trim();
}

function parseUserInputs(inputs: Array<Record<string, unknown>>) {
  const textParts: string[] = [];
  const images: string[] = [];
  inputs.forEach((input) => {
    const type = asString(input.type);
    if (type === "text") {
      const text = asString(input.text);
      if (text) {
        textParts.push(text);
      }
      return;
    }
    if (type === "skill") {
      const name = asString(input.name);
      if (name) {
        textParts.push(`$${name}`);
      }
      return;
    }
    if (type === "image" || type === "localImage") {
      const value = extractImageInputValue(input);
      if (value) {
        images.push(value);
      }
    }
  });
  return { text: textParts.join(" ").trim(), images };
}

export function normalizeThreadItem(
  item: Record<string, unknown>,
  source: ThreadItemSource,
): ConversationItem | null {
  const type = asString(item.type);
  const id = asString(item.id);
  if (!id || !type) {
    return null;
  }
  if (type === "agentMessage") {
    return source === "history"
      ? {
          id,
          kind: "message",
          role: "assistant",
          text: asString(item.text),
        }
      : null;
  }
  if (type === "userMessage") {
    const content = Array.isArray(item.content) ? item.content : [];
    const { text, images } = parseUserInputs(content as Array<Record<string, unknown>>);
    return {
      id,
      kind: "message",
      role: "user",
      text,
      images: images.length > 0 ? images : undefined,
    };
  }
  if (type === "reasoning") {
    const summary = Array.isArray(item.summary)
      ? item.summary.map((entry) => asString(entry)).join("\n")
      : asString(item.summary ?? "");
    const content = Array.isArray(item.content)
      ? item.content.map((entry) => asString(entry)).join("\n")
      : asString(item.content ?? "");
    return { id, kind: "reasoning", summary, content };
  }
  if (type === "plan") {
    return {
      id,
      kind: "tool",
      toolType: "plan",
      title: "Plan",
      detail: asString(item.status ?? ""),
      status: asString(item.status ?? ""),
      output: asString(item.text ?? ""),
    };
  }
  if (type === "commandExecution") {
    const command = Array.isArray(item.command)
      ? item.command.map((part) => asString(part)).join(" ")
      : asString(item.command ?? "");
    const durationMs = asNumber(item.durationMs ?? item.duration_ms);
    return {
      id,
      kind: "tool",
      toolType: type,
      title: command ? `Command: ${command}` : "Command",
      detail: asString(item.cwd ?? ""),
      status: asString(item.status ?? ""),
      output: asString(item.aggregatedOutput ?? ""),
      durationMs,
    };
  }
  if (type === "fileChange") {
    const changes = Array.isArray(item.changes) ? item.changes : [];
    const normalizedChanges = changes
      .map((change) => {
        const path = asString(change?.path ?? "");
        const kind = change?.kind as Record<string, unknown> | string | undefined;
        const kindType =
          typeof kind === "string"
            ? kind
            : typeof kind === "object" && kind
              ? asString((kind as Record<string, unknown>).type ?? "")
              : "";
        const normalizedKind = kindType ? kindType.toLowerCase() : "";
        const diff = asString(change?.diff ?? "");
        return { path, kind: normalizedKind || undefined, diff: diff || undefined };
      })
      .filter((change) => change.path);
    const formattedChanges = normalizedChanges
      .map((change) => {
        const prefix =
          change.kind === "add"
            ? "A"
            : change.kind === "delete"
              ? "D"
              : change.kind
                ? "M"
                : "";
        return [prefix, change.path].filter(Boolean).join(" ");
      })
      .filter(Boolean);
    const paths = formattedChanges.join(", ");
    const diffOutput = normalizedChanges
      .map((change) => change.diff ?? "")
      .filter(Boolean)
      .join("\n\n");
    return {
      id,
      kind: "tool",
      toolType: type,
      title: "File changes",
      detail: paths || "Pending changes",
      status: asString(item.status ?? ""),
      output: diffOutput,
      changes: normalizedChanges,
    };
  }
  if (type === "mcpToolCall") {
    const server = asString(item.server ?? "");
    const tool = asString(item.tool ?? "");
    const args = jsonText(item.arguments);
    return {
      id,
      kind: "tool",
      toolType: type,
      title: `Tool: ${server}${tool ? ` / ${tool}` : ""}`,
      detail: args,
      status: asString(item.status ?? ""),
      output: jsonText(item.result ?? item.error),
    };
  }
  if (type === "hookPrompt") {
    const fragments = Array.isArray(item.fragments) ? item.fragments : [];
    const text = fragments
      .map((fragment) =>
        fragment && typeof fragment === "object"
          ? asString((fragment as Record<string, unknown>).text)
          : "",
      )
      .filter(Boolean)
      .join("\n");
    return {
      id,
      kind: "tool",
      toolType: type,
      title: "Hook prompt",
      detail: "",
      status: "completed",
      output: text,
    };
  }
  if (type === "dynamicToolCall") {
    const tool = asString(item.tool).trim() || "Tool";
    const namespace = asString(item.namespace).trim();
    return {
      id,
      kind: "tool",
      toolType: type,
      title: namespace ? `Tool: ${namespace} / ${tool}` : `Tool: ${tool}`,
      detail: jsonText(item.arguments),
      status: asString(item.status).trim() || "completed",
      output: contentItemsText(item.contentItems),
    };
  }
  if (type === "collabToolCall" || type === "collabAgentToolCall") {
    return parseCollabToolCallItem(item);
  }
  if (type === "webSearch") {
    const status = asString(item.status ?? "").trim();
    return {
      id,
      kind: "tool",
      toolType: type,
      title: "Web search",
      detail: asString(item.query ?? ""),
      status: status || "completed",
      output: "",
    };
  }
  if (type === "imageView") {
    return {
      id,
      kind: "tool",
      toolType: type,
      title: "Image view",
      detail: asString(item.path ?? ""),
      status: "",
      output: "",
    };
  }
  if (type === "imageGeneration") {
    return {
      id,
      kind: "tool",
      toolType: type,
      title: "Image generation",
      detail: asString(item.revisedPrompt ?? ""),
      status: asString(item.status).trim() || "completed",
      output: asString(item.savedPath ?? item.result ?? ""),
    };
  }
  if (type === "sleep") {
    const durationMs = asNumber(item.durationMs ?? item.duration_ms) ?? 0;
    return {
      id,
      kind: "tool",
      toolType: type,
      title: "Wait",
      detail: `Waited ${durationMs}ms`,
      status: asString(item.status).trim() || "completed",
      output: "",
      durationMs,
    };
  }
  if (type === "subAgentActivity") {
    return {
      id,
      kind: "tool",
      toolType: type,
      title: "Sub-agent activity",
      detail: [asString(item.kind), asString(item.agentPath ?? item.agent_path)]
        .filter(Boolean)
        .join(" · "),
      status: asString(item.status).trim() || "completed",
      output: "",
    };
  }
  if (type === "contextCompaction") {
    const status = asString(item.status ?? "").trim();
    return {
      id,
      kind: "tool",
      toolType: type,
      title: "Context compaction",
      detail: "Compacting conversation context to fit token limits.",
      status: status || "completed",
      output: "",
    };
  }
  if (type === "enteredReviewMode" || type === "exitedReviewMode") {
    return {
      id,
      kind: "review",
      state: type === "enteredReviewMode" ? "started" : "completed",
      text: asString(item.review ?? ""),
    };
  }
  return {
    id,
    kind: "tool",
    toolType: type,
    title: `Unsupported item: ${type}`,
    detail: jsonText(item),
    status: asString(item.status).trim() || "unknown",
    output: "",
  };
}

export function buildConversationItem(
  item: Record<string, unknown>,
): ConversationItem | null {
  return normalizeThreadItem(item, "live");
}

export function buildConversationItemFromThreadItem(
  item: Record<string, unknown>,
): ConversationItem | null {
  return normalizeThreadItem(item, "history");
}

export function buildItemsFromThread(thread: Record<string, unknown>) {
  const turns = Array.isArray(thread.turns) ? thread.turns : [];
  const items: ConversationItem[] = [];
  turns.forEach((turn) => {
    const turnRecord = turn as Record<string, unknown>;
    const turnItems = Array.isArray(turnRecord.items)
      ? (turnRecord.items as Record<string, unknown>[])
      : [];
    turnItems.forEach((item) => {
      const converted = buildConversationItemFromThreadItem(item);
      if (converted) {
        items.push(converted);
      }
    });
  });
  return items;
}

export function isReviewingFromThread(thread: Record<string, unknown>) {
  const turns = Array.isArray(thread.turns) ? thread.turns : [];
  let reviewing = false;
  turns.forEach((turn) => {
    const turnRecord = turn as Record<string, unknown>;
    const turnItems = Array.isArray(turnRecord.items)
      ? (turnRecord.items as Record<string, unknown>[])
      : [];
    turnItems.forEach((item) => {
      const type = asString(item?.type ?? "");
      if (type === "enteredReviewMode") {
        reviewing = true;
      } else if (type === "exitedReviewMode") {
        reviewing = false;
      }
    });
  });
  return reviewing;
}
