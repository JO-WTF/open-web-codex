type ThreadInfo = {
  id: string;
  label: string;
  updatedAt: number;
  turnCount?: number;
};

type Props = {
  threads: ThreadInfo[];
  activeId: string | null;
  onSelect: (id: string) => void;
  onNew: () => void;
  busy: boolean;
};

function relativeTime(ts: number): string {
  const seconds = Math.floor((Date.now() - ts) / 1000);
  if (seconds < 60) return "now";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}

export default function Threads({ threads, activeId, onSelect, onNew, busy }: Props) {
  return (
    <>
      <div className="web-section-heading">Threads</div>
      {threads.length === 0 && (
        <div style={{ padding: "8px 12px", fontSize: 12, color: "var(--text-muted)" }}>
          No threads yet. Start a new one.
        </div>
      )}
      {threads.map((t) => (
        <div
          key={t.id}
          className={`web-thread-item${t.id === activeId ? " web-thread-item-active" : ""}`}
          onClick={() => onSelect(t.id)}
        >
          <div className="web-thread-title">{t.label}</div>
          <div className="web-thread-meta">
            <span>{relativeTime(t.updatedAt)}</span>
            {t.turnCount != null && <span>{t.turnCount} turns</span>}
          </div>
        </div>
      ))}
      <button className="web-thread-new-btn" onClick={onNew} disabled={busy}>
        + New Thread
      </button>
    </>
  );
}
