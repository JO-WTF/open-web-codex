import { useState } from "react";
import ChevronRight from "lucide-react/dist/esm/icons/chevron-right";
import Brain from "lucide-react/dist/esm/icons/brain";

type Props = {
  text: string;
  summary?: string;
  meta?: string;
  streaming?: boolean;
};

function previewLabel(text: string): string {
  // Show first line or first 80 chars as label
  const firstLine = text.split("\n")[0] ?? "";
  if (firstLine.length > 80) return firstLine.slice(0, 80) + "…";
  return firstLine || "Reasoning";
}

export default function ReasoningBlock({ text, summary, meta, streaming = false }: Props) {
  // Reasoning is valuable execution context; show it on arrival instead of
  // leaving a barely discoverable collapsed row between messages.
  const [open, setOpen] = useState(true);
  const trimmedText = text.trim();
  if (!trimmedText) return null;

  const label = streaming ? previewLabel(trimmedText === "Reasoning in progress" ? "Reasoning" : trimmedText) : summary || previewLabel(trimmedText === "Reasoning completed" ? "Reasoning" : trimmedText);
  const hasDetails = streaming || trimmedText !== "Reasoning completed";

  return (
    <div className="web-reasoning">
      <div className="web-reasoning-header" onClick={() => setOpen(!open)} role="button" tabIndex={0} onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") setOpen(!open); }}>
        <span className={`web-reasoning-chevron${open ? " web-reasoning-chevron-open" : ""}`}>
          <ChevronRight size={12} />
        </span>
        <Brain size={14} className="web-reasoning-icon" aria-hidden="true" />
        {streaming && <span className="web-reasoning-working" aria-hidden="true" />}
        <span className="web-reasoning-label" title={trimmedText}>{label}</span>
        {meta && <span className="web-reasoning-meta">{meta}</span>}
      </div>
      {open && hasDetails && (
        <div className="web-reasoning-body">
          {trimmedText.split("\n").map((line, i) => (
            <p key={i} className="web-reasoning-line">{line || "\u00A0"}</p>
          ))}
        </div>
      )}
    </div>
  );
}
