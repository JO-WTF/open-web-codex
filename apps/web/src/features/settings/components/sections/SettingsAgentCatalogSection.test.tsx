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
    publishedAgents: [],
    templates: [template],
    isLoading: false,
    actionDefinitionId: null,
    loadingAgentKey: null,
    error: null,
    validationByDefinition: {},
    detailByAgent: {},
    onRefresh: vi.fn(),
    onSaveDraft: vi.fn(async () => null),
    onValidate: vi.fn(async () => null),
    onPublish: vi.fn(async () => null),
    onLoadPublished: vi.fn(async () => null),
  };
}

describe("SettingsAgentCatalogSection", () => {
  afterEach(cleanup);

  it("separates the Agent directory from the create form in Studio mode", () => {
    const props = baseProps();
    render(<SettingsAgentCatalogSection {...props} studioMode />);

    expect(screen.getByText("Agent directory")).toBeTruthy();
    expect(screen.queryByLabelText("Agent ID")).toBeNull();
    const newAgentButton = screen.getByRole("button", { name: "New Agent" });
    expect(newAgentButton.classList.contains("settings-studio-create-button")).toBe(true);
    fireEvent.click(newAgentButton);
    expect(screen.getByLabelText("Agent ID")).toBeTruthy();
    expect(screen.queryByText("Agent directory")).toBeNull();
    fireEvent.click(
      screen.getByRole("button", { name: "Back to Agent directory" }),
    );
    expect(screen.getByText("Agent directory")).toBeTruthy();
  });

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
    expect(screen.queryByText("How to define a useful Agent")).toBeNull();
    const agentIdHelp = screen.getByLabelText("Help for Agent ID");
    fireEvent.click(agentIdHelp);
    expect(document.activeElement).toBe(agentIdHelp);
    expect(
      screen.getByText(/It becomes permanent after the draft is created/),
    ).toBeTruthy();
    expect(screen.getByLabelText("Help for Custom Agent instructions")).toBeTruthy();
    fireEvent.change(screen.getByLabelText("Custom Agent instructions"), {
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

  it("shows a built-in Agent as an immutable readable release", () => {
    const key = `${template.definition_id}@${template.version}`;
    const props: SettingsAgentCatalogSectionProps = {
      ...baseProps(),
      publishedAgents: [template],
      detailByAgent: {
        [key]: {
          ...template,
          developer_instructions: "Build and validate the bounded planning dataset.",
          content_sha256: "b".repeat(64),
          execution_semantics_sha256: "c".repeat(64),
        },
      },
    };

    render(<SettingsAgentCatalogSection {...props} />);

    expect(screen.getByText("Built-in")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "View" }));
    expect(screen.getByText("Build data")).toBeTruthy();
    expect(
      screen.getAllByText("supply_chain_data.build_planning_dataset"),
    ).toHaveLength(2);
    expect(screen.getByText("planning-dataset.v1")).toBeTruthy();
    expect(screen.getByText(/Build and validate the bounded planning dataset/)).toBeTruthy();
    expect(props.onLoadPublished).not.toHaveBeenCalled();
  });
});
