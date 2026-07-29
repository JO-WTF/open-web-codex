import { useEffect, useRef, useState } from "react";
import AlertCircle from "lucide-react/dist/esm/icons/alert-circle";
import CheckCircle2 from "lucide-react/dist/esm/icons/check-circle-2";
import Copy from "lucide-react/dist/esm/icons/copy";
import Database from "lucide-react/dist/esm/icons/database";
import Upload from "lucide-react/dist/esm/icons/upload";
import X from "lucide-react/dist/esm/icons/x";
import type {
  PublishWorkspaceDatasetRequest,
  WorkspaceDatasetReleaseSummary,
} from "../../../../browser/types";
import { platformClient } from "../../../../browser/session";
import { ModalShell } from "../../design-system/components/modal/ModalShell";

type DatasetReleaseDialogProps = {
  workspaceId: string;
  onClose: () => void;
  onPublished: () => void;
};

type SelectedFile = {
  fieldId: string;
  file: File;
  logicalName: string;
  role: string;
  mediaType: string;
};

type ImportedManifest = {
  datasetId?: string;
  version?: string;
  roles: Map<string, string>;
};

const MAX_LOCAL_MANIFEST_BYTES = 1024 * 1024;

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : "The operation failed.";
}

function displayNameFromId(value: string) {
  return value
    .split("-")
    .filter(Boolean)
    .map((part) => part[0]?.toUpperCase() + part.slice(1))
    .join(" ");
}

function mediaTypeFor(file: File) {
  if (file.type) {
    return file.type;
  }
  const name = file.name.toLowerCase();
  if (name.endsWith(".csv.gz") || name.endsWith(".gz")) {
    return "application/gzip";
  }
  if (name.endsWith(".csv")) {
    return "text/csv";
  }
  if (name.endsWith(".geojson")) {
    return "application/geo+json";
  }
  if (name.endsWith(".json")) {
    return "application/json";
  }
  return "application/octet-stream";
}

function readTextFile(file: File): Promise<string> {
  if (typeof file.text === "function") {
    return file.text();
  }
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () =>
      typeof reader.result === "string"
        ? resolve(reader.result)
        : reject(new Error("Manifest was not read as text."));
    reader.onerror = () => reject(reader.error ?? new Error("Manifest could not be read."));
    reader.readAsText(file);
  });
}

async function importedManifest(files: File[]): Promise<ImportedManifest | null> {
  const manifest = files.find(
    (file) =>
      file.name === "dataset-manifest.json" &&
      file.size > 0 &&
      file.size <= MAX_LOCAL_MANIFEST_BYTES,
  );
  if (!manifest) {
    return null;
  }
  try {
    const value = JSON.parse(await readTextFile(manifest)) as {
      schema_version?: unknown;
      dataset_id?: unknown;
      version?: unknown;
      files?: unknown;
    };
    if (
      value.schema_version !== "workspace_dataset_release.v1" ||
      !Array.isArray(value.files)
    ) {
      return null;
    }
    const roles = new Map<string, string>();
    for (const entry of value.files) {
      if (!entry || typeof entry !== "object") {
        continue;
      }
      const record = entry as Record<string, unknown>;
      if (typeof record.path === "string" && typeof record.role === "string") {
        roles.set(record.path, record.role);
      }
    }
    return {
      datasetId: typeof value.dataset_id === "string" ? value.dataset_id : undefined,
      version: typeof value.version === "string" ? value.version : undefined,
      roles,
    };
  } catch {
    return null;
  }
}

function formatBytes(value: number) {
  if (value < 1024) {
    return `${value} B`;
  }
  if (value < 1024 * 1024) {
    return `${(value / 1024).toFixed(1)} KiB`;
  }
  return `${(value / 1024 / 1024).toFixed(1)} MiB`;
}

