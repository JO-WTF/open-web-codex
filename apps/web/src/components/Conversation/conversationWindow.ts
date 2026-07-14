import type { MessageEntry } from "./MessageList";

export const DEFAULT_VISIBLE_TURNS = 10;

function userMessageIndexes(items: MessageEntry[]) {
  return items.flatMap((item, index) => item.level === "user" ? [index] : []);
}

export function initialConversationStart(
  items: MessageEntry[],
  visibleTurns = DEFAULT_VISIBLE_TURNS,
) {
  const indexes = userMessageIndexes(items);
  if (indexes.length <= visibleTurns) return 0;
  return indexes[indexes.length - visibleTurns] ?? 0;
}

export function previousConversationStart(
  items: MessageEntry[],
  currentStart: number,
  visibleTurns = DEFAULT_VISIBLE_TURNS,
) {
  const indexes = userMessageIndexes(items);
  const currentTurn = indexes.indexOf(currentStart);
  if (currentTurn <= 0) return 0;
  return indexes[Math.max(0, currentTurn - visibleTurns)] ?? 0;
}
