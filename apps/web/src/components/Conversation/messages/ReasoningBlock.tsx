import { useState } from "react";

type Props = {
  text: string;
  summary?: string;
  meta?: string;
};

export default function ReasoningBlock({ text, summary, meta }: Props) {
  const [open, setOpen] = useState(false);
  return (
    <div className="web-reasoning">
      <div className="web-reasoning-header" onClick={() => setOpen(!open)}>
        <span className="web-reasoning-label">{summary || "Reasoning"}</span>
        {meta && <span className="web-reasoning-meta">{meta}</span>}
        <span className={`web-reasoning-chevron${open ? " web-reasoning-chevron-open" : ""}`}>
          ▶
        </span>
      </div>
      {open && <div className="web-reasoning-body">{text}</div>}
    </div>
  );
}
