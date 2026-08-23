// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import CopilotManager from "./CopilotManager";

const status = {
  packages: [
    { packageId: "configured-package", available: true, displayName: "Configured package" },
    { packageId: "inactive-package", available: true, displayName: "Inactive package" },
  ],
  installations: [
    {
      packageId: "configured-package",
      sourceRevision: "a".repeat(64),
      active: true,
      state: "configured" as const,
      restartRequired: true,
      managedSkillIds: ["summary-skill"],
      managedAgentRoleIds: ["reviewer_role"],
      runtimeDiscoveredSkillIds: ["summary-skill"],
    },
    {
      packageId: "inactive-package",
      sourceRevision: "b".repeat(64),
      active: false,
      state: "failed" as const,
      restartRequired: false,
      managedSkillIds: [],
      managedAgentRoleIds: [],
      runtimeDiscoveredSkillIds: [],
      failureCode: "profile_composition_failed",
    },
    {
      packageId: "missing-package",
      sourceRevision: "c".repeat(64),
      active: true,
      state: "unavailable" as const,
      restartRequired: true,
      managedSkillIds: ["missing-skill"],
      managedAgentRoleIds: [],
      runtimeDiscoveredSkillIds: [],
      failureCode: "application_source_unavailable",
    },
  ],
};

describe("CopilotManager", () => {
  afterEach(cleanup);

  it("shows bounded status, managed ids, restart state, and safe failures", () => {
    render(
      <CopilotManager
        status={status}
        loading={false}
        error={null}
        onRefresh={vi.fn(async () => undefined)}
        onActivate={vi.fn(async () => undefined)}
        onDeactivate={vi.fn(async () => undefined)}
      />,
    );

    expect(screen.getByRole("region", { name: "Copilot installations" })).toBeTruthy();
    expect(screen.getByText("Configured")).toBeTruthy();
    expect(screen.getByText("Failed")).toBeTruthy();
    expect(screen.getAllByText("Unavailable").length).toBeGreaterThan(0);
    expect(screen.getByText("summary-skill")).toBeTruthy();
    expect(screen.getByText("reviewer_role")).toBeTruthy();
    expect(screen.getByText("profile_composition_failed")).toBeTruthy();
    expect(screen.getByText("application_source_unavailable")).toBeTruthy();
    expect(screen.getAllByText(/Cold restart required/)).toHaveLength(2);
    expect(screen.getByText(/operator sync and a cold restart/)).toBeTruthy();
  });

  it("activates available packages and deactivates missing active records", async () => {
    const onActivate = vi.fn(async () => undefined);
    const onDeactivate = vi.fn(async () => undefined);
    render(
      <CopilotManager
        status={status}
        loading={false}
        error={null}
        onRefresh={vi.fn(async () => undefined)}
        onActivate={onActivate}
        onDeactivate={onDeactivate}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Activate Inactive package" }));
    await waitFor(() => expect(onActivate).toHaveBeenCalledWith("inactive-package"));

    fireEvent.click(screen.getByRole("button", { name: "Deactivate missing-package" }));
    await waitFor(() => expect(onDeactivate).toHaveBeenCalledWith("missing-package"));

    const missingCard = screen
      .getByRole("button", { name: "Deactivate missing-package" })
      .closest("article");
    expect(missingCard).toBeTruthy();
    expect(within(missingCard as HTMLElement).getAllByText("Unavailable").length)
      .toBeGreaterThan(0);
  });

  it("reports action failures without inventing success", async () => {
    render(
      <CopilotManager
        status={status}
        loading={false}
        error={null}
        onRefresh={vi.fn(async () => undefined)}
        onActivate={vi.fn(async () => { throw new Error("Activation was rejected"); })}
        onDeactivate={vi.fn(async () => undefined)}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Activate Inactive package" }));
    expect((await screen.findByRole("alert")).textContent).toContain("Activation was rejected");
  });

  it("prevents overlapping activation changes", async () => {
    let finishActivation: () => void = () => undefined;
    const activation = new Promise<void>((resolve) => {
      finishActivation = resolve;
    });
    const onActivate = vi.fn(() => activation);
    render(
      <CopilotManager
        status={status}
        loading={false}
        error={null}
        onRefresh={vi.fn(async () => undefined)}
        onActivate={onActivate}
        onDeactivate={vi.fn(async () => undefined)}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Activate Inactive package" }));
    await waitFor(() => expect(onActivate).toHaveBeenCalledOnce());
    expect(screen.getByRole("button", { name: "Deactivate missing-package" }))
      .toHaveProperty("disabled", true);

    finishActivation();
    await waitFor(() => expect(
      screen.getByRole("button", { name: "Deactivate missing-package" }),
    ).toHaveProperty("disabled", false));
  });
});
