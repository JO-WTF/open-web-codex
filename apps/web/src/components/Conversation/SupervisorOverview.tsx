import Bot from "lucide-react/dist/esm/icons/bot";
import FileCheck2 from "lucide-react/dist/esm/icons/file-check-2";
import Network from "lucide-react/dist/esm/icons/network";
import ShieldCheck from "lucide-react/dist/esm/icons/shield-check";
import type {
  ArtifactSummary,
  RuntimeAgentProjection,
  SupervisorPolicyBinding,
} from "../../../browser/types";

type Props = {
  policy: SupervisorPolicyBinding | null;
  agents: RuntimeAgentProjection[];
  artifacts: ArtifactSummary[];
  loading?: boolean;
  error?: string | null;
};

function agentLabel(agent: RuntimeAgentProjection): string {
  if (agent.is_root) return "Root Supervisor";
  return agent.agent_nickname?.trim()
    || agent.agent_role?.trim()
    || "Runtime Agent";
}

function statusPresentation(agent: RuntimeAgentProjection): {
  label: string;
  tone: "idle" | "active" | "waiting" | "terminal" | "error";
} {
  const flags = new Set(agent.active_flags);
  if ([...flags].some((flag) => flag.toLowerCase().includes("waiting"))) {
    return { label: "Waiting", tone: "waiting" };
  }
  switch (agent.status_type) {
    case "active":
    case "running":
    case "inProgress":
      return { label: "Running", tone: "active" };
    case "completed":
      return { label: "Completed", tone: "terminal" };
    case "failed":
    case "error":
    case "systemError":
      return { label: "Failed", tone: "error" };
    case "interrupted":
      return { label: "Interrupted", tone: "terminal" };
    case "idle":
      return { label: "Ready", tone: "idle" };
    default:
      return { label: "Unknown", tone: "idle" };
  }
}

export default function SupervisorOverview({
  policy,
  agents,
  artifacts,
  loading = false,
  error = null,
}: Props) {
  if (!policy && !loading && !error) return null;

  const orderedAgents = [...agents].sort((left, right) => {
    if (left.is_root !== right.is_root) return left.is_root ? -1 : 1;
    return left.first_observed_at.localeCompare(right.first_observed_at);
  });

  return (
    <section className="web-supervisor-overview" aria-label="Enterprise Supervisor collaboration">
      <div className="web-supervisor-overview-heading">
        <span className="web-supervisor-overview-icon" aria-hidden="true">
          <ShieldCheck size={16} />
        </span>
        <div>
          <strong>{policy?.display_name ?? "Enterprise Supervisor Copilot"}</strong>
          <span>
            {policy
              ? `Policy ${policy.policy_id} · ${policy.version}`
              : "Loading the bound Supervisor Policy…"}
          </span>
        </div>
        {policy ? (
          <span className={`web-supervisor-binding is-${policy.state}`}>
            {policy.state}
          </span>
        ) : null}
      </div>

      {error ? (
        <p className="web-supervisor-overview-error" role="alert">{error}</p>
      ) : loading && !policy ? (
        <p className="web-supervisor-overview-empty" role="status">
          Loading governed collaboration state…
        </p>
      ) : (
        <>
          <div className="web-supervisor-agent-list" role="list" aria-label="Runtime Agents">
            {orderedAgents.map((agent) => {
              const status = statusPresentation(agent);
              return (
                <div className="web-supervisor-agent" role="listitem" key={agent.thread_id}>
                  <span className="web-supervisor-agent-icon" aria-hidden="true">
                    {agent.is_root ? <Bot size={15} /> : <Network size={15} />}
                  </span>
                  <span className="web-supervisor-agent-copy">
                    <strong>{agentLabel(agent)}</strong>
                    <span>
                      {agent.is_root
                        ? "Owns task decomposition and final synthesis"
                        : agent.agent_role ?? "Runtime role not reported"}
                    </span>
                  </span>
                  <span className={`web-supervisor-agent-status is-${status.tone}`}>
                    <span aria-hidden="true" />
                    {status.label}
                  </span>
                </div>
              );
            })}
            {orderedAgents.length === 0 ? (
              <p className="web-supervisor-overview-empty">
                Waiting for the Runtime-owned root Thread projection.
              </p>
            ) : null}
          </div>
          <div className="web-supervisor-artifacts" aria-label="Evidence Artifacts">
            <div className="web-supervisor-section-heading">
              <FileCheck2 size={14} aria-hidden="true" />
              <strong>Evidence Artifacts</strong>
              <span>{artifacts.length}</span>
            </div>
            {artifacts.length ? (
              <div className="web-supervisor-artifact-list" role="list">
                {artifacts.map((artifact) => (
                  <div className="web-supervisor-artifact" role="listitem" key={artifact.id}>
                    <span className="web-supervisor-artifact-copy">
                      <strong>{artifact.artifact_schema}</strong>
                      <span>
                        {artifact.producer_agent_role ?? "Root Supervisor"}
                        {artifact.byte_size !== null
                          ? ` · ${artifact.byte_size.toLocaleString()} bytes`
                          : ""}
                      </span>
                    </span>
                    <span className={`web-supervisor-artifact-state is-${artifact.state}`}>
                      {artifact.state}
                    </span>
                  </div>
                ))}
              </div>
            ) : (
              <p className="web-supervisor-overview-empty">
                Waiting for validated Runtime Resources.
              </p>
            )}
          </div>
        </>
      )}
    </section>
  );
}
