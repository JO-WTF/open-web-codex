type BridgeHandler = (payload: Record<string, unknown>) => Promise<unknown>;

const handlers = new Map<string, BridgeHandler>();

export function isTauri(): boolean {
  return false;
}

export function convertFileSrc(path: string): string {
  if (/^(?:data:|blob:|https?:\/\/)/i.test(path)) {
    return path;
  }
  return `/api/files/content?path=${encodeURIComponent(path)}`;
}

export function registerBrowserCommand(
  command: string,
  handler: BridgeHandler,
): () => void {
  handlers.set(command, handler);
  return () => {
    if (handlers.get(command) === handler) {
      handlers.delete(command);
    }
  };
}

export async function invoke<T>(
  command: string,
  payload: Record<string, unknown> = {},
): Promise<T> {
  const handler = handlers.get(command);
  if (!handler) {
    throw new Error(`Browser command is not connected: ${command}`);
  }
  return await handler(payload) as T;
}
