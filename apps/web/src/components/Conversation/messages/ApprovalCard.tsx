type Props = {
  command: string;
  workspaceId?: string;
  threadId?: string;
  onResolve?: (workspaceId: string, threadId: string, decision: string) => void;
};

export default function ApprovalCard({ command, workspaceId, threadId, onResolve }: Props) {
  const shortCmd = command.replace(/^\/bin\/zsh -lc '/, "").replace(/'$/, "").slice(0, 120);

  const handleAccept = () => {
    if (workspaceId && threadId && onResolve) {
      onResolve(workspaceId, threadId, "accept");
    }
  };

  const handleDeny = () => {
    if (workspaceId && threadId && onResolve) {
      onResolve(workspaceId, threadId, "cancel");
    }
  };

  return (
    <div className="web-approval-card">
      <div className="web-approval-header">
        <span className="web-approval-icon">&#9888;</span>
        <span className="web-approval-label">Approval required</span>
      </div>
      <pre className="web-approval-command"><code>{shortCmd}</code></pre>
      {workspaceId && threadId ? (
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
