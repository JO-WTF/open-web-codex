// Multi-agent business scenario: natural request plus typed acceptance helpers.

export const taskPrompt = [
  "请根据已上传的仓网数据，先评估当前各城市在 12 小时内送达的覆盖情况，并给出需求量加权的达标率。",
  "在这个基线之上，只新增候选仓 Balikpapan，比较新增前后 12 小时达标率的变化；请分别说明城市数量口径和需求量加权口径的变化率。",
  "最后请在对话中展示这次评估结果的地图。",
].join("\n");

export function facilityChangeMatches({
  event,
  eventData,
  facilityChangeEvidence,
  candidateWarehouseId,
  serviceTargetHours,
}) {
  const argumentsValue = eventData(event)?.arguments;
  const scenario = argumentsValue?.scenario;
  const evidence = facilityChangeEvidence(event);
  const exactAddition =
    Array.isArray(scenario?.add_warehouse_ids) &&
    scenario.add_warehouse_ids.length === 1 &&
    scenario.add_warehouse_ids[0] === candidateWarehouseId;
  const noRemovalOrRelocation =
    (scenario?.remove_warehouse_ids === undefined ||
      (Array.isArray(scenario.remove_warehouse_ids) &&
        scenario.remove_warehouse_ids.length === 0)) &&
    (scenario?.relocations === undefined ||
      (Array.isArray(scenario.relocations) && scenario.relocations.length === 0));
  const hasExactServiceTarget =
    Array.isArray(scenario?.service_targets) &&
    scenario.service_targets.length === 1 &&
    scenario.service_targets[0] === serviceTargetHours;
  const usesBaseline = argumentsValue?.before_ref?.resource_schema === "network_baseline.v2";
  const resultMatchesRequest =
    evidence?.added_warehouse_ids.length === 1 &&
    evidence.added_warehouse_ids[0] === candidateWarehouseId &&
    evidence.removed_warehouse_ids.length === 0 &&
    evidence.target_hours === serviceTargetHours &&
    Number.isFinite(evidence.city_coverage_rate_delta) &&
    Number.isFinite(evidence.demand_weighted_coverage_rate_delta);
  return {
    valid:
      scenario?.objective === "min_time" &&
      exactAddition &&
      noRemovalOrRelocation &&
      hasExactServiceTarget &&
      usesBaseline &&
      resultMatchesRequest,
    evidence,
  };
}

export function hasStandaloneMapEmbedInFinalRootMessage(events, rootThreadId) {
  const rootMessages = events
    .filter((event) => {
      if (event?.thread_id !== rootThreadId) return false;
      const data = event?.payload?.data ?? {};
      const itemType = event?.payload?.itemType ?? data.type;
      return itemType === "agentMessage" || data.type === "message";
    })
    .map((event) => {
      const data = event?.payload?.data ?? {};
      return typeof data.text === "string"
        ? data.text
        : typeof data.message === "string"
          ? data.message
          : typeof data.content === "string"
            ? data.content
            : "";
    });
  const finalMessage = rootMessages.at(-1) ?? "";
  return finalMessage
    .split(/\r?\n/)
    .some((line) => /^::codex-inline-vis\{artifact="[^"]+"\}$/.test(line.trim()));
}

export function validateBusinessEvidence({
  dataHandoff,
  baseline,
  baselineCoverage,
  roles,
  facilityChangeValid,
  facilityChangeDetails,
  mapProducerCompleted,
  rootFinalMapEmbed,
  generatedWorkspaceFiles,
}) {
  const warningCount = dataHandoff?.warning_count;
  const warnings = dataHandoff?.warnings;
  const handoffWarningsValid =
    Number.isInteger(warningCount) &&
    Array.isArray(warnings) &&
    warnings.length === Math.min(warningCount, 64) &&
    dataHandoff?.warnings_truncated === (warningCount > 64);
  const candidateCountValid =
    dataHandoff?.outcome === "prepared_ready"
      ? Number.isInteger(dataHandoff.role_counts?.candidate_warehouse)
      : Number.isInteger(dataHandoff?.candidate_warehouse_count);
  if (
    !["ready", "prepared_ready"].includes(dataHandoff?.outcome) ||
    !["created", "reused"].includes(dataHandoff?.operation) ||
    typeof dataHandoff?.prepared_input_relative_path !== "string" ||
    !dataHandoff.prepared_input_relative_path.startsWith("outputs/warehouse-network/prepared/") ||
    !/^[a-f0-9]{64}$/.test(dataHandoff.input_identity?.content_sha256 ?? "") ||
    !dataHandoff.role_counts ||
    !candidateCountValid ||
    !handoffWarningsValid
  ) {
    return { code: "multi_agent_typed_data_handoff_invalid" };
  }
  if (
    baseline?.resource_ref?.resource_schema !== "network_baseline.v2" ||
    typeof baselineCoverage?.city_coverage_rate !== "number" ||
    !Number.isFinite(baselineCoverage.city_coverage_rate) ||
    typeof baselineCoverage?.demand_weighted_coverage_rate !== "number" ||
    !Number.isFinite(baselineCoverage.demand_weighted_coverage_rate)
  ) {
    return { code: "multi_agent_typed_baseline_invalid" };
  }
  if (!roles.has("data_agent") || !roles.has("network_agent")) {
    return {
      code: "multi_agent_child_roles_missing",
      details: { observed_roles: [...roles].filter((role) => typeof role === "string") },
    };
  }
  if (!facilityChangeValid) {
    return {
      code: facilityChangeDetails
        ? "balikpapan_facility_change_invalid"
        : "balikpapan_facility_change_not_completed",
      details: { facilityChange: facilityChangeDetails },
    };
  }
  if (
    mapProducerCompleted !== true
  ) {
    return { code: "copilot_chain_incomplete", details: { reason: "map_producer_item_not_projected" } };
  }
  if (rootFinalMapEmbed !== true) {
    return { code: "map_embed_not_forwarded", details: { reason: "root_final_message_missing_standalone_embed" } };
  }
  if (
    !Array.isArray(generatedWorkspaceFiles) ||
    !generatedWorkspaceFiles.some((relativePath) =>
      relativePath.startsWith("outputs/warehouse-network/prepared/"),
    ) ||
    generatedWorkspaceFiles.some(
      (relativePath) => !relativePath.startsWith("outputs/warehouse-network/"),
    )
  ) {
    return {
      code: "generated_workspace_output_scope_invalid",
      details: { generatedWorkspaceFiles: generatedWorkspaceFiles.slice(0, 40) },
    };
  }
  return { valid: true };
}
