import type { RequestUserInputRequest } from "../types";
import type { PlatformApproval } from "../services/platformTypes";
import { parseWebUserInputRequest } from "./webUserInput";

export function parseUserInputFromApproval(
  approval: PlatformApproval,
): RequestUserInputRequest | null {
  if (approval.request_type !== "item/tool/requestUserInput") {
    return null;
  }
  if (!approval.workspace_id || approval.codex_request_id == null) {
    return null;
  }
  if (approval.status !== "pending") {
    return null;
  }
  return parseWebUserInputRequest(
    approval.workspace_id,
    approval.codex_request_id,
    approval.request_payload ?? {},
  );
}
