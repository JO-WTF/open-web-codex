export function requireCompletedTurn(
  events,
  { threadId, turnId, label, sanitize = String },
) {
  const completion = events.find(
    (event) =>
      (threadId == null || event.thread_id === threadId) &&
      event.turn_id === turnId &&
      event.event_type === "codex.turn.completed",
  );
  if (!completion) return undefined;

  const completedTurn = completion.payload?.data?.turn;
  if (
    !completedTurn ||
    typeof completedTurn !== "object" ||
    typeof completedTurn.status !== "string"
  ) {
    throw new Error(`${label} completion omitted its typed status`);
  }
  if (completedTurn.status !== "completed") {
    throw new Error(
      `${label} reached ${completedTurn.status}: ${sanitize(
        completedTurn.error ?? completion.payload,
      )}`,
    );
  }
  return events;
}
