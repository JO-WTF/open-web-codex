import { useEffect, useState } from "react";
import type {
  DataIntakeParameterAnswer,
  DataIntakeSessionSummary,
  DataMappingCandidate,
} from "../../../browser/types";

type Props = {
  session: DataIntakeSessionSummary | null;
  loading: boolean;
  error: string | null;
  onRefresh: () => void;
  onOpenUpload: () => void;
  onConfirmMapping: (confirmed: DataMappingCandidate[]) => void;
  onSubmitParameters: (answers: DataIntakeParameterAnswer[]) => void;
  onConfirmAnalysis: (requestId: string) => void;
  onRequestChange: (message: string) => void;
};

function mappingKey(candidate: DataMappingCandidate) {
  return [
    candidate.sourceAssetId,
    candidate.sourceRef,
    candidate.sourcePath,
    candidate.sourceField,
    candidate.targetEntity,
    candidate.targetField,
  ].join("|");
}

function statusLabel(session: DataIntakeSessionSummary) {
  const openRequests = session.inputRequests.filter((request) => request.status === "open");
  if (session.status === "active" && openRequests.length > 0) {
    return "Awaiting your input";
  }
  return {
    active: "Preparing planning inputs",
    ready: "Ready for analysis",
    failed: "Data preparation failed",
    cancelled: "Data preparation cancelled",
  }[status];
}

function answerValue(session: DataIntakeSessionSummary, name: string) {
  const answer = session.answers.find((item) => item.name === name);
  if (answer?.value === undefined || answer.value === null) return "";
  return String(answer.value);
}

function isSyntheticDemo(value: unknown): boolean {
  if (!value || typeof value !== "object") return false;
  const profile = value as Record<string, unknown>;
  return profile.dataClassification === "synthetic_demo";
}

function hasDataPreparationEvidence(session: DataIntakeSessionSummary): boolean {
  const hasOpenRequest = session.inputRequests.some((request) => request.status === "open");
  return session.status !== "active"
    || session.requirementProfile !== null
    || session.gaps.length > 0
    || hasOpenRequest;
}

