import { useCallback, useEffect, useRef, useState } from "react";
import {
  applySavedMapsConfiguration,
  useMapsConfiguration,
} from "../../../services/mapsConfiguration";
import MapsConfigurationModal from "./MapsConfigurationModal";
import ApprovalStatusIcon from "./ApprovalStatusIcon";
import {
  isApprovalOutcome,
  type ApprovalStatus,
} from "../../../utils/approvalStatus";

type Props = {
  command: string;
  workspaceId?: string;
  requestId?: number | string;
  status?: ApprovalStatus;
  mode?: string;
  credentialKind?: "maps";
  submitting?: boolean;
  onResolve?: (workspaceId: string, requestId: number | string, decision: "accept" | "decline") => void;
};

export default function ApprovalCard({
  command,
  workspaceId,
  requestId,
  status = "pending",
  mode,
  credentialKind,
  submitting = false,
  onResolve,
}: Props) {
  const [configurationOpen, setConfigurationOpen] = useState(false);
  const [configurationError, setConfigurationError] = useState("");
  const [usingSavedConfiguration, setUsingSavedConfiguration] = useState(false);
  const attemptedSavedApprovalId = useRef<string | null>(null);
  const mapsConfiguration = useMapsConfiguration();
  const shortCmd = command.replace(/^\/bin\/zsh -lc '/, "").replace(/'$/, "").slice(0, 120);
  const pending = status === "pending";
  const credentialRequest = mode === "url";
  const mapsCredentialRequest = credentialRequest && credentialKind === "maps";
  const approvalId = typeof requestId === "string" ? requestId : null;
  const resolvedLabel = status === "accepted"
    ? (mapsCredentialRequest ? "Configured" : credentialRequest ? "Configuration opened" : "Accepted")
    : status === "declined"
      ? "Denied"
      : status === "answered"
        ? "Other response provided"
        : status === "cancelled"
          ? "Cancelled"
          : "Resolved";

  const handleAccept = useCallback(() => {
    if (!submitting && workspaceId && requestId !== undefined && onResolve) {
      onResolve(workspaceId, requestId, "accept");
    }
  }, [onResolve, requestId, submitting, workspaceId]);

  const handleDeny = () => {
    if (!submitting && workspaceId && requestId !== undefined && onResolve) {
      onResolve(workspaceId, requestId, "decline");
    }
  };

  useEffect(() => {
    if (
      !pending
      || !mapsCredentialRequest
      || !approvalId
      || mapsConfiguration.loading
      || !mapsConfiguration.configured
      || attemptedSavedApprovalId.current === approvalId
    ) {
      return;
    }
    attemptedSavedApprovalId.current = approvalId;
    setUsingSavedConfiguration(true);
    setConfigurationError("");
    void applySavedMapsConfiguration(approvalId)
      .then(handleAccept)
      .catch((error: unknown) => {
        attemptedSavedApprovalId.current = null;
        setConfigurationError(
          error instanceof Error
            ? error.message
            : "使用已保存的地图配置失败",
        );
      })
      .finally(() => setUsingSavedConfiguration(false));
  }, [
    handleAccept,
    mapsConfiguration.configured,
    mapsConfiguration.loading,
    mapsCredentialRequest,
    pending,
    approvalId,
  ]);

  const openConfiguration = () => {
    setConfigurationError("");
    setConfigurationOpen(true);
  };

  const configuredProvider = () => {
    setConfigurationOpen(false);
    attemptedSavedApprovalId.current = approvalId;
    handleAccept();
  };

  return (
    <>
      <div className="web-approval-card">
      <div className="web-approval-header">
        {pending ? (
          <span className="web-approval-icon">&#9888;</span>
        ) : isApprovalOutcome(status) ? (
          <ApprovalStatusIcon status={status} detail={shortCmd} />
        ) : (
          <span className="web-approval-resolved-icon" aria-hidden="true">&#10003;</span>
        )}
        <span className="web-approval-label">
          {pending
            ? mapsCredentialRequest
              ? "Map provider and API key required"
              : credentialRequest
                ? "MCP credential configuration required"
              : "Approval required"
            : "Approval resolved"}
        </span>
      </div>
      <pre className="web-approval-command"><code>{shortCmd}</code></pre>
      {!pending ? (
        <div className={`web-approval-resolution is-${status}`}>{resolvedLabel}</div>
      ) : workspaceId && requestId !== undefined ? (
        <div className="web-approval-actions">
          {mapsCredentialRequest
            ? usingSavedConfiguration || mapsConfiguration.loading
              ? (
                  <span className="web-approval-hint">
                    正在使用已保存的
                    {mapsConfiguration.provider === "google" ? " Google Maps" : " Mapbox"}
                    {" "}配置…
                  </span>
                )
              : (
                <>
                  {configurationError ? (
                    <span className="web-approval-hint">{configurationError}</span>
                  ) : null}
                  <button
                    className="web-approval-accept"
                    type="button"
                    onClick={openConfiguration}
                    disabled={!approvalId || !mapsConfiguration.canConfigure}
                  >
                    配置 Key
                  </button>
                </>
                )
            : credentialRequest
            ? (
                <span className="web-approval-hint">
                  This MCP credential request is not supported by the secure browser configuration flow.
                </span>
              )
            : (
                <button
                  className="web-approval-accept"
                  onClick={handleAccept}
                  disabled={submitting}
                >
                  {submitting ? "Submitting…" : "Accept"}
                </button>
              )}
          <button className="web-approval-deny" onClick={handleDeny} disabled={submitting}>
            {credentialRequest ? "Cancel" : "Deny"}
          </button>
        </div>
      ) : (
        <div className="web-approval-hint">
          Connect a workspace and start a thread to approve commands here
        </div>
      )}
      </div>
      {configurationOpen && mapsCredentialRequest && approvalId ? (
        <MapsConfigurationModal
          initialProvider={mapsConfiguration.provider ?? "mapbox"}
          approvalId={approvalId}
          onClose={() => setConfigurationOpen(false)}
          onSaved={configuredProvider}
        />
      ) : null}
    </>
  );
}
