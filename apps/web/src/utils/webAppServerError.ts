export type WebAppServerError = {
  recoverable: boolean;
  text: string;
};

export function isWebAppServerRecoveryEvent(method: string): boolean {
  return method === "turn/started"
    || method === "turn/completed"
    || method === "thread/status/changed"
    || method === "turn/plan/updated"
    || method === "turn/diff/updated"
    || method === "serverRequest/resolved"
    || method.startsWith("item/");
}

export function parseWebAppServerError(params: Record<string, unknown>): WebAppServerError {
  const error = params.error && typeof params.error === "object"
    ? params.error as Record<string, unknown>
    : params;
  const message = typeof error.message === "string" ? error.message : "";
  const details = typeof error.additionalDetails === "string" ? error.additionalDetails : "";
  const codexInfo = error.codexErrorInfo && typeof error.codexErrorInfo === "object"
    ? error.codexErrorInfo as Record<string, unknown>
    : null;
  const reconnectMatch = `${message} ${details}`.match(/Reconnecting\.\.\.\s*(\d+)\s*\/\s*(\d+)/i);
  const disconnected = Boolean(codexInfo?.responseStreamDisconnected) || /stream disconnected|websocket request/i.test(details);

  if (reconnectMatch || disconnected) {
    const progress = reconnectMatch ? ` (${reconnectMatch[1]}/${reconnectMatch[2]})` : "";
    return { recoverable: true, text: `Connection interrupted. Reconnecting${progress}…` };
  }

  return {
    recoverable: false,
    text: message || details || "The runtime reported an unknown error.",
  };
}
