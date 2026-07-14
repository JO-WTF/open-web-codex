import { useState } from "react";
import CheckCircle2 from "lucide-react/dist/esm/icons/circle-check";
import Circle from "lucide-react/dist/esm/icons/circle";
import LoaderCircle from "lucide-react/dist/esm/icons/loader-circle";
import type { TurnPlanStep } from "../../types";

export type GoalInfo = {
  objective: string;
  status: string;
  tokenBudget: number | null;
  tokensUsed: number;
  timeUsedSeconds: number;
  steps?: TurnPlanStep[];
  fileCount?: number;
  additions?: number;
  deletions?: number;
};

type Props = {
  goal: GoalInfo | null;
};

export default function GoalBanner({ goal }: Props) {
  const [open, setOpen] = useState(false);
  const steps = goal?.steps ?? [];
  const completed = steps.filter((step) => step.status === "completed").length;
  const currentStep = Math.min(completed + 1, steps.length || 1);
  const progressLabel = goal
    ? steps.length ? `Step ${currentStep} / ${steps.length}` : goal.objective
    : "No active goal";
  const showDetail = Boolean(goal && open && steps.length > 0);

  return (
    <div className={`web-goal-banner${goal ? "" : " is-empty"}`} onMouseEnter={() => goal && setOpen(true)} onMouseLeave={() => setOpen(false)}>
      {showDetail && (
        <div className="web-goal-detail" role="list" aria-label="Goal steps">
          {steps.map((step, index) => (
            <div className={`web-goal-step is-${step.status}`} role="listitem" key={`${step.step}-${index}`}>
              {step.status === "completed" ? <CheckCircle2 size={14} /> : step.status === "inProgress" ? <LoaderCircle size={14} /> : <Circle size={14} />}
              <span>{step.step}</span>
            </div>
          ))}
        </div>
      )}
      <button type="button" className="web-goal-header" onClick={() => goal && setOpen(!open)} aria-expanded={showDetail} aria-disabled={!goal}>
        <span className={`web-goal-ring${goal?.status === "active" ? " is-active" : ""}`} />
        <span className="web-goal-progress-label">{progressLabel}</span>
        {goal?.fileCount != null && <span className="web-goal-files">· {goal.fileCount} files changed</span>}
        {goal?.additions != null && <span className="web-goal-additions">+{goal.additions}</span>}
        {goal?.deletions != null && <span className="web-goal-deletions">-{goal.deletions}</span>}
      </button>
    </div>
  );
}
