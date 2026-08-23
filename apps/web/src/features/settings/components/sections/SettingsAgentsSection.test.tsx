// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { SettingsAgentsSectionProps } from "@settings/hooks/useSettingsAgentsSection";
import { SettingsAgentsSection } from "./SettingsAgentsSection";

const baseProps = (): SettingsAgentsSectionProps => ({
  settings: {
    configPath: "/Users/me/.codex/config.toml",
    multiAgentEnabled: false,
    maxThreads: 6,
    maxDepth: 1,
    agents: [
      {
        name: "researcher",
        description: "Research-focused role",
        developerInstructions: "Investigate and propose safe changes.",
        configFile: "researcher.toml",
        resolvedPath: "/Users/me/.codex/agents/researcher.toml",
        managedByApp: true,
        fileExists: true,
      },
    ],
  },
  isLoading: false,
  isUpdatingCore: false,
  creatingAgent: false,
  updatingAgentName: null,
  deletingAgentName: null,
  readingConfigAgentName: null,
  writingConfigAgentName: null,
  error: null,
  onRefresh: vi.fn(),
  onSetMultiAgentEnabled: vi.fn(async () => true),
  onSetMaxThreads: vi.fn(async () => true),
  onSetMaxDepth: vi.fn(async () => true),
  onCreateAgent: vi.fn(async () => true),
  onUpdateAgent: vi.fn(async () => true),
  onDeleteAgent: vi.fn(async () => true),
  onReadAgentConfig: vi.fn(async () => "model = \"gpt-5-codex\""),
  onWriteAgentConfig: vi.fn(async () => true),
  modelOptions: [
    {
      id: "gpt-5-codex",
      model: "gpt-5-codex",
      displayName: "gpt-5-codex",
      description: "",
      supportedReasoningEfforts: [],
      defaultReasoningEffort: null,
      isDefault: true,
    },
  ],
  modelOptionsLoading: false,
  modelOptionsError: null,
});

describe("SettingsAgentsSection", () => {
  afterEach(() => {
    cleanup();
  });

  it("does not send developerInstructions when unchanged during edit", async () => {
    const props = baseProps();
    const onUpdateAgent = vi.fn(
      async (_input: Parameters<SettingsAgentsSectionProps["onUpdateAgent"]>[0]) => true,
    );
    render(<SettingsAgentsSection {...props} onUpdateAgent={onUpdateAgent} />);

    fireEvent.click(screen.getByRole("button", { name: "Edit" }));
    const nameInputs = screen.getAllByLabelText("Name") as HTMLInputElement[];
    fireEvent.change(nameInputs[1], { target: { value: "researcher-v2" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => {
      expect(onUpdateAgent).toHaveBeenCalledTimes(1);
    });
    const payload = onUpdateAgent.mock.calls[0]?.[0];
    if (!payload) {
      throw new Error("Expected update payload");
    }
    expect(payload).toMatchObject({
      originalName: "researcher",
      name: "researcher-v2",
      description: "Research-focused role",
      renameManagedFile: true,
    });
    expect(payload).not.toHaveProperty("developerInstructions");
  });

  it("updates max depth from stepper control", async () => {
    const props = baseProps();
    const onSetMaxDepth = vi.fn(async () => true);
    render(<SettingsAgentsSection {...props} onSetMaxDepth={onSetMaxDepth} />);

    fireEvent.click(screen.getByRole("button", { name: "Increase max depth" }));

    await waitFor(() => {
      expect(onSetMaxDepth).toHaveBeenCalledWith(2);
    });
  });

  it("does not increase max depth beyond 4", () => {
    const props = baseProps();
    const onSetMaxDepth = vi.fn(async () => true);
    render(
      <SettingsAgentsSection
        {...props}
        settings={{ ...props.settings!, maxDepth: 4 }}
        onSetMaxDepth={onSetMaxDepth}
      />,
    );

    const increaseDepthButton = screen.getByRole("button", { name: "Increase max depth" });
    expect((increaseDepthButton as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(increaseDepthButton);
    expect(onSetMaxDepth).not.toHaveBeenCalled();
  });
});
