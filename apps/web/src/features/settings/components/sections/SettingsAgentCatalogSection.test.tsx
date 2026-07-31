// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { SettingsAgentCatalogSectionProps } from "@settings/hooks/useSettingsAgentCatalogSection";
import { SettingsAgentCatalogSection } from "./SettingsAgentCatalogSection";

const template: SettingsAgentCatalogSectionProps["templates"][number] = {
  source: "repository",
  release_id: null,
  definition_id: "enterprise-data-agent",
  version: "4.0.0",
  display_name: "Enterprise Data Agent",
  description: "Inspects an exact Dataset Release.",
  responsibilities: ["Inspect data"],
  input_artifact_types: [],
  output_artifact_types: ["indonesia_dataset_inspection.v1"],
  required_capabilities: [
    "supply_chain_indonesia.inspect_indonesia_dataset_release",
  ],
  capability_template: null,
  dataset_releases: [],
  required_workspace_id: null,
};

function baseProps(): SettingsAgentCatalogSectionProps {
  return {
    definitions: [],
    publishedAgents: [],
    templates: [template],
    capabilityPackages: [],
    datasetReleases: [],
    workspaceNames: {},
    isLoadingDatasets: false,
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
    onLoadDatasetReleases: vi.fn(async () => undefined),
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
    expect(
      (screen.getByLabelText("Reviewed capability template") as HTMLSelectElement).value,
    ).toBe("");
    expect(screen.queryByText("Available capabilities")).toBeNull();
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
      target: { value: "repository:enterprise-data-agent@4.0.0" },
    });
    expect(
      screen.getByText(
        /supply_chain_indonesia\.inspect_indonesia_dataset_release/,
      ),
    ).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Create draft" }));

    await waitFor(() => expect(props.onSaveDraft).toHaveBeenCalledTimes(1));
    expect(vi.mocked(props.onSaveDraft).mock.calls[0][0]).toMatchObject({
      definition_id: "regional-data-reviewer",
      capability_template: {
        source: "repository_agent",
        definition_id: "enterprise-data-agent",
        version: "4.0.0",
        release_id: null,
      },
      output_artifact_types: ["indonesia_dataset_inspection.v1"],
    });
  });

  it("binds a Workspace capability package by immutable release id", async () => {
    const props = {
      ...baseProps(),
      capabilityPackages: [
        {
          release_id: "019ff890-8c28-7ef1-a2c4-b4562ffb7344",
          workspace_id: "019ff890-8c28-7ef1-a2c4-b4562ffb7555",
          package_id: "delivery-promise-tools",
          version: "1.0.0",
          display_name: "Delivery Promise Tools",
          description: "Checks delivery promises.",
          capability_root_id: "local-delivery-promise-tools-1-0-0",
          capabilities: ["delivery_promise.check_promises"],
          mcp_server_names: ["delivery_promise"],
          tool_names: ["check_promises"],
          input_artifact_types: ["delivery-requests.v1"],
          output_artifact_types: ["delivery-promise-report.v1"],
          includes_skills: true,
          source: "workspace_release" as const,
          content_sha256: "d".repeat(64),
        },
      ],
      datasetReleases: [
        {
          id: "019ff890-8c28-7ef1-a2c4-b4562ffb7666",
          workspace_id: "019ff890-8c28-7ef1-a2c4-b4562ffb7555",
          dataset_id: "delivery-requests",
          version: "1.0.0",
          display_name: "Delivery Requests",
          description: "Tutorial delivery requests.",
          state: "published" as const,
          content_sha256: "e".repeat(64),
          failure_code: null,
          files: [],
          published_at: "2026-07-29T00:00:00Z",
          created_at: "2026-07-29T00:00:00Z",
          updated_at: "2026-07-29T00:00:00Z",
        },
      ],
      workspaceNames: {
        "019ff890-8c28-7ef1-a2c4-b4562ffb7555": "Tutorial Workspace",
      },
    };
    render(<SettingsAgentCatalogSection {...props} />);

    fireEvent.change(screen.getByLabelText("Agent ID"), {
      target: { value: "delivery-promise-agent" },
    });
    fireEvent.change(screen.getByLabelText("Display name"), {
      target: { value: "Delivery Promise Agent" },
    });
    fireEvent.change(screen.getByLabelText("Description"), {
      target: { value: "Checks promised delivery dates." },
    });
    fireEvent.change(screen.getByLabelText("Responsibilities"), {
      target: { value: "Check delivery promises" },
    });
    fireEvent.change(screen.getByLabelText("Custom Agent instructions"), {
      target: { value: "Use the provided Tool and report every failed row." },
    });
    fireEvent.change(screen.getByLabelText("Reviewed capability template"), {
      target: { value: "workspace:019ff890-8c28-7ef1-a2c4-b4562ffb7344" },
    });
    fireEvent.click(
      screen.getByRole("checkbox", {
        name: /Delivery Requests · 1\.0\.0/,
      }),
    );
    fireEvent.click(screen.getByRole("button", { name: "Create draft" }));

    await waitFor(() => expect(props.onSaveDraft).toHaveBeenCalledTimes(1));
    expect(vi.mocked(props.onSaveDraft).mock.calls[0][0]).toMatchObject({
      capability_template: {
        source: "workspace_package_release",
        definition_id: "delivery-promise-tools",
        version: "1.0.0",
        release_id: "019ff890-8c28-7ef1-a2c4-b4562ffb7344",
      },
      input_artifact_types: ["delivery-requests.v1"],
      output_artifact_types: ["delivery-promise-report.v1"],
      dataset_release_ids: ["019ff890-8c28-7ef1-a2c4-b4562ffb7666"],
    });
  });

  it("blocks a draft when every template-provided Artifact output is removed", async () => {
    const props = baseProps();
    render(<SettingsAgentCatalogSection {...props} />);

    fireEvent.change(screen.getByLabelText("Reviewed capability template"), {
      target: { value: "repository:enterprise-data-agent@4.0.0" },
    });
    expect(
      screen.getByText(/Type IDs are fixed by the reviewed template/),
    ).toBeTruthy();
    fireEvent.click(
      screen.getByRole("checkbox", {
        name: "Output · indonesia_dataset_inspection.v1",
      }),
    );
    fireEvent.click(screen.getByRole("button", { name: "Create draft" }));

    expect(
      screen.getByText(/Select at least one Artifact output/),
    ).toBeTruthy();
    expect(props.onSaveDraft).not.toHaveBeenCalled();
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
    expect(screen.getByText("Inspect data")).toBeTruthy();
    expect(
      screen.getAllByText(
        "supply_chain_indonesia.inspect_indonesia_dataset_release",
      ).length,
    ).toBe(1);
    expect(screen.getAllByText("indonesia_dataset_inspection.v1").length).toBeGreaterThanOrEqual(1);
    const technicalContract = screen.getByText("Technical contract").closest("details");
    expect(technicalContract?.open).toBe(false);
    fireEvent.click(screen.getByText("Technical contract"));
    expect(technicalContract?.open).toBe(true);
    expect(screen.getByText(/Build and validate the bounded planning dataset/)).toBeTruthy();
    expect(props.onLoadPublished).not.toHaveBeenCalled();
  });

  it("routes an empty Dataset state to the shared publisher", () => {
    const props = baseProps();
    const onOpenDatasetPublisher = vi.fn();
    render(
      <SettingsAgentCatalogSection
        {...props}
        studioMode
        onOpenDatasetPublisher={onOpenDatasetPublisher}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "New Agent" }));
    fireEvent.change(screen.getByLabelText("Reviewed capability template"), {
      target: { value: "repository:enterprise-data-agent@4.0.0" },
    });
    fireEvent.click(screen.getByTestId("agent-studio-add-data"));

    expect(onOpenDatasetPublisher).toHaveBeenCalledWith(
      null,
      expect.any(HTMLButtonElement),
    );
  });

  it("copies a built-in exact release into a new user-owned definition", () => {
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

    render(<SettingsAgentCatalogSection {...props} studioMode />);
    fireEvent.click(screen.getByRole("button", { name: "View details" }));
    fireEvent.click(screen.getByRole("button", { name: "Create custom Agent" }));

    expect((screen.getByLabelText("Agent ID") as HTMLInputElement).value).toBe("");
    expect((screen.getByLabelText("Version") as HTMLInputElement).value).toBe("1.0.0");
    expect(
      (screen.getByLabelText("Reviewed capability template") as HTMLSelectElement).value,
    ).toBe("repository:enterprise-data-agent@4.0.0");
    expect(
      (screen.getByLabelText("Custom Agent instructions") as HTMLTextAreaElement).value,
    ).toBe("Build and validate the bounded planning dataset.");
  });

  it("creates a new version from the complete exact user release", () => {
    const userAgent = {
      ...template,
      source: "user_release" as const,
      release_id: "agent-release-1",
      capability_template: {
        source: "repository_agent" as const,
        definition_id: template.definition_id,
        version: template.version,
        release_id: null,
      },
    };
    const key = `${userAgent.definition_id}@${userAgent.version}`;
    const props: SettingsAgentCatalogSectionProps = {
      ...baseProps(),
      definitions: [
        {
          id: "definition-1",
          definition_id: userAgent.definition_id,
          display_name: userAgent.display_name,
          description: userAgent.description,
          owner_user_id: "user-1",
          draft: null,
          releases: [
            {
              id: "agent-release-1",
              definition_id: userAgent.definition_id,
              version: userAgent.version,
              display_name: userAgent.display_name,
              description: userAgent.description,
              content_sha256: "b".repeat(64),
              published_at: "2026-07-29T00:00:00Z",
            },
          ],
          created_at: "2026-07-29T00:00:00Z",
          updated_at: "2026-07-29T00:00:00Z",
        },
      ],
      publishedAgents: [userAgent],
      detailByAgent: {
        [key]: {
          ...userAgent,
          developer_instructions: "Preserve the complete reviewed method.",
          content_sha256: "b".repeat(64),
          execution_semantics_sha256: "c".repeat(64),
        },
      },
    };

    render(<SettingsAgentCatalogSection {...props} studioMode />);
    fireEvent.click(screen.getByRole("button", { name: "View details" }));
    fireEvent.click(screen.getByRole("button", { name: "Create new version" }));

    expect((screen.getByLabelText("Agent ID") as HTMLInputElement).value).toBe(
      userAgent.definition_id,
    );
    expect((screen.getByLabelText("Version") as HTMLInputElement).value).toBe("4.0.1");
    expect(
      (screen.getByLabelText("Custom Agent instructions") as HTMLTextAreaElement).value,
    ).toBe("Preserve the complete reviewed method.");
  });
});
