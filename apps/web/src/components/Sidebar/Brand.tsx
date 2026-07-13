type BrandProps = {
  state: "checking" | "online" | "offline";
  version: string | null;
};

const LABELS: Record<BrandProps["state"], string> = {
  checking: "Checking",
  online: "Online",
  offline: "Offline",
};

export default function Brand({ state, version }: BrandProps) {
  return (
    <div className="web-brand">
      <div className="web-brand-heading">
        <FolderKanban size={18} aria-hidden="true" />
        <div className="web-brand-title">Projects</div>
      </div>
      <span className={`web-brand-connection web-badge-${state}`} title={version ?? LABELS[state]}>
        <span className="web-badge-dot" />
        <span className="web-sr-only">{version ?? LABELS[state]}</span>
      </span>
    </div>
  );
}
import FolderKanban from "lucide-react/dist/esm/icons/folder-kanban";
