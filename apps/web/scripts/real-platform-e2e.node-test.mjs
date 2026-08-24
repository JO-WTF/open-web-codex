import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  assertDataInspectionToolOutput,
  configureModelToolSearchRequest,
} from "./real-platform-e2e.mjs";

const providerId = "deterministic-chat-e2e";
const model = "gpt-5.4";
const modelPath = "/providers/" + providerId + "/models/" + model;

function catalog({ currentProviderId, currentModelId }) {
  return {
    currentProviderId,
    currentModelId,
    data: [
      {
        id: providerId,
        models: [{ modelId: model, supportsSearchTool: true }],
      },
    ],
  };
}

describe("deterministic Runtime-owned model selection", () => {
  it("patches model capability before selecting and proves the returned default", async () => {
    const calls = [];
    let patched = false;
    const request = async (pathname, options) => {
      calls.push({ pathname, options });
      if (options.method === "PATCH") {
        assert.equal(pathname, modelPath);
        patched = true;
        return catalog({ currentProviderId: "other", currentModelId: "other-model" });
      }
      assert.equal(options.method, "POST");
      assert.equal(pathname, modelPath + "/select");
      assert.equal(patched, true, "model select must wait for the PATCH result");
      return catalog({ currentProviderId: providerId, currentModelId: model });
    };

    await configureModelToolSearchRequest(request, { providerId, model });

    assert.deepEqual(
      calls.map((call) => ({ pathname: call.pathname, method: call.options.method })),
      [
        { pathname: modelPath, method: "PATCH" },
        { pathname: modelPath + "/select", method: "POST" },
      ],
    );
  });

  it("rejects a selection response that does not make the exact model current", async () => {
    const request = async (pathname, options) => {
      if (options.method === "PATCH") {
        assert.equal(pathname, modelPath);
        return catalog({ currentProviderId: providerId, currentModelId: null });
      }
      return catalog({ currentProviderId: providerId, currentModelId: "other-model" });
    };

    await assert.rejects(
      configureModelToolSearchRequest(request, { providerId, model }),
      (error) =>
        error instanceof assert.AssertionError &&
        error.actual === "other-model" &&
        error.expected === model,
    );
  });
});

describe("deterministic Data inspection output", () => {
  const inspectionOutput = {
    inspection_identity: {
      schemaVersion: "workspace_source_inspection.v2",
      content_sha256: "0".repeat(64),
      source_count: 6,
    },
    inspected_relative_paths: ["mock_data/demand-cities.csv"],
  };

  it("requires a direct, complete JSON inspection result in a Data model request", () => {
    assert.doesNotThrow(() =>
      assertDataInspectionToolOutput([
        {
          body: {
            messages: [
              { role: "tool", content: JSON.stringify(inspectionOutput) },
            ],
          },
        },
      ]),
    );
  });

  it("rejects a truncated inspection result instead of parsing around it", () => {
    const truncated = JSON.stringify(inspectionOutput) + " chars truncated";
    assert.throws(
      () =>
        assertDataInspectionToolOutput([
          { body: { messages: [{ role: "tool", content: truncated }] } },
        ]),
      { message: "Data inspection tool output was truncated before it reached the model" },
    );
  });
});
