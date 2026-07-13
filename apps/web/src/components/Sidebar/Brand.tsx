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
      <div>
        <div className="web-brand-sub">open-web-codex</div>
        <div className="web-brand-title">Codex</div>
      </div>
      <span className={`web-badge web-badge-${state}`}>
        <span className="web-badge-dot" />
        {version ?? LABELS[state]}
      </span>
    </div>
  );
}
