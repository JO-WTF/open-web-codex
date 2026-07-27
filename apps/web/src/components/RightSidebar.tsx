import { useEffect, useRef, type ReactNode } from "react";
import Bot from "lucide-react/dist/esm/icons/bot";
import Files from "lucide-react/dist/esm/icons/files";
import X from "lucide-react/dist/esm/icons/x";

export type RightSidebarTab = "agents" | "files";

type Props = {
  activeTab: RightSidebarTab;
  agentUnread?: boolean;
  width: number;
  onWidthChange: (width: number) => void;
  onTabChange: (tab: RightSidebarTab) => void;
  onClose: () => void;
  agentPanel: ReactNode;
  filePanel: ReactNode;
};

type ResizeSession = {
  x: number;
  width: number;
  currentWidth: number;
  shell: HTMLElement;
  sidebar: HTMLElement;
  cleanup: (commit: boolean) => void;
};

const MIN_PANEL_WIDTH = 300;
const MAX_PANEL_WIDTH = 720;

export default function RightSidebar({
  activeTab,
  agentUnread = false,
  width,
  onWidthChange,
  onTabChange,
  onClose,
  agentPanel,
  filePanel,
}: Props) {
  const resizeSession = useRef<ResizeSession | null>(null);
  const clampWidth = (value: number) => Math.min(MAX_PANEL_WIDTH, Math.max(MIN_PANEL_WIDTH, value));

  useEffect(() => () => {
    resizeSession.current?.cleanup(false);
  }, []);

  return (
    <aside
      className="web-right-sidebar"
      aria-label="Thread details"
      onKeyDown={(event) => {
        if (event.key === "Escape") onClose();
      }}
    >
      <div
        className="web-right-sidebar-resizer"
        role="separator"
        aria-label="Resize details panel"
        aria-orientation="vertical"
        aria-valuemin={MIN_PANEL_WIDTH}
        aria-valuemax={MAX_PANEL_WIDTH}
        aria-valuenow={width}
        tabIndex={0}
        onPointerDown={(event) => {
          const sidebar = event.currentTarget.closest<HTMLElement>(".web-right-sidebar");
          const shell = event.currentTarget.closest<HTMLElement>(".web-app-shell");
          if (!sidebar || !shell) return;
          const session: ResizeSession = {
            x: event.clientX,
            width,
            currentWidth: width,
            shell,
            sidebar,
            cleanup: () => undefined,
          };
          const move = (moveEvent: PointerEvent) => {
            const nextWidth = clampWidth(session.width + session.x - moveEvent.clientX);
            if (nextWidth === session.currentWidth) return;
            session.currentWidth = nextWidth;
            session.shell.style.setProperty("--web-right-panel-width", `${nextWidth}px`);
          };
          const finish = (commit: boolean) => {
            window.removeEventListener("pointermove", move);
            window.removeEventListener("pointerup", finishPointer);
            window.removeEventListener("pointercancel", cancelPointer);
            session.shell.classList.remove("web-right-panel-resizing");
            session.sidebar.classList.remove("is-resizing");
            if (resizeSession.current === session) resizeSession.current = null;
            if (commit) onWidthChange(session.currentWidth);
          };
          const finishPointer = () => finish(true);
          const cancelPointer = () => finish(false);
          session.cleanup = finish;
          resizeSession.current = session;
          shell.classList.add("web-right-panel-resizing");
          sidebar.classList.add("is-resizing");
          window.addEventListener("pointermove", move);
          window.addEventListener("pointerup", finishPointer, { once: true });
          window.addEventListener("pointercancel", cancelPointer, { once: true });
        }}
        onKeyDown={(event) => {
          if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
          event.preventDefault();
          onWidthChange(clampWidth(width + (event.key === "ArrowLeft" ? 16 : -16)));
        }}
      />
      <header className="web-right-sidebar-header">
        <div className="web-right-sidebar-tabs" role="tablist" aria-label="Details panel">
          <button
            type="button"
            role="tab"
            aria-selected={activeTab === "agents"}
            aria-controls="web-agent-panel"
            className={activeTab === "agents" ? "is-active" : ""}
            onClick={() => onTabChange("agents")}
          >
            <Bot size={14} aria-hidden="true" />
            <span>Agents</span>
            {agentUnread ? <span className="web-agent-unread" aria-label="New Agent activity" /> : null}
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={activeTab === "files"}
            aria-controls="web-files-panel"
            className={activeTab === "files" ? "is-active" : ""}
            onClick={() => onTabChange("files")}
          >
            <Files size={14} aria-hidden="true" />
            <span>Files</span>
          </button>
        </div>
        <button type="button" className="web-right-sidebar-close" onClick={onClose} aria-label="Close details panel">
          <X size={15} />
        </button>
      </header>
      <div
        id="web-agent-panel"
        role="tabpanel"
        aria-label="Agent activity"
        className="web-right-sidebar-panel"
        hidden={activeTab !== "agents"}
      >
        {agentPanel}
      </div>
      <div
        id="web-files-panel"
        role="tabpanel"
        aria-label="Workspace files"
        className="web-right-sidebar-panel"
        hidden={activeTab !== "files"}
      >
        {filePanel}
      </div>
    </aside>
  );
}
