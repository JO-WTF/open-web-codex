import { describe, expect, it } from "vitest";
import {
  parseModelCatalog,
  parseModelProviderCatalog,
  readStoredModelId,
  selectedModelStorageKey,
  writeStoredModelId,
} from "./modelCatalog";

describe("modelCatalog", () => {
  it("parses provider catalog responses", () => {
    const parsed = parseModelProviderCatalog({
      currentProviderId: "deepseek",
      data: [{
        id: "deepseek",
        name: "DeepSeek",
        kind: "custom",
        isCurrent: true,
        modelCount: 1,
        baseUrl: "https://api.deepseek.com",
        envKey: "DEEPSEEK_API_KEY",
        wireApi: "responses",
        canEdit: true,
        canDelete: true,
        canFetchModels: true,
        models: [{ modelId: "deepseek-chat", contextWindow: 65536 }],
      }],
    });
    expect(parsed.currentProviderId).toBe("deepseek");
    expect(parsed.providers[0]?.models?.[0]?.contextWindow).toBe(65536);
  });

  it("parses model list responses", () => {
    expect(parseModelCatalog({
      data: [{ id: "gpt-5.4", model: "gpt-5.4", displayName: "GPT-5.4" }],
    })).toEqual([{
      id: "gpt-5.4",
      model: "gpt-5.4",
      displayName: "GPT-5.4",
    }]);
  });

  it("persists selected model ids per task", () => {
    const key = selectedModelStorageKey("task-1");
    localStorage.removeItem(key);
    expect(readStoredModelId("task-1")).toBeNull();
    writeStoredModelId("task-1", "gpt-5.4");
    expect(readStoredModelId("task-1")).toBe("gpt-5.4");
    localStorage.removeItem(key);
  });
});
