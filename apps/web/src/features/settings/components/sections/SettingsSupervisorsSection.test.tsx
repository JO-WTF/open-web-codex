// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { SettingsSupervisorsSectionProps } from "@settings/hooks/useSettingsSupervisorsSection";
import { SettingsSupervisorsSection } from "./SettingsSupervisorsSection";

const agents: SettingsSupervisorsSectionProps["agents"] = [
  {
    source: "repository",
    release_id: null,
    definition_id: "enterprise-data-agent",
    version: "1.6.0",
    display_name: "Enterprise Data Agent",
    description: "Builds the planning dataset.",
    responsibilities: ["Build data"],
    input_artifact_types: [],
    output_artifact_types: ["planning-dataset.v1"],
    required_capabilities: ["supply_chain_data.build_planning_dataset"],
    capability_template: null,
  },
  {
    source: "repository",
    release_id: null,
    definition_id: "enterprise-network-planning-agent",
    version: "1.5.0",
    display_name: "Enterprise Network Planning Agent",
    description: "Compares network scenarios.",
    responsibilities: ["Compare scenarios"],
    input_artifact_types: ["planning-dataset.v1"],
    output_artifact_types: ["scenario_comparison.v1"],
    required_capabilities: ["supply_chain_planner.compare_network_scenarios"],
    capability_template: null,
  },
];

const baseProps = (): SettingsSupervisorsSectionProps => ({
  definitions: [],
  publishedPolicies: [],
  agents,
  instructionPolicies: [
    {
      release_id: null,
      policy_id: "platform-supervisor-behavior",
      version: "1.0.0",
      display_name: "Platform Supervisor behavior",
      description: "Platform boundaries.",
      source: "repository",
      content_sha256: "f".repeat(64),
    },
  ],
  instructionPolicyDetails: {
    "platform-supervisor-behavior@1.0.0": {
      release_id: null,
      policy_id: "platform-supervisor-behavior",
      version: "1.0.0",
      display_name: "Platform Supervisor behavior",
      description: "Platform boundaries.",
      source: "repository",
      platform_instructions: "Use only authorized Runtime capabilities.",
      content_sha256: "f".repeat(64),
    },
  },
  canPublishInstructionPolicy: true,
  isPublishingInstructionPolicy: false,
  isLoading: false,
  actionDefinitionId: null,
  loadingPolicyKey: null,
  error: null,
  validationByDefinition: {},
  detailByPolicy: {},
  onRefresh: vi.fn(),
  onSaveDraft: vi.fn(async () => null),
  onValidate: vi.fn(async () => null),
  onPublish: vi.fn(async () => null),
  onLoadPublished: vi.fn(async () => null),
  onPublishInstructionPolicy: vi.fn(async () => null),
});

