// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
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
  agents,
  isLoading: false,
  actionDefinitionId: null,
  error: null,
  validationByDefinition: {},
  onRefresh: vi.fn(),
  onSaveDraft: vi.fn(async () => null),
  onValidate: vi.fn(async () => null),
  onPublish: vi.fn(async () => null),
});

describe("SettingsSupervisorsSection", () => {
  afterEach(cleanup);

  it("derives typed Agent handoffs from the published catalog", async () => {
    const props = baseProps();
    render(<SettingsSupervisorsSection {...props} />);

    fireEvent.click(
      screen.getByRole("checkbox", { name: /Enterprise Data Agent/ }),
    );
    fireEvent.click(
      screen.getByRole("checkbox", { name: /Enterprise Network Planning Agent/ }),
    );
    expect(screen.getByText("planning-dataset.v1")).toBeTruthy();

    fireEvent.change(screen.getByLabelText("Policy ID"), {
      target: { value: "network-supervisor" },
    });
    fireEvent.change(screen.getByLabelText("Display name"), {
      target: { value: "Network Supervisor" },
    });
    fireEvent.change(screen.getByLabelText("Description"), {
      target: { value: "Coordinates a planning workflow." },
    });
    fireEvent.change(screen.getByLabelText("Responsibilities"), {
      target: { value: "Coordinate agents\nPublish a recommendation" },
    });
    fireEvent.change(screen.getByLabelText("Supervisor instructions"), {
      target: { value: "Delegate data preparation before scenario analysis." },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create draft" }));

    await waitFor(() => expect(props.onSaveDraft).toHaveBeenCalledTimes(1));
    const request = vi.mocked(props.onSaveDraft).mock.calls[0][0];
    expect(request.agents).toHaveLength(2);
    expect(request.artifact_contracts).toContainEqual({
      artifact_type: "planning-dataset.v1",
      producer_agent: "enterprise-data-agent@1.6.0",
      consumer_agents: ["enterprise-network-planning-agent@1.5.0"],
      required: true,
    });
  });
});
