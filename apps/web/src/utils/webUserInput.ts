import type { RequestUserInputRequest } from "../types";

export function parseWebUserInputRequest(
  workspaceId: string,
  requestId: number | string,
  params: Record<string, unknown>,
): RequestUserInputRequest | null {
  const questions = (Array.isArray(params.questions) ? params.questions : []).flatMap((entry) => {
    if (!entry || typeof entry !== "object") return [];
    const question = entry as Record<string, unknown>;
    const id = String(question.id ?? "").trim();
    if (!id) return [];
    const options = (Array.isArray(question.options) ? question.options : []).flatMap((entry) => {
      if (!entry || typeof entry !== "object") return [];
      const option = entry as Record<string, unknown>;
      const label = String(option.label ?? "").trim();
      const description = String(option.description ?? "").trim();
      return label || description ? [{ label, description }] : [];
    });
    return [{
      id,
      header: String(question.header ?? "").trim(),
      question: String(question.question ?? "").trim(),
      isOther: Boolean(question.isOther ?? question.is_other),
      isSecret: Boolean(question.isSecret ?? question.is_secret),
      options: options.length ? options : undefined,
    }];
  });
  if (!questions.length) return null;
  const autoResolution = params.autoResolutionMs ?? params.auto_resolution_ms;
  return {
    workspace_id: workspaceId,
    request_id: requestId,
    params: {
      thread_id: String(params.threadId ?? params.thread_id ?? ""),
      turn_id: String(params.turnId ?? params.turn_id ?? ""),
      item_id: String(params.itemId ?? params.item_id ?? ""),
      questions,
      auto_resolution_ms: typeof autoResolution === "number" ? autoResolution : null,
    },
  };
}
