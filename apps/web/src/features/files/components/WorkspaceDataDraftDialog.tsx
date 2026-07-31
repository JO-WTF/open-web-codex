import { useRef, useState } from "react";
import AlertCircle from "lucide-react/dist/esm/icons/alert-circle";
import CheckCircle2 from "lucide-react/dist/esm/icons/check-circle-2";
import Database from "lucide-react/dist/esm/icons/database";
import Upload from "lucide-react/dist/esm/icons/upload";
import X from "lucide-react/dist/esm/icons/x";
import { platformClient } from "../../../../browser/session";
import type { WorkspaceDataDraftSummary } from "../../../../browser/types";
import { ModalShell } from "../../design-system/components/modal/ModalShell";

type WorkspaceDataDraftDialogProps = {
  workspaceId: string;
  onClose: () => void;
  onCreated: (draft: WorkspaceDataDraftSummary) => void;
};

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : "The upload failed.";
}

export function WorkspaceDataDraftDialog({
  workspaceId,
  onClose,
  onCreated,
}: WorkspaceDataDraftDialogProps) {
  const inputRef = useRef<HTMLInputElement | null>(null);
  const idempotencyKeyRef = useRef<string | null>(null);
  const [files, setFiles] = useState<File[]>([]);
  const [uploading, setUploading] = useState(false);
  const [draft, setDraft] = useState<WorkspaceDataDraftSummary | null>(null);
  const [error, setError] = useState<string | null>(null);

  const upload = async () => {
    if (files.length === 0) {
      setError("Choose at least one .xlsx, .csv or .json file.");
      return;
    }
    setUploading(true);
    setError(null);
    try {
      const key =
        idempotencyKeyRef.current ??
        globalThis.crypto?.randomUUID?.() ??
        `data-draft-${Date.now()}-${Math.random().toString(16).slice(2)}`;
      idempotencyKeyRef.current = key;
      const nextDraft = await platformClient.createDataDraft(workspaceId, files, key);
      setDraft(nextDraft);
      onCreated(nextDraft);
    } catch (uploadError) {
      setError(errorMessage(uploadError));
    } finally {
      setUploading(false);
    }
  };

  return (
    <ModalShell
      className="dataset-release-modal"
      cardClassName="dataset-release-card"
      ariaLabelledBy="workspace-data-draft-title"
      onBackdropClick={() => {
        if (!uploading) onClose();
      }}
    >
      <header className="dataset-release-header">
        <div>
          <div className="dataset-release-kicker">
            <Database size={14} aria-hidden />
            Workspace data
          </div>
          <h2 id="workspace-data-draft-title">Add planning data</h2>
          <p>
            Upload the files you have. The Supervisor will profile them, suggest field
            mappings, and ask for confirmation before analysis.
          </p>
        </div>
        <button type="button" className="ghost icon-button" aria-label="Close data upload" onClick={onClose} disabled={uploading}>
          <X size={16} aria-hidden />
        </button>
      </header>
      <section className="dataset-release-form" aria-label="Workspace data upload">
        <input
          ref={inputRef}
          className="dataset-release-file-input"
          type="file"
          multiple
          accept=".xlsx,.csv,.json"
          onChange={(event) => {
            idempotencyKeyRef.current = null;
            setDraft(null);
            setError(null);
            setFiles(event.target.files ? Array.from(event.target.files) : []);
          }}
        />
        <button type="button" className="dataset-release-dropzone" onClick={() => inputRef.current?.click()} disabled={uploading}>
          <Upload size={19} aria-hidden />
          <span>{files.length > 0 ? `${files.length} files selected` : "Choose planning files"}</span>
          <small>Supported: .xlsx, .csv, .json. Up to 20 files, 100 MiB each, 250 MiB total.</small>
        </button>
        {files.length > 0 ? (
          <div className="dataset-release-files">
            {files.map((file) => (
              <div className="dataset-release-file" key={`${file.name}:${file.size}:${file.lastModified}`}>
                <div className="dataset-release-file-name"><strong>{file.name}</strong><span>{Math.ceil(file.size / 1024)} KiB</span></div>
              </div>
            ))}
          </div>
        ) : null}
        {error ? <div className="ds-modal-error dataset-release-error" role="alert"><AlertCircle size={14} aria-hidden />{error}</div> : null}
        {draft ? <div className="dataset-release-success" role="status"><CheckCircle2 size={15} aria-hidden />Data draft received (revision {draft.revision}).</div> : null}
        <div className="ds-modal-actions">
          <button type="button" className="ghost ds-modal-button" onClick={onClose} disabled={uploading}>Close</button>
          <button type="button" className="primary ds-modal-button dataset-release-publish" onClick={() => void upload()} disabled={uploading || files.length === 0}>
            {uploading ? "Uploading…" : "Upload data"}
          </button>
        </div>
      </section>
    </ModalShell>
  );
}
