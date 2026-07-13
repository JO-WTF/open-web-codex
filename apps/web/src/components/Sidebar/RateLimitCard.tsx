type WindowInfo = {
  usedPercent?: unknown;
  resetsAt?: unknown;
};

type Props = {
  rateLimits: Record<string, unknown>;
};

function resetLabel(value: unknown) {
  if (typeof value !== "number" || !Number.isFinite(value)) return "Reset time unavailable";
  const milliseconds = value < 10_000_000_000 ? value * 1000 : value;
  const minutes = Math.max(0, Math.round((milliseconds - Date.now()) / 60_000));
  if (minutes < 60) return `Resets in ${minutes}m`;
  const hours = Math.round(minutes / 60);
  if (hours < 48) return `Resets in ${hours}h`;
  return `Resets in ${Math.round(hours / 24)}d`;
}

function UsageRow({ label, value }: { label: string; value: WindowInfo | null }) {
  if (!value || typeof value.usedPercent !== "number") return null;
  const usedPercent = Math.max(0, Math.min(100, Math.round(value.usedPercent)));
  return (
    <div className="web-quota-row">
      <div className="web-quota-copy">
        <strong>{label}</strong>
        <span>· {resetLabel(value.resetsAt)}</span>
        <b>{usedPercent}%</b>
      </div>
      <span className="web-quota-track" aria-label={`${label} ${usedPercent}% used`}>
        <span style={{ width: `${usedPercent}%` }} />
      </span>
    </div>
  );
}

export default function RateLimitCard({ rateLimits }: Props) {
  const primary = rateLimits.primary && typeof rateLimits.primary === "object"
    ? rateLimits.primary as WindowInfo
    : null;
  const secondary = rateLimits.secondary && typeof rateLimits.secondary === "object"
    ? rateLimits.secondary as WindowInfo
    : null;
  const credits = rateLimits.credits && typeof rateLimits.credits === "object"
    ? rateLimits.credits as Record<string, unknown>
    : null;
  const creditText = credits?.hasCredits
    ? credits.unlimited ? "Unlimited credits" : `${credits.balance ?? "--"} credits`
    : null;

  if (!primary && !secondary && !creditText) return null;
  return (
    <section className="web-quota-card" aria-label="Codex usage limits">
      <UsageRow label="Session" value={primary} />
      <UsageRow label="Weekly" value={secondary} />
      {creditText ? <div className="web-quota-credits">Credits: {creditText}</div> : null}
    </section>
  );
}