export default function DataIntakePanel({
  session,
  loading,
  error,
  onRefresh,
  onOpenUpload,
  onConfirmMapping,
  onSubmitParameters,
  onConfirmAnalysis,
  onRequestChange,
}: Props) {
  const [parameterValues, setParameterValues] = useState<Record<string, string>>({});
  const [parameterSources, setParameterSources] = useState<Record<string, string>>({});

  useEffect(() => {
    if (!session) return;
    setParameterValues(Object.fromEntries(
      session.parameters.map((parameter) => [parameter.name, answerValue(session, parameter.name)]),
    ));
    setParameterSources(Object.fromEntries(
      session.parameters.map((parameter) => [
        parameter.name,
        session.answers.find((answer) => answer.name === parameter.name)?.source ?? "user",
      ]),
    ));
  }, [session]);

  const openRequests = session?.inputRequests.filter((request) => request.status === "open") ?? [];
  const pendingKinds = new Set(openRequests.map((request) => request.kind));
  const showMapping = pendingKinds.has("confirm_mapping");
  const showDataGap = pendingKinds.has("provide_data") || (session?.status === "active" && session.gaps.length > 0 && !showMapping);
  const showFinalChecklist = pendingKinds.has("confirm_analysis");
  const showParameters = pendingKinds.has("answer_parameters");
  const hasPendingRequest = pendingKinds.size > 0;

  if (session && !error && !hasDataPreparationEvidence(session)) return null;
  if (loading) {
    return <section className="web-data-intake" aria-live="polite">Loading data readiness…</section>;
  }

  return (
    <section className="web-data-intake" aria-label="Data preparation">
      <div className="web-data-intake-header">
        <div>
          <span className="web-data-intake-kicker">Data preparation</span>
          <h3>{session ? session.contract.displayName : "Prepare planning data"}</h3>
        </div>
        {session ? (
          <div className="web-data-intake-actions">
            {isSyntheticDemo(session.sourceProfile) ? (
              <span className="web-data-intake-status">Synthetic demo</span>
            ) : null}
            <span className={`web-data-intake-status is-${session.status}`}>{statusLabel(session)}</span>
            <button type="button" className="ghost" onClick={onRefresh}>Refresh</button>
          </div>
        ) : null}
      </div>
      {error ? <p className="web-data-intake-error" role="alert">{error}</p> : null}
      {session?.requirementProfile ? (
        <div className="web-data-intake-profile">
          <strong>Planning data requirements</strong>
          <small>需求已生成，数据准备会自动继续；仅字段映射、业务参数和最终分析清单需要确认。</small>
          <ProfileCard value={session.requirementProfile} />
        </div>
      ) : null}
      {session ? openRequests.map((request) => (
        <div className="web-data-intake-request" role="status" key={request.requestId}>
          <strong>{request.prompt}</strong>
          {request.kind === "confirm_analysis" ? (
            <>
              <small>确认后，以上数据、映射和参数将被锁定为本次分析快照。</small>
              <div className="web-data-intake-actions">
                <button type="button" className="primary" disabled={loading} onClick={() => onConfirmAnalysis(request.requestId)}>
                  开始分析
                </button>
                <button
                  type="button"
                  className="ghost"
                  disabled={loading}
                  onClick={() => onRequestChange("我想修改最终分析 Checklist 中的数据、映射、参数或限制，请先不要开始分析。")}
                >
                  修改数据或参数
                </button>
              </div>
            </>
          ) : (
            <small>等待新的对话 Turn 或对应的确认卡片。</small>
          )}
        </div>
      )) : null}
      {!session ? (
        <p>Describe the planning goal or upload Excel, CSV or JSON in the conversation. The Network/Data Agents will publish the required profile and continue data preparation; only the mapping, parameters and final analysis checklist may need confirmation.</p>
      ) : null}
      {session && showDataGap ? (
        <>
          <p>Upload one or more planning files. No Dataset ID, version, role, or manifest is required.</p>
          {session.gaps.length > 0 ? <GapList gaps={session.gaps.map((gap) => gap.message)} /> : null}
          <div className="web-data-intake-actions">
            <button type="button" className="primary" onClick={onOpenUpload}>Upload planning files</button>
            <button type="button" className="ghost" onClick={onRefresh}>Refresh status</button>
          </div>
        </>
      ) : null}
      {session && showMapping ? (
        <>
          <p>These are candidates only. Confirm every field that affects the model; low-confidence matches are never accepted automatically.</p>
          <div className="web-data-intake-mappings">
            {session.candidates.map((candidate) => {
              const key = mappingKey(candidate);
              return (
                <div className="web-data-intake-mapping" key={key}>
                  <span>
                    <strong>{candidate.sourceDisplayName ?? "已上传数据文件"} · {candidate.sourceField}</strong> → {candidate.targetEntity}.{candidate.targetField}
                    <small>
                      {Math.round(candidate.confidence * 100)}% · {candidate.reason}
                      {candidate.sourceUnit || candidate.targetUnit
                        ? ` · 单位 ${candidate.sourceUnit ?? "未识别"} → ${candidate.targetUnit ?? "未定义"}`
                        : " · 单位待确认"}
                      {candidate.transformation ? ` · 转换 ${candidate.transformation}` : " · 转换 identity"}
                    </small>
                    {candidate.conflict ? <small className="web-data-intake-error">冲突：{candidate.conflict}</small> : null}
                  </span>
                </div>
              );
            })}
          </div>
          {session.gaps.length > 0 ? <GapList gaps={session.gaps.map((gap) => gap.message)} /> : null}
          {session.gaps.some((gap) => gap.required) ? (
            <div className="web-data-intake-actions">
              <button type="button" className="ghost" onClick={onOpenUpload}>Upload another file</button>
            </div>
          ) : null}
          <button
            type="button"
            className="primary"
            disabled={session.candidates.length === 0 || session.gaps.some((gap) => gap.code === "ambiguous_mapping")}
            onClick={() => onConfirmMapping(session.candidates)}
          >
            Confirm whole mapping
          </button>
          <button
            type="button"
            className="ghost"
            onClick={() => onRequestChange("我想修改上面的完整字段映射，请指出需要重新理解的文件、Sheet、数组路径或字段。")}
          >
            提出修改
          </button>
        </>
      ) : null}
      {showParameters && session ? (
        <form
          className="web-data-intake-parameters"
          onSubmit={(event) => {
            event.preventDefault();
            const answers = session.parameters.map((parameter) => ({
              name: parameter.name,
              value: parameter.dataType === "number"
                ? Number(parameterValues[parameter.name])
                : parameterValues[parameter.name] ?? "",
              unit: parameter.unit,
              source: parameterSources[parameter.name]?.trim() || "user",
            }));
            onSubmitParameters(answers);
          }}
        >
          <p>Confirm the assumptions that influence the result. Each answer must include its unit and source.</p>
          {session.parameters.map((parameter) => (
            <label className="web-data-intake-parameter" key={parameter.name}>
              <span>{parameter.displayName}{parameter.unit ? ` (${parameter.unit})` : ""}</span>
              <input
                required={parameter.required}
                type={parameter.dataType === "number" ? "number" : "text"}
                value={parameterValues[parameter.name] ?? ""}
                onChange={(event) => setParameterValues((current) => ({ ...current, [parameter.name]: event.target.value }))}
              />
              <small>{parameter.description}</small>
              <input
                required={parameter.required}
                placeholder="Source, e.g. business policy"
                value={parameterSources[parameter.name] ?? ""}
                onChange={(event) => setParameterSources((current) => ({ ...current, [parameter.name]: event.target.value }))}
              />
            </label>
          ))}
          <button type="submit" className="primary">Confirm parameters</button>
        </form>
      ) : null}
      {session && session.status === "active" && !hasPendingRequest && !showDataGap && !showMapping && !showParameters && !showFinalChecklist ? <p>输入已提交，Network/Data Agent 正在完成下一步画像、归一化或检查。</p> : null}
      {showFinalChecklist && session?.readinessReview ? (
        <div className="web-data-intake-checklist">
          <strong>Final analysis checklist</strong>
          <ChecklistCard value={session.readinessReview} />
        </div>
      ) : null}
      {session?.status === "ready" ? <p>Input Readiness is complete. Analysis can start from this same Thread.</p> : null}
      {session?.status === "failed" ? <p className="web-data-intake-error">{session.failureSummary ?? "The data preparation capability is unavailable."}</p> : null}
    </section>
  );
}

