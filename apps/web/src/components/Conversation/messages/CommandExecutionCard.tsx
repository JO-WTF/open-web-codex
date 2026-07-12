import { useState } from "react";

type Props = {
  command: string;
  output?: string;
  exitCode?: number;
  durationMs?: number;
  cwd?: string;
};

export default function CommandExecutionCard({ command, output, exitCode, durationMs, cwd }: Props) {
  const [open, setOpen] = useState(false);
  const ok = exitCode === 0;

  const shortCmd = command.replace(/^\/bin\/zsh -lc '/, "").replace(/'$/, "").slice(0, 120);

  return (
    <div className="web-cmdex-card">
      <div className="web-cmdex-header" onClick={() => setOpen(!open)}>
        <span className={`web-cmdex-status ${ok ? "web-cmdex-ok" : "web-cmdex-err"}`}>
          {ok ? "\u2713" : "\u2717"} {ok ? "OK" : `exit ${exitCode}`}
        </span>
        <code className="web-cmdex-cmd">{shortCmd}</code>
        {durationMs != null && <span className="web-cmdex-dur">{durationMs}ms</span>}
        <span className={`web-cmdex-chevron${open ? " web-cmdex-chevron-open" : ""}`}>&#9654;</span>
      </div>
      {open && (
        <div className="web-cmdex-body">
          {output ? (
            <pre className="web-cmdex-output"><code>{output}</code></pre>
          ) : (
            <div className="web-cmdex-no-output">(no output)</div>
          )}
          {cwd && <div className="web-cmdex-cwd">cwd: {cwd}</div>}
        </div>
      )}
    </div>
  );
}
