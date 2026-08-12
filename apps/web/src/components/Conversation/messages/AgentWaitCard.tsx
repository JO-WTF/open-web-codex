import { useId } from "react";
import Network from "lucide-react/dist/esm/icons/network";
import Timer from "lucide-react/dist/esm/icons/timer";

type Props = {
  status: string;
  elapsedLabel?: string;
  live?: boolean;
  onOpenAgentPanel?: () => void;
};

type Presentation = {
  title: string;
  description: string;
  label: string;
  tone: "active" | "done" | "error" | "pending";
};

function waitPresentation(status: string): Presentation {
  const normalized = status.toLowerCase().replace(/[^a-z0-9]/g, "");
  if (normalized === "inprogress" || normalized === "running") {
    return {
      title: "Waiting for Agent updates",
      description: "The Supervisor is still active and will continue when an Agent reports back or you add new input.",
      label: "Waiting",
      tone: "active",
    };
  }
  if (normalized === "failed" || normalized === "error" || normalized === "interrupted") {
    return {
      title: "Agent wait interrupted",
      description: "Review Agent activity for the latest recovery or failure details.",
      label: "Needs attention",
      tone: "error",
    };
  }
  if (normalized === "completed" || normalized === "done") {
    return {
      title: "Agent update received",
      description: "The wait cycle finished and the Supervisor can continue the collaboration.",
      label: "Completed",
      tone: "done",
    };
  }
  return {
    title: "Preparing to wait for Agents",
    description: "The Supervisor is preparing a bounded wait for the next Agent update.",
    label: "Preparing",
    tone: "pending",
  };
}

export default function AgentWaitCard({
  status,
  elapsedLabel,
  live = false,
  onOpenAgentPanel,
}: Props) {
  const presentation = waitPresentation(status);
  const titleId = useId();
  return (
    <article
      className={`web-agent-wait-card is-${presentation.tone}`}
      aria-labelledby={titleId}
      {...(live ? { role: "status" } : {})}
    >
      <span className="web-agent-wait-icon" aria-hidden="true">
        <Network size={17} />
      </span>
      <div className="web-agent-wait-copy">
        <div className="web-agent-wait-heading">
          <strong id={titleId}>{presentation.title}</strong>
          <span className={`web-agent-wait-status is-${presentation.tone}`}>
            <span aria-hidden="true" />
            {presentation.label}
          </span>
        </div>
        <p>{presentation.description}</p>
        <div className="web-agent-wait-meta">
          {elapsedLabel ? (
            <span aria-hidden="true">
              <Timer size={12} aria-hidden="true" />
              {elapsedLabel}
            </span>
          ) : null}
          {onOpenAgentPanel ? (
            <button type="button" onClick={onOpenAgentPanel}>
              Open Agent activity
            </button>
          ) : null}
        </div>
      </div>
    </article>
  );
}
