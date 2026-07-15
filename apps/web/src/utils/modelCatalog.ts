import type { ModelProviderSummary, ModelSummary } from "../components/Conversation/Composer";
import { parseModelListResponse } from "../features/models/utils/modelListResponse";

function asArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function asString(value: unknown): string | null {
  if (typeof value !== "string") {
    return null;
  }
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
}

function asNumber(value: unknown): number | null {
  if (typeof value === "number" && Number.isFinite(value)) {
    return value;
  }
  if (typeof value === "string") {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : null;
  }
  return null;
}

function extractCatalogData(response: unknown): unknown[] {
  if (!response || typeof response !== "object") {
    return [];
  }
  const record = response as Record<string, unknown>;
  const result =
    record.result && typeof record.result === "object"
      ? (record.result as Record<string, unknown>)
      : null;
  if (Array.isArray(result?.data)) {
    return result.data;
  }
  if (Array.isArray(record.data)) {
    return record.data;
  }
  return [];
}

export function parseModelProviderCatalog(response: unknown): {
  providers: ModelProviderSummary[];
  currentProviderId: string | null;
} {
  const items = extractCatalogData(response);
  const record = (response && typeof response === "object"
    ? response
    : {}) as Record<string, unknown>;
  const result =
    record.result && typeof record.result === "object"
      ? (record.result as Record<string, unknown>)
      : record;
  const currentProviderId = asString(
    result.currentProviderId ?? result.current_provider_id,
  );

  const providers = items
    .map((item) => {
      if (!item || typeof item !== "object") {
        return null;
      }
      const entry = item as Record<string, unknown>;
      const id = asString(entry.id);
      const name = asString(entry.name);
      if (!id || !name) {
        return null;
      }
      const kind = asString(entry.kind);
      const models = asArray(entry.models).map((model) => {
        if (!model || typeof model !== "object") {
          return null;
        }
        const modelEntry = model as Record<string, unknown>;
        const modelId = asString(modelEntry.modelId ?? modelEntry.model_id);
        if (!modelId) {
          return null;
        }
        return {
          modelId,
          modelName: asString(modelEntry.modelName ?? modelEntry.model_name),
          contextWindow: asNumber(modelEntry.contextWindow ?? modelEntry.context_window),
        };
      }).filter((model): model is NonNullable<typeof model> => model !== null);

      return {
        id,
        name,
        kind: (kind === "builtIn" || kind === "local" || kind === "custom" ? kind : "custom") as ModelProviderSummary["kind"],
        isCurrent: Boolean(entry.isCurrent ?? entry.is_current),
        modelCount: asNumber(entry.modelCount ?? entry.model_count) ?? models.length,
        baseUrl: asString(entry.baseUrl ?? entry.base_url) ?? null,
        envKey: asString(entry.envKey ?? entry.env_key) ?? null,
        wireApi: asString(entry.wireApi ?? entry.wire_api) ?? "responses",
        canEdit: Boolean(entry.canEdit ?? entry.can_edit),
        canDelete: Boolean(entry.canDelete ?? entry.can_delete),
        canFetchModels: Boolean(entry.canFetchModels ?? entry.can_fetch_models),
        models,
      } satisfies ModelProviderSummary;
    })
    .filter((provider): provider is NonNullable<typeof provider> => provider !== null);

  return {
    providers,
    currentProviderId: currentProviderId ?? providers.find((provider) => provider.isCurrent)?.id ?? null,
  };
}

export function parseModelCatalog(response: unknown): ModelSummary[] {
  return parseModelListResponse(response).map((model) => ({
    id: model.id,
    displayName: model.displayName,
    model: model.model,
  }));
}

export function selectedModelStorageKey(taskId: string) {
  return `open-web-codex:task-model:${taskId}`;
}

export function readStoredModelId(taskId: string | null): string | null {
  if (!taskId) {
    return null;
  }
  try {
    return localStorage.getItem(selectedModelStorageKey(taskId));
  } catch {
    return null;
  }
}

export function writeStoredModelId(taskId: string, modelId: string) {
  try {
    localStorage.setItem(selectedModelStorageKey(taskId), modelId);
  } catch {
    // ignore storage failures
  }
}