export function DatasetReleaseDialog({
  workspaceId,
  onClose,
  onPublished,
}: DatasetReleaseDialogProps) {
  const inputRef = useRef<HTMLInputElement | null>(null);
  const idempotencyKeyRef = useRef<string | null>(null);
  const [datasetId, setDatasetId] = useState("");
  const [version, setVersion] = useState("1.0.0");
  const [displayName, setDisplayName] = useState("");
  const [description, setDescription] = useState("");
  const [selectedFiles, setSelectedFiles] = useState<SelectedFile[]>([]);
  const [releases, setReleases] = useState<WorkspaceDatasetReleaseSummary[]>([]);
  const [loadingReleases, setLoadingReleases] = useState(true);
  const [publishing, setPublishing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [published, setPublished] =
    useState<WorkspaceDatasetReleaseSummary | null>(null);
  const [copiedReleaseId, setCopiedReleaseId] = useState<string | null>(null);

  const invalidateRequest = () => {
    idempotencyKeyRef.current = null;
    setPublished(null);
  };

  const refreshReleases = async () => {
    setLoadingReleases(true);
    try {
      setReleases(await platformClient.listWorkspaceDatasetReleases(workspaceId));
    } catch (loadError) {
      setError(errorMessage(loadError));
    } finally {
      setLoadingReleases(false);
    }
  };

  useEffect(() => {
    void refreshReleases();
  }, [workspaceId]);

  const handleFileSelection = async (fileList: FileList | null) => {
    invalidateRequest();
    setError(null);
    const files = fileList ? Array.from(fileList) : [];
    const duplicateNames = files
      .map((file) => file.name)
      .filter((name, index, names) => names.indexOf(name) !== index);
    if (duplicateNames.length > 0) {
      setSelectedFiles([]);
      setError("File names must be unique inside one Dataset Release.");
      return;
    }
    const imported = await importedManifest(files);
    if (imported?.datasetId) {
      setDatasetId(imported.datasetId);
      setDisplayName((current) => current || displayNameFromId(imported.datasetId ?? ""));
    }
    if (imported?.version) {
      setVersion(imported.version);
    }
    setSelectedFiles(
      files.map((file, index) => ({
        fieldId: `file-${index}`,
        file,
        logicalName: file.name,
        role:
          imported?.roles.get(file.name) ??
          (file.name === "dataset-manifest.json" ? "dataset_manifest" : "source_data"),
        mediaType: mediaTypeFor(file),
      })),
    );
  };

  const publish = async () => {
    if (selectedFiles.length === 0) {
      setError("Choose at least one data file.");
      return;
    }
    setPublishing(true);
    setError(null);
    try {
      const idempotencyKey =
        idempotencyKeyRef.current ??
        globalThis.crypto?.randomUUID?.() ??
        `dataset-${Date.now()}-${Math.random().toString(16).slice(2)}`;
      idempotencyKeyRef.current = idempotencyKey;
      const request: PublishWorkspaceDatasetRequest = {
        idempotency_key: idempotencyKey,
        dataset_id: datasetId.trim(),
        version: version.trim(),
        display_name: displayName.trim(),
        description: description.trim(),
        files: selectedFiles.map((entry) => ({
          field_id: entry.fieldId,
          logical_name: entry.logicalName,
          role: entry.role.trim(),
          media_type: entry.mediaType,
        })),
      };
      const files = new Map(selectedFiles.map((entry) => [entry.fieldId, entry.file]));
      const release = await platformClient.publishWorkspaceDatasetRelease(
        workspaceId,
        request,
        files,
      );
      setPublished(release);
      await refreshReleases();
      onPublished();
    } catch (publishError) {
      setError(errorMessage(publishError));
    } finally {
      setPublishing(false);
    }
  };

  const copyReleaseIdentity = async (release: WorkspaceDatasetReleaseSummary) => {
    setError(null);
    if (!navigator.clipboard?.writeText) {
      setError("Clipboard access is unavailable in this browser.");
      return;
    }
    try {
      await navigator.clipboard.writeText(
        JSON.stringify(
          {
            workspace_id: release.workspace_id,
            release_id: release.id,
            dataset_id: release.dataset_id,
            version: release.version,
            content_sha256: release.content_sha256,
          },
          null,
          2,
        ),
      );
      setCopiedReleaseId(release.id);
    } catch (copyError) {
      setError(errorMessage(copyError));
    }
  };

  return (
    <ModalShell
      className="dataset-release-modal"
      cardClassName="dataset-release-card"
      ariaLabelledBy="dataset-release-title"
      onBackdropClick={() => {
        if (!publishing) {
          onClose();
        }
      }}
    >
      <header className="dataset-release-header">
        <div>
          <div className="dataset-release-kicker">
            <Database size={14} aria-hidden />
            Workspace data
          </div>
          <h2 id="dataset-release-title">Publish a data release</h2>
          <p>
            A release keeps the selected files, their roles, and hashes together.
            Agents can reference one exact version instead of searching folders.
          </p>
        </div>
        <button
          type="button"
          className="ghost icon-button"
          aria-label="Close data release dialog"
          onClick={onClose}
          disabled={publishing}
        >
          <X size={16} aria-hidden />
        </button>
      </header>

      <div className="dataset-release-layout">
        <section className="dataset-release-form" aria-label="New data release">
          <div className="dataset-release-fields">
            <label className="ds-modal-label">
              Dataset ID
              <input
                className="ds-modal-input"
                value={datasetId}
                placeholder="indonesia-warehouse-network"
                onChange={(event) => {
                  invalidateRequest();
                  setDatasetId(event.target.value);
                }}
              />
            </label>
            <label className="ds-modal-label">
              Version
              <input
                className="ds-modal-input"
                value={version}
                placeholder="1.0.0"
                onChange={(event) => {
                  invalidateRequest();
                  setVersion(event.target.value);
                }}
              />
            </label>
          </div>
          <label className="ds-modal-label">
            Name
            <input
              className="ds-modal-input"
              value={displayName}
              placeholder="Indonesia warehouse network"
              onChange={(event) => {
                invalidateRequest();
                setDisplayName(event.target.value);
              }}
            />
          </label>
          <label className="ds-modal-label">
            What this data is for
            <textarea
              className="ds-modal-textarea dataset-release-description"
              value={description}
              placeholder="Inputs for the current-network coverage tutorial."
              onChange={(event) => {
                invalidateRequest();
                setDescription(event.target.value);
              }}
            />
          </label>

          <input
            ref={inputRef}
            className="dataset-release-file-input"
            type="file"
            multiple
            onChange={(event) => {
              void handleFileSelection(event.target.files);
            }}
          />
          <button
            type="button"
            className="dataset-release-dropzone"
            onClick={() => inputRef.current?.click()}
            disabled={publishing}
          >
            <Upload size={19} aria-hidden />
            <span>
              {selectedFiles.length > 0
                ? `${selectedFiles.length} files selected`
                : "Choose release files"}
            </span>
            <small>
              Up to 32 files, 32 MiB each, 64 MiB total. A compatible
              dataset-manifest.json can prefill file roles.
            </small>
          </button>

          {selectedFiles.length > 0 ? (
            <div className="dataset-release-files">
              {selectedFiles.map((entry, index) => (
                <div className="dataset-release-file" key={entry.fieldId}>
                  <div className="dataset-release-file-name">
                    <strong>{entry.logicalName}</strong>
                    <span>{formatBytes(entry.file.size)}</span>
                  </div>
                  <label className="ds-modal-label">
                    File role
                    <input
                      className="ds-modal-input"
                      value={entry.role}
                      onChange={(event) => {
                        invalidateRequest();
                        setSelectedFiles((current) =>
                          current.map((file, fileIndex) =>
                            fileIndex === index
                              ? { ...file, role: event.target.value }
                              : file,
                          ),
                        );
                      }}
                    />
                  </label>
                </div>
              ))}
            </div>
          ) : null}

          {error ? (
            <div className="ds-modal-error dataset-release-error" role="alert">
              <AlertCircle size={14} aria-hidden />
              {error}
            </div>
          ) : null}
          {published?.state === "published" ? (
            <div className="dataset-release-success" role="status">
              <CheckCircle2 size={15} aria-hidden />
              Published {published.dataset_id}@{published.version}
            </div>
          ) : null}
          <div className="ds-modal-actions">
            <button
              type="button"
              className="ghost ds-modal-button"
              onClick={onClose}
              disabled={publishing}
            >
              Close
            </button>
            <button
              type="button"
              className="primary ds-modal-button dataset-release-publish"
              onClick={() => void publish()}
              disabled={
                publishing ||
                !datasetId.trim() ||
                !version.trim() ||
                !displayName.trim() ||
                !description.trim() ||
                selectedFiles.length === 0
              }
            >
              {publishing ? "Publishing…" : "Publish release"}
            </button>
          </div>
        </section>

        <aside className="dataset-release-history" aria-label="Published data releases">
          <div className="dataset-release-history-title">
            <span>Available releases</span>
            <span>{releases.length}</span>
          </div>
          {loadingReleases ? (
            <p className="dataset-release-muted">Loading releases…</p>
          ) : releases.length === 0 ? (
            <p className="dataset-release-muted">
              No data has been published in this Workspace yet.
            </p>
          ) : (
            <div className="dataset-release-list">
              {releases.map((release) => (
                <article
                  className={`dataset-release-summary is-${release.state}`}
                  key={release.id}
                >
                  <div>
                    <strong>{release.display_name}</strong>
                    <span>
                      {release.dataset_id}@{release.version}
                    </span>
                  </div>
                  <div className="dataset-release-summary-meta">
                    <span>{release.files.length} files</span>
                    <span>{release.state}</span>
                  </div>
                  {release.state === "published" ? (
                    <div className="dataset-release-identity">
                      <code title={release.content_sha256}>
                        SHA-256 {release.content_sha256.slice(0, 12)}…
                      </code>
                      <button
                        type="button"
                        className="ghost dataset-release-copy"
                        onClick={() => void copyReleaseIdentity(release)}
                        aria-label={`Copy exact identity for ${release.display_name}`}
                      >
                        {copiedReleaseId === release.id ? (
                          <CheckCircle2 size={12} aria-hidden />
                        ) : (
                          <Copy size={12} aria-hidden />
                        )}
                        {copiedReleaseId === release.id ? "Copied" : "Copy identity"}
                      </button>
                    </div>
                  ) : null}
                  {release.failure_code ? (
                    <small>Failure: {release.failure_code}</small>
                  ) : null}
                </article>
              ))}
            </div>
          )}
        </aside>
      </div>
    </ModalShell>
  );
}
