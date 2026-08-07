export function createBrowserId(prefix = "id"): string {
  const randomUUID = globalThis.crypto?.randomUUID;
  if (typeof randomUUID === "function") {
    return randomUUID.call(globalThis.crypto);
  }
  return `${prefix}-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}
