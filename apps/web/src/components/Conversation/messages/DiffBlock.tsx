import { useState } from "react";
import ChevronRight from "lucide-react/dist/esm/icons/chevron-right";
import FileDiff from "lucide-react/dist/esm/icons/file-diff";

type DiffLine = { type: "add" | "del" | "ctx"; text: string };

type Props = {
  title: string;
  lines: DiffLine[];
  updating?: boolean;
};

export default function DiffBlock({ title, lines, updating = false }: Props) {
  const [open, setOpen] = useState(true);

  return (
    <div className="web-diff-block">
      <button
        type="button"
        className="web-diff-header"
        aria-expanded={open}
        onClick={() => setOpen((current) => !current)}
      >
        <ChevronRight className={open ? "web-diff-chevron is-open" : "web-diff-chevron"} size={14} />
        <FileDiff className="web-diff-icon" size={15} />
        <span className="web-diff-title">{title}</span>
        <span className={updating ? "web-diff-state is-updating" : "web-diff-state"}>
          {updating && <span className="web-diff-spinner" aria-hidden="true" />}
          {updating ? "Updating files…" : "Completed"}
        </span>
      </button>
      {open && (
        <div className="web-diff-lines">
          {lines.map((line, i) => (
            <div key={i} className={`web-diff-line web-diff-line-${line.type}`}>
              <span className="web-diff-line-sign">
                {line.type === "add" ? "+" : line.type === "del" ? "-" : " "}
              </span>
              <span>{line.text}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