function GapList({ gaps }: { gaps: string[] }) {
  return (
    <ul className="web-data-intake-gaps">
      {gaps.map((gap) => <li key={gap}>{gap}</li>)}
    </ul>
  );
}

function ProfileCard({ value }: { value: unknown }) {
  const profile = value && typeof value === "object" ? value as Record<string, unknown> : {};
  const entities = Array.isArray(profile.entities)
    ? profile.entities
    : Array.isArray(profile.requiredEntities) ? profile.requiredEntities : [];
  const parameters = Array.isArray(profile.parameters)
    ? profile.parameters
    : Array.isArray(profile.businessParameters) ? profile.businessParameters : [];
  const outputs = Array.isArray(profile.outputs) ? profile.outputs : [];
  const assumptions = Array.isArray(profile.assumptions) ? profile.assumptions : [];
  const exclusions = Array.isArray(profile.exclusions) ? profile.exclusions : [];
  return (
    <div className="web-data-intake-profile-card">
      <h4>{String(profile.title ?? profile.goal ?? "本次规划的数据需求")}</h4>
      <p>{String(profile.problemType ?? profile.description ?? "Network Agent 已根据你的目标生成数据需求。")}</p>
      <div className="web-data-intake-profile-columns">
        <div><strong>需要的数据</strong><ul>{entities.map((item, index) => <li key={index}>{requirementEntityItem(item)}</li>)}</ul></div>
        <div><strong>需要确认的参数</strong><ul>{parameters.map((item, index) => <li key={index}>{humanItem(item)}</li>)}</ul></div>
        {outputs.length > 0 ? <div><strong>计划输出</strong><ul>{outputs.map((item, index) => <li key={index}>{humanItem(item)}</li>)}</ul></div> : null}
      </div>
      {assumptions.length > 0 ? <div><strong>需要确认的假设</strong><ul>{assumptions.map((item, index) => <li key={index}>{humanItem(item)}</li>)}</ul></div> : null}
      {exclusions.length > 0 ? <div><strong>本次不包含</strong><ul>{exclusions.map((item, index) => <li key={index}>{humanItem(item)}</li>)}</ul></div> : null}
    </div>
  );
}

