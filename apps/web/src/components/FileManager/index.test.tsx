// @vitest-environment jsdom
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import FileManager from "./index";

const {
  listWorkspaceDatasetReleases,
  publishWorkspaceDatasetRelease,
} = vi.hoisted(() => ({
  listWorkspaceDatasetReleases: vi.fn(),
  publishWorkspaceDatasetRelease: vi.fn(),
}));

vi.mock("../../../browser/session", () => ({
  platformClient: {
    listWorkspaceDatasetReleases,
    publishWorkspaceDatasetRelease,
  },
}));

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("FileManager", () => {
  it("does not load supplementary file data while its tab is inactive", () => {
    const listFiles = vi.fn().mockResolvedValue(["README.md"]);
    const readFile = vi.fn().mockResolvedValue({ content: "# Project", truncated: false });
    render(
      <FileManager
        workspaceId="workspace-1"
        selectedPath="README.md"
        onSelectedPathChange={vi.fn()}
        onClose={vi.fn()}
        panelWidth={360}
        onPanelWidthChange={vi.fn()}
        listFiles={listFiles}
        readFile={readFile}
        loadGitStatus={vi.fn().mockResolvedValue({ files: [] })}
        embedded
        enabled={false}
      />,
    );

    expect(listFiles).not.toHaveBeenCalled();
    expect(readFile).not.toHaveBeenCalled();
  });

  it("shows git states and previews a selected file", async () => {
    const readFile = vi.fn().mockResolvedValue({ content: "export const value = 1;", truncated: false });
    render(
      <FileManager
        workspaceId="workspace-1"
        selectedPath={null}
        onSelectedPathChange={vi.fn()}
        onClose={vi.fn()}
        panelWidth={360}
        onPanelWidthChange={vi.fn()}
        listFiles={vi.fn().mockResolvedValue(["README.md", "src/config.ts"])}
        readFile={readFile}
        loadGitStatus={vi.fn().mockResolvedValue({ files: [{ path: "README.md", status: "??", additions: 1, deletions: 0 }] })}
      />,
    );

    expect(await screen.findByText("README.md")).toBeTruthy();
    expect(screen.getByText("A")).toBeTruthy();
    fireEvent.click(screen.getByText("src"));
    expect(await screen.findByText("config.ts")).toBeTruthy();
  });

  it("opens the single Dataset Release publisher from the production Files panel", async () => {
    listWorkspaceDatasetReleases.mockResolvedValue([]);
    render(
      <FileManager
        workspaceId="workspace-1"
        selectedPath={null}
        onSelectedPathChange={vi.fn()}
        onClose={vi.fn()}
        panelWidth={360}
        onPanelWidthChange={vi.fn()}
        listFiles={vi.fn().mockResolvedValue([])}
        readFile={vi.fn().mockResolvedValue({ content: "", truncated: false })}
        loadGitStatus={vi.fn().mockResolvedValue({ files: [] })}
        embedded
      />,
    );

    expect(await screen.findByText("No Workspace files yet")).toBeTruthy();
    fireEvent.click(screen.getByTestId("workspace-files-empty-add-data"));

    expect(
      await screen.findByRole("heading", { name: "Publish a data release" }),
    ).toBeTruthy();
    expect(listWorkspaceDatasetReleases).toHaveBeenCalledWith("workspace-1");
    fireEvent.click(screen.getByRole("button", { name: "Close data release dialog" }));
    expect(
      screen.queryByRole("heading", { name: "Publish a data release" }),
    ).toBeNull();
  });

  it("refreshes Workspace files after a Dataset Release is published", async () => {
    listWorkspaceDatasetReleases.mockResolvedValue([]);
    publishWorkspaceDatasetRelease.mockResolvedValue({
      id: "release-1",
      workspace_id: "workspace-1",
      dataset_id: "network-inputs",
      version: "1.0.0",
      display_name: "Network inputs",
      description: "Inputs for network planning.",
      state: "published",
      content_sha256: "a".repeat(64),
      failure_code: null,
      files: [],
      published_at: "2026-07-30T00:00:00Z",
      created_at: "2026-07-30T00:00:00Z",
      updated_at: "2026-07-30T00:00:00Z",
    });
    const listFiles = vi.fn()
      .mockResolvedValueOnce([])
      .mockResolvedValue(["datasets/network-inputs/1.0.0/data.csv"]);
    const { container } = render(
      <FileManager
        workspaceId="workspace-1"
        selectedPath={null}
        onSelectedPathChange={vi.fn()}
        onClose={vi.fn()}
        panelWidth={360}
        onPanelWidthChange={vi.fn()}
        listFiles={listFiles}
        readFile={vi.fn().mockResolvedValue({ content: "", truncated: false })}
        loadGitStatus={vi.fn().mockResolvedValue({ files: [] })}
        embedded
      />,
    );

    await screen.findByText("No Workspace files yet");
    fireEvent.click(screen.getByTestId("workspace-files-empty-add-data"));
    fireEvent.change(screen.getByLabelText("Dataset ID"), {
      target: { value: "network-inputs" },
    });
    fireEvent.change(screen.getByLabelText("Name"), {
      target: { value: "Network inputs" },
    });
    fireEvent.change(screen.getByLabelText("What this data is for"), {
      target: { value: "Inputs for network planning." },
    });
    const fileInput = document.body.querySelector<HTMLInputElement>(
      '.dataset-release-file-input[type="file"]',
    );
    expect(fileInput).not.toBeNull();
    fireEvent.change(fileInput!, {
      target: {
        files: [new File(["data"], "data.csv", { type: "text/csv" })],
      },
    });

    await waitFor(() =>
      expect(
        (screen.getByRole("button", {
          name: "Publish release",
        }) as HTMLButtonElement).disabled,
      ).toBe(false),
    );
    fireEvent.click(screen.getByRole("button", { name: "Publish release" }));

    await waitFor(() => expect(listFiles).toHaveBeenCalledTimes(2));
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    fireEvent.click(await screen.findByText("datasets"));
    fireEvent.click(await screen.findByText("network-inputs"));
    fireEvent.click(await screen.findByText("1.0.0"));
    expect(await screen.findByText("data.csv")).toBeTruthy();
    expect(container.querySelector(".web-file-manager")).toBeTruthy();
  });

  it("loads a file selected by an external message link", async () => {
    const readFile = vi.fn().mockResolvedValue({ content: "# Project", truncated: false });
    render(
      <FileManager
        workspaceId="workspace-1"
        selectedPath="README.md"
        onSelectedPathChange={vi.fn()}
        onClose={vi.fn()}
        panelWidth={360}
        onPanelWidthChange={vi.fn()}
        listFiles={vi.fn().mockResolvedValue(["README.md"])}
        readFile={readFile}
        loadGitStatus={vi.fn().mockResolvedValue({ files: [] })}
      />,
    );
    expect(await screen.findByRole("heading", { name: "Project" })).toBeTruthy();
    expect(readFile).toHaveBeenCalledWith("workspace-1", "README.md");
  });

  it("renders markdown previews and opens relative file links in the file manager", async () => {
    const onSelectedPathChange = vi.fn();
    render(
      <FileManager
        workspaceId="workspace-1"
        selectedPath="docs/guide.md"
        onSelectedPathChange={onSelectedPathChange}
        onClose={vi.fn()}
        panelWidth={360}
        onPanelWidthChange={vi.fn()}
        listFiles={vi.fn().mockResolvedValue(["docs/guide.md", "src/config.ts"])}
        readFile={vi.fn().mockResolvedValue({
          content: "# Guide\n\n**Ready**\n\n| Item | State |\n| --- | --- |\n| Build | OK |\n\n[Config](../src/config.ts)",
          truncated: false,
        })}
        loadGitStatus={vi.fn().mockResolvedValue({ files: [] })}
      />,
    );

    expect(await screen.findByRole("heading", { name: "Guide" })).toBeTruthy();
    expect(screen.getByText("Ready").tagName).toBe("STRONG");
    expect(screen.getByRole("table")).toBeTruthy();
    fireEvent.click(screen.getByRole("link", { name: "Config" }));
    expect(onSelectedPathChange).toHaveBeenCalledWith("src/config.ts");
  });

  it("ignores a stale preview error after switching workspaces", async () => {
    let rejectOldPreview: (reason: Error) => void = () => undefined;
    const oldPreview = new Promise<{ content: string; truncated: boolean }>((_, reject) => {
      rejectOldPreview = reject;
    });
    const readFile = vi.fn().mockImplementation((workspaceId: string) => workspaceId === "workspace-1"
      ? oldPreview
      : Promise.resolve({ content: "", truncated: false }));
    const commonProps = {
      onSelectedPathChange: vi.fn(),
      onClose: vi.fn(),
      panelWidth: 360,
      onPanelWidthChange: vi.fn(),
      listFiles: vi.fn().mockResolvedValue(["README.md"]),
      readFile,
      loadGitStatus: vi.fn().mockResolvedValue({ files: [] }),
    };
    const view = render(
      <FileManager {...commonProps} workspaceId="workspace-1" selectedPath="old-file.md" />,
    );

    expect(readFile).toHaveBeenCalledWith("workspace-1", "old-file.md");
    view.rerender(
      <FileManager {...commonProps} workspaceId="workspace-2" selectedPath={null} />,
    );
    await act(async () => {
      rejectOldPreview(new Error("Failed to open file: No such file or directory (os error 2)"));
      await oldPreview.catch(() => undefined);
    });

    expect(screen.queryByText(/No such file or directory/)).toBeNull();
    expect(screen.getByText("Select a file to preview")).toBeTruthy();
  });

  it("ignores a stale file listing after the active Thread changes", async () => {
    let resolveOldFiles: (files: string[]) => void = () => undefined;
    const oldFiles = new Promise<string[]>((resolve) => {
      resolveOldFiles = resolve;
    });
    const commonProps = {
      workspaceId: "workspace-1",
      selectedPath: null,
      onSelectedPathChange: vi.fn(),
      onClose: vi.fn(),
      panelWidth: 360,
      onPanelWidthChange: vi.fn(),
      readFile: vi.fn().mockResolvedValue({ content: "", truncated: false }),
    };
    const view = render(
      <FileManager
        {...commonProps}
        listFiles={vi.fn().mockReturnValue(oldFiles)}
        loadGitStatus={vi.fn().mockResolvedValue({ files: [] })}
      />,
    );

    view.rerender(
      <FileManager
        {...commonProps}
        listFiles={vi.fn().mockResolvedValue(["new-thread.txt"])}
        loadGitStatus={vi.fn().mockResolvedValue({ files: [] })}
      />,
    );
    expect(await screen.findByText("new-thread.txt")).toBeTruthy();

    await act(async () => {
      resolveOldFiles(["old-thread.txt"]);
      await oldFiles;
    });
    expect(screen.queryByText("old-thread.txt")).toBeNull();
    expect(screen.getByText("new-thread.txt")).toBeTruthy();
  });

  it("collapses the file list independently from the panel", async () => {
    render(
      <FileManager
        workspaceId="workspace-1"
        selectedPath={null}
        onSelectedPathChange={vi.fn()}
        onClose={vi.fn()}
        panelWidth={360}
        onPanelWidthChange={vi.fn()}
        listFiles={vi.fn().mockResolvedValue(["README.md"])}
        readFile={vi.fn().mockResolvedValue({ content: "", truncated: false })}
        loadGitStatus={vi.fn().mockResolvedValue({ files: [] })}
      />,
    );

    expect(await screen.findByText("README.md")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /Workspace/ }));
    expect(screen.queryByText("README.md")).toBeNull();
    expect(screen.getByText("Select a file to preview")).toBeTruthy();
  });

  it("supports keyboard resizing and uses material file icons", async () => {
    const onPanelWidthChange = vi.fn();
    const view = render(
      <FileManager
        workspaceId="workspace-1"
        selectedPath={null}
        onSelectedPathChange={vi.fn()}
        onClose={vi.fn()}
        panelWidth={360}
        onPanelWidthChange={onPanelWidthChange}
        listFiles={vi.fn().mockResolvedValue(["src/config.ts", "README.md"])}
        readFile={vi.fn().mockResolvedValue({ content: "", truncated: false })}
        loadGitStatus={vi.fn().mockResolvedValue({ files: [] })}
      />,
    );

    expect(await screen.findByText("README.md")).toBeTruthy();
    fireEvent.keyDown(screen.getByRole("separator", { name: "Resize file manager" }), { key: "ArrowLeft" });
    expect(onPanelWidthChange).toHaveBeenCalledWith(376);
    expect(view.container.querySelector('img[src*="material-icons"]')).toBeTruthy();
  });

  it("updates the CSS width during pointer dragging and commits React state only on release", async () => {
    const onPanelWidthChange = vi.fn();
    const view = render(
      <div className="web-app-shell">
        <FileManager
          workspaceId="workspace-1"
          selectedPath={null}
          onSelectedPathChange={vi.fn()}
          onClose={vi.fn()}
          panelWidth={360}
          onPanelWidthChange={onPanelWidthChange}
          listFiles={vi.fn().mockResolvedValue([])}
          readFile={vi.fn().mockResolvedValue({ content: "", truncated: false })}
          loadGitStatus={vi.fn().mockResolvedValue({ files: [] })}
        />
      </div>,
    );

    await screen.findByText("No Workspace files yet");
    const separator = screen.getByRole("separator", { name: "Resize file manager" });

    fireEvent.pointerDown(separator, { pointerId: 1, clientX: 500 });
    fireEvent.pointerMove(window, { pointerId: 1, clientX: 450 });

    const shell = view.container.querySelector<HTMLElement>(".web-app-shell");
    expect(shell?.style.getPropertyValue("--web-file-panel-width")).toBe("410px");
    expect(shell?.classList.contains("web-files-resizing")).toBe(true);
    expect(onPanelWidthChange).not.toHaveBeenCalled();

    fireEvent.pointerUp(window, { pointerId: 1, clientX: 450 });
    expect(onPanelWidthChange).toHaveBeenCalledTimes(1);
    expect(onPanelWidthChange).toHaveBeenCalledWith(410);
    expect(shell?.classList.contains("web-files-resizing")).toBe(false);
  });
});
