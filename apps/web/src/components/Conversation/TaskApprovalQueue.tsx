import ShieldAlert from "lucide-react/dist/esm/icons/shield-alert";
import type { ApprovalStatus } from "../../utils/approvalStatus";
import ApprovalCard from "./messages/ApprovalCard";

export type TaskApprovalRequest = {
  threadId: string;
  actorLabel: string;
  workspaceId: string;
  requestId: number | string;
  command: string;
  status?: ApprovalStatus;
  mode?: string;
  credentialKind?: "maps";
  submitting?: boolean;
};

type Props = {
  approvals: TaskApprovalRequest[];
  ariaLabel: string;
  onResolve?: (
    workspaceId: string,
    requestId: number | string,
    decision: "accept" | "decline",
  ) => void;
};

export default function TaskApprovalQueue({
  approvals,
  ariaLabel,
  onResolve,
}: Props) {
  if (approvals.length === 0) return null;

  return (
    <section className="web-task-approval-queue" aria-label={ariaLabel}>
      <div className="web-supervisor-section-heading">
        <ShieldAlert size={14} aria-hidden="true" />
        <strong>Approvals</strong>
        <span>{approvals.length}</span>
      </div>
      <div className="web-task-approval-list">
        {approvals.map((approval) => (
          <article
            className="web-task-approval"
            key={`${approval.threadId}:${String(approval.requestId)}`}
          >
            <div className="web-task-approval-actor">
              <strong>{approval.actorLabel}</strong>
              <span>
                {approval.submitting
                  ? "Submitting your decision…"
                  : "Waiting for your decision"}
              </span>
            </div>
            <ApprovalCard
              command={approval.command}
              workspaceId={approval.workspaceId}
              requestId={approval.requestId}
            status={approval.status}
            mode={approval.mode}
            credentialKind={approval.credentialKind}
              submitting={approval.submitting}
              onResolve={onResolve}
            />
          </article>
        ))}
      </div>
    </section>
  );
}
