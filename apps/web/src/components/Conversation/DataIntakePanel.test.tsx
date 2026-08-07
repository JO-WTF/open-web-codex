// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { DataIntakeSessionSummary } from "../../../browser/types";
import DataIntakePanel from "./DataIntakePanel";

afterEach(cleanup);

function session(dataClassification: "workspace_data" | "synthetic_demo") {
  return {
    intakeId: "intake-1",
    taskId: "task-1",
    workspaceId: "workspace-1",
    contract: {
      contractId: "warehouse-network-planning",
      version: "1.0.0",
      displayName: "Warehouse network planning",
      description: "Confirmed warehouse-network inputs.",
      contentSha256: "a".repeat(64),
      requiredEntities: [],
      businessParameters: [],
    },
    status: "active",
    inputRevision: 1,
    mappingRevision: 0,
    gapFingerprint: "gap",
    evidenceFingerprint: "evidence",
    gaps: [],
    candidates: [],
    confirmedMapping: [],
    parameters: [],
    answers: [],
    attemptCount: 0,
    failureCode: null,
    failureSummary: null,
    inputRequests: [],
    requirementProfile: {
      title: "本次规划的数据需求",
      entities: [{ name: "City demand", fields: [{ name: "city_id" }] }],
    },
    sourceProfile: { schemaVersion: "source_profile.v1", dataClassification },
    mappingProposal: null,
    readinessReview: null,
  } as DataIntakeSessionSummary;
}

const handlers = {
  onRefresh: vi.fn(),
  onOpenUpload: vi.fn(),
  onConfirmMapping: vi.fn(),
  onSubmitParameters: vi.fn(),
  onConfirmAnalysis: vi.fn(),
  onRequestChange: vi.fn(),
};

describe("DataIntakePanel source classification", () => {
  it("marks synthetic Demo intake explicitly", () => {
    render(
      <DataIntakePanel
        session={session("synthetic_demo")}
        loading={false}
        error={null}
        {...handlers}
      />,
    );

    expect(screen.getByText("Synthetic demo")).not.toBeNull();
  });

  it("does not mark ordinary Workspace data as Demo", () => {
    render(
      <DataIntakePanel
        session={session("workspace_data")}
        loading={false}
        error={null}
        {...handlers}
      />,
    );

    expect(screen.queryByText("Synthetic demo")).toBeNull();
  });

  it("does not show an empty intake session before an Agent produces evidence", () => {
    render(
      <DataIntakePanel
        session={{
          ...session("workspace_data"),
          evidenceFingerprint: "",
          sourceProfile: null,
          requirementProfile: null,
        }}
        loading={false}
        error={null}
        {...handlers}
      />,
    );

    expect(screen.queryByLabelText("Data preparation")).toBeNull();
  });

  it("does not show source evidence before the requirement profile is published", () => {
    render(
      <DataIntakePanel
        session={{
          ...session("workspace_data"),
          sourceProfile: { schemaVersion: "source_profile.v1", dataClassification: "workspace_data" },
          requirementProfile: null,
        }}
        loading={false}
        error={null}
        {...handlers}
      />,
    );

    expect(screen.queryByLabelText("Data preparation")).toBeNull();
  });

  it("shows the published profile without requiring confirmation", () => {
    render(
      <DataIntakePanel
        session={{
          ...session("workspace_data"),
          requirementProfile: {
            title: "本次规划的数据需求",
            entities: [{ name: "City demand", fields: [{ name: "city_id" }] }],
          },
        }}
        loading={false}
        error={null}
        {...handlers}
      />,
    );

    expect(screen.getByText("Planning data requirements")).not.toBeNull();
    expect(screen.getByText(/需求已生成，数据准备会自动继续/)).not.toBeNull();
    expect(screen.queryByText("Confirm whole profile")).toBeNull();
  });

});
