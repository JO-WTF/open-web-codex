import type { ConversationItem, ProjectedRunEvent } from "../types";
import {
  buildConversationItem,
  buildConversationItemFromThreadItem,
  buildItemsFromThread,
  mergeThreadItems,
  upsertItem,
} from "./threadItems";

function appendDelta(
  items: ConversationItem[],
  event: ProjectedRunEvent,
): ConversationItem[] {
  const itemId = event.item_id ?? event.payload.itemId;
  const delta = String(event.payload.data.delta ?? "");
  const sourceType = String(event.payload.data.sourceType ?? "");
  if (!itemId || !delta) {
    return items;
  }
  const existing = items.find((item) => item.id === itemId);
  if (sourceType === "item/agentMessage/delta") {
    const text = existing?.kind === "message" ? existing.text : "";
    return upsertItem(items, {
      id: itemId,
      kind: "message",
      role: "assistant",
      text: `${text}${delta}`,
    });
  }
  if (
    sourceType === "item/reasoning/summaryTextDelta" ||
    sourceType === "item/reasoning/textDelta"
  ) {
    const summary = existing?.kind === "reasoning" ? existing.summary : "";
    const content = existing?.kind === "reasoning" ? existing.content : "";
    return upsertItem(items, {
      id: itemId,
      kind: "reasoning",
      summary:
        sourceType === "item/reasoning/summaryTextDelta"
          ? `${summary}${delta}`
          : summary,
      content:
        sourceType === "item/reasoning/textDelta"
          ? `${content}${delta}`
          : content,
    });
  }
  if (existing?.kind === "tool") {
    return upsertItem(items, {
      ...existing,
      output: `${existing.output ?? ""}${delta}`,
    });
  }
  return items;
}

export function buildItemsFromProjectedEvents(events: ProjectedRunEvent[]) {
  return [...events]
    .sort((left, right) => left.sequence - right.sequence)
    .reduce<ConversationItem[]>((items, event) => {
      if (event.event_type === "codex.item.delta") {
        return appendDelta(items, event);
      }
      if (
        event.event_type !== "codex.item.started" &&
        event.event_type !== "codex.item.completed"
      ) {
        return items;
      }
      const itemId = event.item_id ?? event.payload.itemId;
      const itemType = event.payload.itemType;
      if (!itemId || !itemType) {
        return items;
      }
      const raw = {
        id: itemId,
        type: itemType,
        ...event.payload.data,
      };
      const converted =
        event.event_type === "codex.item.completed"
          ? buildConversationItemFromThreadItem(raw)
          : buildConversationItem(raw);
      return converted ? upsertItem(items, converted) : items;
    }, []);
}

export function recoverConversationItems(
  thread: Record<string, unknown>,
  events: ProjectedRunEvent[],
) {
  const authoritativeHistory = buildItemsFromThread(thread);
  const projectedItems = buildItemsFromProjectedEvents(events);
  return mergeThreadItems(authoritativeHistory, projectedItems);
}
