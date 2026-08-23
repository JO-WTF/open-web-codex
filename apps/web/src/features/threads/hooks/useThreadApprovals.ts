import { useCallback } from "react";
import type { Dispatch } from "react";
import type { ApprovalRequest } from "@/types";
import { respondToServerRequest } from "@services/tauri";
import type { ThreadAction } from "./useThreadsReducer";

type UseThreadApprovalsOptions = {
  dispatch: Dispatch<ThreadAction>;
};

export function useThreadApprovals({ dispatch }: UseThreadApprovalsOptions) {
  const handleApprovalDecision = useCallback(
    async (request: ApprovalRequest, decision: "accept" | "decline") => {
      await respondToServerRequest(
        request.workspace_id,
        request.request_id,
        decision,
      );
      dispatch({
        type: "removeApproval",
        requestId: request.request_id,
        workspaceId: request.workspace_id,
      });
    },
    [dispatch],
  );

  return {
    handleApprovalDecision,
  };
}
