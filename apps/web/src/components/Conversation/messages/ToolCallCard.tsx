type Props = {
  toolType: string;
  title: string;
  status: string;
  filePath?: string;
};

const STATUS_CLASS: Record<string, string> = {
  running: "web-tool-status-running",
  completed: "web-tool-status-done",
  done: "web-tool-status-done",
  error: "web-tool-status-error",
  failed: "web-tool-status-error",
};

export default function ToolCallCard({ toolType, title, status, filePath }: Props) {
  const sc = STATUS_CLASS[status.toLowerCase()] ?? "web-tool-status-running";
  return (
    <div className="web-tool-card">
      <div className="web-tool-card-header">
        <span className="web-tool-name">{toolType}</span>
        <span>{title}</span>
        <span className={`web-tool-status ${sc}`}>
          <span className="web-tool-status-dot" />
          {status}
        </span>
      </div>
      {filePath && <div className="web-tool-file">{filePath}</div>}
    </div>
  );
}