describe("SettingsSupervisorsSection", () => {
  afterEach(cleanup);

  it("separates the Supervisor directory from the create form in Studio mode", () => {
    const props = baseProps();
    render(<SettingsSupervisorsSection {...props} studioMode />);

    expect(screen.getByText("Supervisor directory")).toBeTruthy();
    expect(screen.queryByLabelText("Policy ID")).toBeNull();
    const newSupervisorButton = screen.getByRole("button", {
      name: "New Supervisor",
    });
    expect(
      newSupervisorButton.classList.contains("settings-studio-create-button"),
    ).toBe(true);
    fireEvent.click(newSupervisorButton);
    expect(screen.getByLabelText("Policy ID")).toBeTruthy();
    expect(screen.queryByText("Supervisor directory")).toBeNull();
    fireEvent.click(
      screen.getByRole("button", { name: "Back to Supervisor directory" }),
    );
    expect(screen.getByText("Supervisor directory")).toBeTruthy();
  });

  it("keeps platform instruction publication unavailable to non-platform owners", () => {
    const props = baseProps();
    render(
      <SettingsSupervisorsSection
        {...props}
        canPublishInstructionPolicy={false}
      />,
    );

    expect(
      screen.getByText("Platform contracts are read-only for your account."),
    ).toBeTruthy();
    expect(
      screen.queryByRole("button", { name: "Publish immutable version" }),
    ).toBeNull();
  });

  it("lets the platform Owner publish an immutable behavior-contract version", async () => {
    const props = baseProps();
    render(<SettingsSupervisorsSection {...props} />);

    fireEvent.click(screen.getByText("Publish a platform contract version"));
    fireEvent.change(screen.getByLabelText("Platform instructions"), {
      target: { value: "Use only authorized Runtime capabilities." },
    });
    fireEvent.click(
      screen.getByRole("button", { name: "Publish immutable version" }),
    );

    await waitFor(() =>
      expect(props.onPublishInstructionPolicy).toHaveBeenCalledWith({
        policy_id: "platform-supervisor-behavior",
        version: "1.1.0",
        display_name: "Platform Supervisor behavior",
        description: "Platform boundaries and reliable orchestration behavior.",
        platform_instructions: "Use only authorized Runtime capabilities.",
      }),
    );
  });

  it("derives typed Agent handoffs from the published catalog", async () => {
    const props = baseProps();
    render(<SettingsSupervisorsSection {...props} />);

    fireEvent.click(
      screen.getByRole("checkbox", { name: /Enterprise Data Agent/ }),
    );
    fireEvent.click(
      screen.getByRole("checkbox", {
        name: /Enterprise Network Planning Agent/,
      }),
    );
    expect(screen.getByText("planning-dataset.v1")).toBeTruthy();
    fireEvent.change(
      screen.getByLabelText("Spawn limit for Enterprise Data Agent"),
      {
        target: { value: "2" },
      },
    );
    fireEvent.click(screen.getByLabelText("Required planning-dataset.v1"));
    fireEvent.click(
      screen.getByLabelText("Deliver planning-dataset.v1 to supervisor"),
    );

    fireEvent.change(screen.getByLabelText("Policy ID"), {
      target: { value: "network-supervisor" },
    });
    const displayNameInputs = screen.getAllByLabelText("Display name");
    fireEvent.change(displayNameInputs[displayNameInputs.length - 1], {
      target: { value: "Network Supervisor" },
    });
    const descriptionInputs = screen.getAllByLabelText("Description");
    fireEvent.change(descriptionInputs[descriptionInputs.length - 1], {
      target: { value: "Coordinates a planning workflow." },
    });
    fireEvent.change(screen.getByLabelText("Responsibilities"), {
      target: { value: "Coordinate agents\nPublish a recommendation" },
    });
    expect(screen.queryByText("How to define a useful Supervisor")).toBeNull();
    expect(screen.getByLabelText("Help for Policy ID")).toBeTruthy();
    expect(screen.getByLabelText("Help for Allowed Agents")).toBeTruthy();
    expect(
      screen.getByText(/custom instructions cannot override/),
    ).toBeTruthy();
    fireEvent.change(screen.getByLabelText("Custom Supervisor instructions"), {
      target: { value: "Delegate data preparation before scenario analysis." },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create draft" }));

    await waitFor(() => expect(props.onSaveDraft).toHaveBeenCalledTimes(1));
    const request = vi.mocked(props.onSaveDraft).mock.calls[0][0];
    expect(request.instruction_policy).toEqual({
      policy_id: "platform-supervisor-behavior",
      version: "1.0.0",
    });
    expect(request.custom_instructions).toBe(
      "Delegate data preparation before scenario analysis.",
    );
    expect(request.agents).toHaveLength(2);
    expect(request.agents[0]).toMatchObject({ spawn_limit: 2 });
    expect(request.artifact_contracts).toContainEqual({
      artifact_type: "planning-dataset.v1",
      producer_agent: "enterprise-data-agent@1.6.0",
      consumer_agents: [
        "enterprise-network-planning-agent@1.5.0",
        "supervisor",
      ],
      required: false,
    });
  });

  it("shows a built-in Supervisor as an immutable readable release", () => {
    const policy = {
      policy_id: "enterprise-supervisor-copilot",
      version: "1.8.0",
      display_name: "Enterprise Supervisor Copilot",
      description: "Coordinates governed planning Agents.",
      source: "repository" as const,
    };
    const key = `${policy.policy_id}@${policy.version}`;
    const props: SettingsSupervisorsSectionProps = {
      ...baseProps(),
      publishedPolicies: [policy],
      detailByPolicy: {
        [key]: {
          ...policy,
          responsibilities: ["Decompose work into bounded assignments."],
          instruction_policy: baseProps().instructionPolicies[0],
          platform_instructions: "Use only authorized Runtime capabilities.",
          custom_instructions:
            "Coordinate the selected Agents and publish the final report.",
          agents: [
            {
              definition_id: "enterprise-data-agent",
              version: "1.6.0",
              release_id: null,
              spawn_limit: 1,
            },
          ],
          artifact_contracts: [
            {
              artifact_type: "planning-dataset.v1",
              producer_agent: "enterprise-data-agent@1.6.0",
              consumer_agents: ["supervisor"],
              required: true,
            },
          ],
          max_active_child_agents: 2,
          content_sha256: "a".repeat(64),
          execution_semantics_sha256: "d".repeat(64),
        },
      },
    };

    render(<SettingsSupervisorsSection {...props} />);

    expect(screen.getByText("Built-in")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "View" }));
    expect(
      screen.getByText("Decompose work into bounded assignments."),
    ).toBeTruthy();
    expect(screen.getByText(/spawn limit 1/)).toBeTruthy();
    expect(screen.getByText(/planning-dataset.v1/)).toBeTruthy();
    expect(screen.getByText(/Coordinate the selected Agents/)).toBeTruthy();
    expect(props.onLoadPublished).not.toHaveBeenCalled();
  });
});
