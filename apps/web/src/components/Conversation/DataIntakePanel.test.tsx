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
    requirementProfile: null,
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
  onConfirmProfile: vi.fn(),
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

  it("shows profile confirmation instead of claiming the Agent is still processing", () => {
    render(
      <DataIntakePanel
        session={{
          ...session("workspace_data"),
          inputRequests: [{
            requestId: "request-1",
            taskId: "task-1",
            intakeId: "intake-1",
            kind: "confirm_profile",
            sessionRevision: 1,
            status: "open",
            prompt: "Review and confirm the complete planning data profile.",
            value: null,
          }],
        }}
        loading={false}
        error={null}
        {...handlers}
      />,
    );

    expect(screen.getByText("Awaiting profile confirmation")).not.toBeNull();
    expect(screen.getByText("Confirm whole profile")).not.toBeNull();
    expect(screen.queryByText("输入已提交，Network/Data Agent 正在完成下一步画像、归一化或检查。")).toBeNull();
  });
});
