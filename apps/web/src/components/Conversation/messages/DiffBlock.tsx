type DiffLine = { type: "add" | "del" | "ctx"; text: string };

type Props = {
  title: string;
  lines: DiffLine[];
};

export default function DiffBlock({ title, lines }: Props) {
  return (
    <div className="web-diff-block">
      <div className="web-diff-header">{title}</div>
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
    </div>
  );
}
