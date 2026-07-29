// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { DatasetReleaseDialog } from "./DatasetReleaseDialog";

const { listWorkspaceDatasetReleases, publishWorkspaceDatasetRelease } = vi.hoisted(
  () => ({
    listWorkspaceDatasetReleases: vi.fn(),
    publishWorkspaceDatasetRelease: vi.fn(),
  }),
);

vi.mock("../../../../browser/session", () => ({
  platformClient: {
    listWorkspaceDatasetReleases,
    publishWorkspaceDatasetRelease,
  },
}));

describe("DatasetReleaseDialog", () => {
  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it("imports explicit manifest roles and publishes one immutable release request", async () => {
    listWorkspaceDatasetReleases.mockResolvedValue([]);
    publishWorkspaceDatasetRelease.mockResolvedValue({
      id: "release-1",
      workspace_id: "workspace-1",
      dataset_id: "indonesia-warehouse-network-tutorial",
      version: "1.0.0",
      display_name: "Indonesia Warehouse Network Tutorial",
      description: "Inputs for the coverage tutorial.",
      state: "published",
      content_sha256: "a".repeat(64),
      failure_code: null,
      files: [],
      published_at: "2026-07-29T00:00:00Z",
      created_at: "2026-07-29T00:00:00Z",
      updated_at: "2026-07-29T00:00:00Z",
    });
    const onPublished = vi.fn();
    const { container } = render(
      <DatasetReleaseDialog
        workspaceId="workspace-1"
        onClose={vi.fn()}
        onPublished={onPublished}
      />,
    );
    const manifest = new File(
      [
        JSON.stringify({
          schema_version: "workspace_dataset_release.v1",
          dataset_id: "indonesia-warehouse-network-tutorial",
          version: "1.0.0",
          files: [{ path: "customers.csv.gz", role: "customers" }],
        }),
      ],
      "dataset-manifest.json",
      { type: "application/json" },
    );
    const customers = new File(["customers"], "customers.csv.gz", {
      type: "application/gzip",
    });
    const input = container.querySelector('input[type="file"]');
    expect(input).not.toBeNull();

    fireEvent.change(input as HTMLInputElement, {
      target: { files: [manifest, customers] },
    });

    await waitFor(() =>
      expect(
        (screen.getByLabelText("Dataset ID") as HTMLInputElement).value,
      ).toBe("indonesia-warehouse-network-tutorial"),
    );
    const roleInputs = screen.getAllByLabelText("File role") as HTMLInputElement[];
    expect(roleInputs.map((inputElement) => inputElement.value)).toEqual([
      "dataset_manifest",
      "customers",
    ]);
    fireEvent.change(screen.getByLabelText("What this data is for"), {
      target: { value: "Inputs for the coverage tutorial." },
    });
    fireEvent.click(screen.getByRole("button", { name: "Publish release" }));

    await waitFor(() =>
      expect(publishWorkspaceDatasetRelease).toHaveBeenCalledTimes(1),
    );
    expect(publishWorkspaceDatasetRelease.mock.calls[0][0]).toBe("workspace-1");
    expect(publishWorkspaceDatasetRelease.mock.calls[0][1]).toMatchObject({
      dataset_id: "indonesia-warehouse-network-tutorial",
      version: "1.0.0",
      files: [
        { logical_name: "dataset-manifest.json", role: "dataset_manifest" },
        { logical_name: "customers.csv.gz", role: "customers" },
      ],
    });
    expect(onPublished).toHaveBeenCalledTimes(1);
    expect(
      screen.getByText(
        "Published indonesia-warehouse-network-tutorial@1.0.0",
      ),
    ).toBeTruthy();
  });

  it("shows failed releases without treating them as available data", async () => {
    listWorkspaceDatasetReleases.mockResolvedValue([
      {
        id: "release-failed",
        workspace_id: "workspace-1",
        dataset_id: "network",
        version: "1.0.0",
        display_name: "Network",
        description: "Inputs",
        state: "failed",
        content_sha256: "b".repeat(64),
        failure_code: "workspace_io_failed",
        files: [],
        published_at: null,
        created_at: "2026-07-29T00:00:00Z",
        updated_at: "2026-07-29T00:00:00Z",
      },
    ]);

    render(
      <DatasetReleaseDialog
        workspaceId="workspace-1"
        onClose={vi.fn()}
        onPublished={vi.fn()}
      />,
    );

    expect(await screen.findByText("Failure: workspace_io_failed")).toBeTruthy();
  });

  it("copies the exact safe Dataset Release identity", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    listWorkspaceDatasetReleases.mockResolvedValue([
      {
        id: "0198d5b5-7d0f-7a62-8d9a-f6472dbfab12",
        workspace_id: "0198d5b5-7d0f-7a62-8d9a-f6472dbfab11",
        dataset_id: "delivery-commitment-audit",
        version: "1.0.0",
        display_name: "Delivery commitment audit",
        description: "Inputs",
        state: "published",
        content_sha256: "c".repeat(64),
        failure_code: null,
        files: [],
        published_at: "2026-07-29T00:00:00Z",
        created_at: "2026-07-29T00:00:00Z",
        updated_at: "2026-07-29T00:00:00Z",
      },
    ]);

    render(
      <DatasetReleaseDialog
        workspaceId="0198d5b5-7d0f-7a62-8d9a-f6472dbfab11"
        onClose={vi.fn()}
        onPublished={vi.fn()}
      />,
    );

    fireEvent.click(
      await screen.findByRole("button", {
        name: "Copy exact identity for Delivery commitment audit",
      }),
    );

    await waitFor(() => expect(writeText).toHaveBeenCalledTimes(1));
    expect(JSON.parse(writeText.mock.calls[0][0])).toEqual({
      workspace_id: "0198d5b5-7d0f-7a62-8d9a-f6472dbfab11",
      release_id: "0198d5b5-7d0f-7a62-8d9a-f6472dbfab12",
      dataset_id: "delivery-commitment-audit",
      version: "1.0.0",
      content_sha256: "c".repeat(64),
    });
    expect(screen.getByText("Copied")).toBeTruthy();
  });
});
