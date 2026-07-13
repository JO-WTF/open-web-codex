import CircleCheck from "lucide-react/dist/esm/icons/circle-check";

type Props = {
  command: string;
  workspaceId?: string;
  requestId?: number | string;
  status?: "pending" | "accepted" | "declined" | "resolved";
  onResolve?: (workspaceId: string, requestId: number | string, decision: "accept" | "decline") => void;
};

export default function ApprovalCard({ command, workspaceId, requestId, status = "pending", onResolve }: Props) {
  const shortCmd = command.replace(/^\/bin\/zsh -lc '/, "").replace(/'$/, "").slice(0, 120);
  const pending = status === "pending";
  const resolvedLabel = status === "accepted"
    ? "Accepted"
    : status === "declined"
      ? "Denied"
      : "Resolved";

  const handleAccept = () => {
    if (workspaceId && requestId !== undefined && onResolve) {
      onResolve(workspaceId, requestId, "accept");
    }
  };

  const handleDeny = () => {
    if (workspaceId && requestId !== undefined && onResolve) {
      onResolve(workspaceId, requestId, "decline");
    }
  };

  return (
    <div className="web-approval-card">
      <div className="web-approval-header">
        {pending ? (
          <span className="web-approval-icon">&#9888;</span>
        ) : (
          <CircleCheck className="web-approval-resolved-icon" size={14} aria-hidden="true" />
        )}
        <span className="web-approval-label">{pending ? "Approval required" : "Approval resolved"}</span>
      </div>
      <pre className="web-approval-command"><code>{shortCmd}</code></pre>
      {!pending ? (
        <div className={`web-approval-resolution is-${status}`}>{resolvedLabel}</div>
      ) : workspaceId && requestId !== undefined ? (
        <div className="web-approval-actions">
          <button className="web-approval-accept" onClick={handleAccept}>
            Accept
          </button>
          <button className="web-approval-deny" onClick={handleDeny}>
            Deny
          </button>
        </div>
      ) : (
        <div className="web-approval-hint">
          Connect a workspace and start a thread to approve commands here
        </div>
      )}
    </div>
  );
}
