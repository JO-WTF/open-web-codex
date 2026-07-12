import type { WorkspaceInfo } from "../../types";

type Props = {
  workspaces: WorkspaceInfo[];
  activeId: string | null;
  onSelect: (id: string) => void;
  onAdd: (path: string) => void;
  busy: boolean;
};

export default function Workspaces({ workspaces, activeId, onSelect, onAdd, busy }: Props) {
  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") {
      const input = e.currentTarget;
      const path = input.value.trim();
      if (path) {
        onAdd(path);
        input.value = "";
      }
    }
  };

  return (
    <>
      <div className="web-section-heading">
        Workspaces
        <span className="web-section-count">{workspaces.length}</span>
      </div>
      {workspaces.map((ws) => (
        <div
          key={ws.id}
          className={`web-ws-item${ws.id === activeId ? " web-ws-item-active" : ""}`}
          onClick={() => onSelect(ws.id)}
        >
          <span
            className={`web-ws-indicator ${ws.connected ? "web-ws-indicator-connected" : "web-ws-indicator-disconnected"}`}
          />
          <span className="web-ws-name">{ws.name}</span>
        </div>
      ))}
      <div className="web-add-ws">
        <input
          placeholder="/path/to/workspace"
          onKeyDown={handleKeyDown}
          disabled={busy}
        />
      </div>
    </>
  );
}
