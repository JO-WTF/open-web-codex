type Props = {
  command: string;
};

export default function ApprovalCard({ command }: Props) {
  return (
    <div className="web-approval-card">
      <div className="web-approval-header">
        <span className="web-approval-icon">&#9888;</span>
        <span className="web-approval-label">Approval required</span>
      </div>
      <pre className="web-approval-command"><code>{command}</code></pre>
      <div className="web-approval-hint">Approve or deny this command in the Codex desktop app</div>
    </div>
  );
}
