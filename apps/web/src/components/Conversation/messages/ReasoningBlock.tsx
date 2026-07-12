import { useState } from "react";
import ChevronRight from "lucide-react/dist/esm/icons/chevron-right";

type Props = {
  text: string;
  summary?: string;
  meta?: string;
};

function previewLabel(text: string): string {
  // Show first line or first 80 chars as label
  const firstLine = text.split("\n")[0] ?? "";
  if (firstLine.length > 80) return firstLine.slice(0, 80) + "…";
  return firstLine || "Reasoning";
}

export default function ReasoningBlock({ text, summary, meta }: Props) {
  const [open, setOpen] = useState(false);
  const trimmedText = text.trim();
  if (!trimmedText) return null;

  const label = summary || previewLabel(trimmedText);

  return (
    <div className="web-reasoning">
      <div className="web-reasoning-header" onClick={() => setOpen(!open)} role="button" tabIndex={0} onKeyDown={(e) => { if (e.key === "Enter") setOpen(!open); }}>
        <span className={`web-reasoning-chevron${open ? " web-reasoning-chevron-open" : ""}`}>
          <ChevronRight size={12} />
        </span>
        <span className="web-reasoning-label" title={trimmedText}>{label}</span>
        {meta && <span className="web-reasoning-meta">{meta}</span>}
      </div>
      {open && (
        <div className="web-reasoning-body">
          {trimmedText.split("\n").map((line, i) => (
            <p key={i} className="web-reasoning-line">{line || "\u00A0"}</p>
          ))}
        </div>
      )}
    </div>
  );
}