function requirementEntityItem(value: unknown): string {
  if (!value || typeof value !== "object") return humanItem(value);
  const entity = value as Record<string, unknown>;
  const name = String(entity.displayName ?? entity.name ?? "数据实体");
  const fields = Array.isArray(entity.requiredFields)
    ? entity.requiredFields
    : Array.isArray(entity.fields) ? entity.fields : [];
  if (fields.length === 0) return name;
  const fieldNames = fields.map((field) => {
    if (!field || typeof field !== "object") return humanItem(field);
    const record = field as Record<string, unknown>;
    const label = String(record.displayName ?? record.name ?? "字段");
    const required = record.required === false ? "可选" : "必填";
    const unit = record.unit ? `，${String(record.unit)}` : "";
    return `${label}（${required}${unit}）`;
  });
  return `${name}：${fieldNames.join("、")}`;
}

function ChecklistCard({ value }: { value: unknown }) {
  const review = value && typeof value === "object" ? value as Record<string, unknown> : {};
  const sections = ["dataScope", "quality", "mapping", "parameters", "plannedAnalysis", "limitations", "outputs", "checks"];
  const entries = sections.flatMap((key) => {
    const item = review[key];
    return item === undefined || item === null ? [] : [[key, item] as const];
  });
  return (
    <div className="web-data-intake-checklist-card">
      <p>以下检查结果将作为本次分析快照：</p>
      <ul>{entries.length > 0 ? entries.map(([key, item]) => <li key={key}><strong>{humanLabel(key)}</strong><span>{humanItem(item)}</span></li>) : <li>数据、映射、参数和计划输出已通过当前检查。</li>}</ul>
    </div>
  );
}

function humanLabel(value: string) {
  return ({ dataScope: "数据范围", quality: "质量检查", mapping: "字段映射", parameters: "业务参数", plannedAnalysis: "计划分析", limitations: "限制", outputs: "输出", checks: "检查" } as Record<string, string>)[value] ?? value;
}

function humanItem(value: unknown): string {
  if (typeof value === "string" || typeof value === "number" || typeof value === "boolean") return String(value);
  if (Array.isArray(value)) return value.map(humanItem).join("、");
  if (value && typeof value === "object") {
    const record = value as Record<string, unknown>;
    const label = record.displayName ?? record.name ?? record.message ?? record.label;
    if (label !== undefined) return String(label);
    return Object.entries(record)
      .filter(([key]) => !/(^id$|hash|uri|path|ref|server|runtime|artifact)/i.test(key))
      .slice(0, 4)
      .map(([key, item]) => `${humanLabel(key)}: ${humanItem(item)}`)
      .join("；");
  }
  return "未提供";
}
