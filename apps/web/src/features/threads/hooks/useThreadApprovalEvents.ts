import { useCallback } from "react";
import type { Dispatch } from "react";
import type { ApprovalRequest } from "@/types";
import type { ThreadAction } from "./useThreadsReducer";

type UseThreadApprovalEventsOptions = {
  dispatch: Dispatch<ThreadAction>;
};

export function useThreadApprovalEvents({
  dispatch,
}: UseThreadApprovalEventsOptions) {
  return useCallback(
    (approval: ApprovalRequest) => {
      dispatch({ type: "addApproval", approval });
    },
    [dispatch],
  );
}
