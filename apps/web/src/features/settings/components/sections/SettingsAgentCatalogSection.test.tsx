// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { SettingsAgentCatalogSectionProps } from "@settings/hooks/useSettingsAgentCatalogSection";
import { SettingsAgentCatalogSection } from "./SettingsAgentCatalogSection";

const template: SettingsAgentCatalogSectionProps["templates"][number] = {
  source: "repository",
  release_id: null,
  definition_id: "enterprise-data-agent",
  version: "1.6.0",
  display_name: "Enterprise Data Agent",
  description: "Builds a planning dataset.",
  responsibilities: ["Build data"],
  input_artifact_types: [],
  output_artifact_types: ["planning-dataset.v1"],
  required_capabilities: ["supply_chain_data.build_planning_dataset"],
  capability_template: null,
};

function baseProps(): SettingsAgentCatalogSectionProps {
  return {
    definitions: [],
    templates: [template],
    isLoading: false,
    actionDefinitionId: null,
    error: null,
    validationByDefinition: {},
    onRefresh: vi.fn(),
    onSaveDraft: vi.fn(async () => null),
    onValidate: vi.fn(async () => null),
    onPublish: vi.fn(async () => null),
  };
}

describe("SettingsAgentCatalogSection", () => {
  afterEach(cleanup);

  it("publishes only a reviewed capability template selection", async () => {
    const props = baseProps();
    render(<SettingsAgentCatalogSection {...props} />);

    fireEvent.change(screen.getByLabelText("Agent ID"), {
      target: { value: "regional-data-reviewer" },
    });
    fireEvent.change(screen.getByLabelText("Display name"), {
      target: { value: "Regional Data Reviewer" },
    });
    fireEvent.change(screen.getByLabelText("Description"), {
      target: { value: "Reviews regional planning inputs." },
    });
    fireEvent.change(screen.getByLabelText("Responsibilities"), {
      target: { value: "Review source coverage\nPublish validated data" },
    });
    fireEvent.change(screen.getByLabelText("Agent instructions"), {
      target: { value: "Use only reviewed data capabilities." },
    });
    fireEvent.change(screen.getByLabelText("Reviewed capability template"), {
      target: { value: "enterprise-data-agent@1.6.0" },
    });
    expect(screen.getByText(/supply_chain_data\.build_planning_dataset/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Create draft" }));

    await waitFor(() => expect(props.onSaveDraft).toHaveBeenCalledTimes(1));
    expect(vi.mocked(props.onSaveDraft).mock.calls[0][0]).toMatchObject({
      definition_id: "regional-data-reviewer",
      capability_template: {
        definition_id: "enterprise-data-agent",
        version: "1.6.0",
      },
      output_artifact_types: ["planning-dataset.v1"],
    });
  });
});
