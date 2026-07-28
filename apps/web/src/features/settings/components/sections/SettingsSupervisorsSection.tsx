import { useEffect, useMemo, useState } from "react";
import type {
  AgentDefinitionSummary,
  SupervisorAgentSelection,
  SupervisorArtifactContractInput,
  SupervisorDraftRequest,
  SupervisorInstructionPolicySummary,
} from "../../../../../browser/types";
import {
  SettingsSection,
  SettingsSubsection,
} from "@/features/design-system/components/settings/SettingsPrimitives";
import type { SettingsSupervisorsSectionProps } from "@settings/hooks/useSettingsSupervisorsSection";
import {
  AgentStudioCreateButton,
  AgentStudioFieldHeading,
} from "./AgentStudioControls";

type SettingsSupervisorsSectionComponentProps =
  SettingsSupervisorsSectionProps & {
    studioMode?: boolean;
  };

function agentIdentity(
  agent: Pick<AgentDefinitionSummary, "definition_id" | "version">,
) {
  return `${agent.definition_id}@${agent.version}`;
}

function emptyDraft(
  instructionPolicy?: SupervisorInstructionPolicySummary,
): SupervisorDraftRequest {
  return {
    policy_id: "",
    version: "1.0.0",
    display_name: "",
    description: "",
    responsibilities: [],
    instruction_policy: {
      policy_id: instructionPolicy?.policy_id ?? "",
      version: instructionPolicy?.version ?? "",
    },
    custom_instructions: "",
    agents: [],
    artifact_contracts: [],
    max_active_child_agents: 2,
  };
}

function contractsForAgents(
  selections: SupervisorAgentSelection[],
  catalog: AgentDefinitionSummary[],
  current: SupervisorArtifactContractInput[] = [],
): SupervisorArtifactContractInput[] {
  const selected = catalog.filter((agent) =>
    selections.some(
      (selection) =>
        selection.definition_id === agent.definition_id &&
        selection.version === agent.version,
    ),
  );
  return selected.flatMap((producer) =>
    producer.output_artifact_types.map((artifactType) => {
      const consumers = selected
        .filter(
          (candidate) =>
            agentIdentity(candidate) !== agentIdentity(producer) &&
            candidate.input_artifact_types.includes(artifactType),
        )
        .map(agentIdentity);
      const previous = current.find(
        (contract) =>
          contract.artifact_type === artifactType &&
          contract.producer_agent === agentIdentity(producer),
      );
      const allowedConsumers = new Set(["supervisor", ...consumers]);
      const retainedConsumers =
        previous?.consumer_agents.filter((consumer) =>
          allowedConsumers.has(consumer),
        ) ?? [];
      const previousUsedSupervisorFallback =
        previous?.consumer_agents.length === 1 &&
        previous.consumer_agents[0] === "supervisor" &&
        consumers.length > 0;
      return {
        artifact_type: artifactType,
        producer_agent: agentIdentity(producer),
        consumer_agents: previousUsedSupervisorFallback
          ? consumers
          : retainedConsumers.length > 0
            ? retainedConsumers
            : consumers.length > 0
              ? consumers
              : ["supervisor"],
        required: previous?.required ?? true,
      };
    }),
  );
}

function compatibleConsumers(
  contract: SupervisorArtifactContractInput,
  selections: SupervisorAgentSelection[],
  catalog: AgentDefinitionSummary[],
) {
  const selected = catalog.filter((agent) =>
    selections.some(
      (selection) =>
        selection.definition_id === agent.definition_id &&
        selection.version === agent.version,
    ),
  );
  return [
    "supervisor",
    ...selected
      .filter(
        (agent) =>
          agentIdentity(agent) !== contract.producer_agent &&
          agent.input_artifact_types.includes(contract.artifact_type),
      )
      .map(agentIdentity),
  ];
}

export function SettingsSupervisorsSection({
  definitions,
  publishedPolicies,
  agents,
  instructionPolicies,
  instructionPolicyDetails,
  canPublishInstructionPolicy,
  isPublishingInstructionPolicy,
  isLoading,
  actionDefinitionId,
  loadingPolicyKey,
  error,
  validationByDefinition,
  detailByPolicy,
  onRefresh,
  onSaveDraft,
  onValidate,
  onPublish,
  onLoadPublished,
  onPublishInstructionPolicy,
  studioMode = false,
}: SettingsSupervisorsSectionComponentProps) {
  const [editingDefinitionId, setEditingDefinitionId] = useState<string | null>(
    null,
  );
  const [viewingPolicyKey, setViewingPolicyKey] = useState<string | null>(null);
  const [editorOpen, setEditorOpen] = useState(false);
  const [draft, setDraft] = useState<SupervisorDraftRequest>(() =>
    emptyDraft(instructionPolicies[0]),
  );
  const [responsibilitiesText, setResponsibilitiesText] = useState("");
  const [platformPolicyDraft, setPlatformPolicyDraft] = useState({
    policy_id: "platform-supervisor-behavior",
    version: "1.1.0",
    display_name: "Platform Supervisor behavior",
    description: "Platform boundaries and reliable orchestration behavior.",
    platform_instructions: "",
  });
  const selectedAgentIds = useMemo(
    () =>
      new Set(
        draft.agents.map((agent) => `${agent.definition_id}@${agent.version}`),
      ),
    [draft.agents],
  );

  useEffect(() => {
    if (!draft.instruction_policy.policy_id && instructionPolicies.length > 0) {
      setDraft((current) => ({
        ...current,
        instruction_policy: {
          policy_id: instructionPolicies[0].policy_id,
          version: instructionPolicies[0].version,
        },
      }));
    }
  }, [draft.instruction_policy.policy_id, instructionPolicies]);

  const resetEditor = () => {
    setEditingDefinitionId(null);
    setDraft(emptyDraft(instructionPolicies[0]));
    setResponsibilitiesText("");
    setEditorOpen(false);
  };

  const createDefinition = () => {
    setViewingPolicyKey(null);
    setEditingDefinitionId(null);
    setDraft(emptyDraft(instructionPolicies[0]));
    setResponsibilitiesText("");
    setEditorOpen(true);
  };

  const editDefinition = (
    definitionId: string,
    nextDraft: SupervisorDraftRequest,
  ) => {
    setEditingDefinitionId(definitionId);
    setDraft(nextDraft);
    setResponsibilitiesText(nextDraft.responsibilities.join("\n"));
    setViewingPolicyKey(null);
    setEditorOpen(true);
  };

  const updateAgentSelection = (
    agent: AgentDefinitionSummary,
    selected: boolean,
  ) => {
    setDraft((current) => {
      const nextAgents = selected
        ? [
            ...current.agents,
            {
              definition_id: agent.definition_id,
              version: agent.version,
              release_id: agent.release_id,
              spawn_limit: 1,
            },
          ]
        : current.agents.filter(
            (entry) =>
              entry.definition_id !== agent.definition_id ||
              entry.version !== agent.version,
          );
      return {
        ...current,
        agents: nextAgents,
        artifact_contracts: contractsForAgents(
          nextAgents,
          agents,
          current.artifact_contracts,
        ),
        max_active_child_agents: Math.max(
          1,
          Math.min(
            current.max_active_child_agents,
            nextAgents.reduce((total, entry) => total + entry.spawn_limit, 0) ||
              1,
          ),
        ),
      };
    });
  };

  const updateAgentSpawnLimit = (
    agent: AgentDefinitionSummary,
    spawnLimit: number,
  ) => {
    setDraft((current) => ({
      ...current,
      agents: current.agents.map((entry) =>
        entry.definition_id === agent.definition_id &&
        entry.version === agent.version
          ? {
              ...entry,
              spawn_limit: Math.max(1, Math.min(16, spawnLimit || 1)),
            }
          : entry,
      ),
    }));
  };

  const toggleDeliverable = (
    contract: SupervisorArtifactContractInput,
    checked: boolean,
  ) => {
    setDraft((current) => ({
      ...current,
      artifact_contracts: checked
        ? [...current.artifact_contracts, contract]
        : current.artifact_contracts.filter(
            (entry) =>
              entry.artifact_type !== contract.artifact_type ||
              entry.producer_agent !== contract.producer_agent,
          ),
    }));
  };

  const updateDeliverable = (
    contract: SupervisorArtifactContractInput,
    update: (
      current: SupervisorArtifactContractInput,
    ) => SupervisorArtifactContractInput,
  ) => {
    setDraft((current) => ({
      ...current,
      artifact_contracts: current.artifact_contracts.map((entry) =>
        entry.artifact_type === contract.artifact_type &&
        entry.producer_agent === contract.producer_agent
          ? update(entry)
          : entry,
      ),
    }));
  };

  const save = async () => {
    const responsibilities = responsibilitiesText
      .split("\n")
      .map((value) => value.trim())
      .filter(Boolean);
    const saved = await onSaveDraft(
      { ...draft, responsibilities },
      editingDefinitionId ?? undefined,
    );
    if (saved) resetEditor();
  };

  const availableContracts = contractsForAgents(
    draft.agents,
    agents,
    draft.artifact_contracts,
  );
  const saving = actionDefinitionId === (editingDefinitionId ?? "new");

  const viewPublished = (policy: (typeof publishedPolicies)[number]) => {
    const key = `${policy.policy_id}@${policy.version}`;
    if (!studioMode && viewingPolicyKey === key) {
      setViewingPolicyKey(null);
      return;
    }
    setViewingPolicyKey(key);
    setEditorOpen(false);
    if (!detailByPolicy[key]) {
      void onLoadPublished(policy);
    }
  };
  const visiblePublishedPolicies = studioMode && viewingPolicyKey
    ? publishedPolicies.filter(
        (policy) => `${policy.policy_id}@${policy.version}` === viewingPolicyKey,
      )
    : publishedPolicies;

  return (
    <SettingsSection
      title="Supervisor Studio"
      subtitle="Define responsibilities, select governed Agents, lock delivery contracts, and publish an immutable Supervisor version."
    >
      <div className="settings-help">
        Codex Runtime still owns Agent execution and Tool discovery. Publishing
        only succeeds when every selected Agent and Artifact handoff is valid.
      </div>

      {studioMode && (
        <div className="settings-agents-actions settings-studio-page-actions">
          {viewingPolicyKey ? (
            <button
              type="button"
              className="ghost"
              onClick={() => setViewingPolicyKey(null)}
            >
              Back to Supervisor directory
            </button>
          ) : editorOpen ? (
            <button type="button" className="ghost" onClick={resetEditor}>
              Back to Supervisor directory
            </button>
          ) : (
            <>
              <AgentStudioCreateButton onClick={createDefinition}>
                New Supervisor
              </AgentStudioCreateButton>
              <button type="button" className="ghost" onClick={onRefresh} disabled={isLoading}>
                {isLoading ? "Loading…" : "Refresh"}
              </button>
            </>
          )}
        </div>
      )}

      {(!studioMode || !editorOpen) && (
        <>
      {(!studioMode || !viewingPolicyKey) && (
        <>
      <SettingsSubsection
        title="Platform behavior contracts"
        subtitle="Platform Owners publish immutable instruction versions. Supervisor authors can select them but cannot edit their content."
      />
      {canPublishInstructionPolicy && !studioMode ? (
        <details className="settings-authoring-guide">
          <summary>Publish a platform contract version</summary>
          <div className="settings-field settings-supervisor-editor">
            <div className="settings-supervisor-grid">
              <label className="settings-label">
                Contract ID
                <input
                  className="settings-input"
                  value={platformPolicyDraft.policy_id}
                  onChange={(event) =>
                    setPlatformPolicyDraft((current) => ({
                      ...current,
                      policy_id: event.target.value,
                    }))
                  }
                />
              </label>
              <label className="settings-label">
                Version
                <input
                  className="settings-input"
                  value={platformPolicyDraft.version}
                  onChange={(event) =>
                    setPlatformPolicyDraft((current) => ({
                      ...current,
                      version: event.target.value,
                    }))
                  }
                />
              </label>
            </div>
            <label className="settings-label">
              Display name
              <input
                className="settings-input"
                value={platformPolicyDraft.display_name}
                onChange={(event) =>
                  setPlatformPolicyDraft((current) => ({
                    ...current,
                    display_name: event.target.value,
                  }))
                }
              />
            </label>
            <label className="settings-label">
              Description
              <textarea
                className="settings-agents-textarea settings-agents-textarea--compact"
                rows={2}
                value={platformPolicyDraft.description}
                onChange={(event) =>
                  setPlatformPolicyDraft((current) => ({
                    ...current,
                    description: event.target.value,
                  }))
                }
              />
            </label>
            <label className="settings-label">
              Platform instructions
              <textarea
                className="settings-agents-textarea"
                rows={10}
                value={platformPolicyDraft.platform_instructions}
                onChange={(event) =>
                  setPlatformPolicyDraft((current) => ({
                    ...current,
                    platform_instructions: event.target.value,
                  }))
                }
                placeholder="Define model-visible platform boundaries and reliable Supervisor behavior. Authorization and capabilities remain enforced by typed contracts."
              />
            </label>
            <button
              type="button"
              className="ghost"
              disabled={
                isPublishingInstructionPolicy ||
                !platformPolicyDraft.platform_instructions.trim()
              }
              onClick={() =>
                void onPublishInstructionPolicy(platformPolicyDraft)
              }
            >
              {isPublishingInstructionPolicy
                ? "Publishing…"
                : "Publish immutable version"}
            </button>
          </div>
        </details>
      ) : canPublishInstructionPolicy ? (
        <div className="settings-help">
          Platform contract publication remains in Settings so Supervisor
          browsing and authoring stay separate.
        </div>
      ) : (
        <div className="settings-help">
          Platform contracts are read-only for your account.
        </div>
      )}
      <div className="settings-supervisor-policy-list">
        {instructionPolicies.map((policy) => (
          <div key={`${policy.policy_id}@${policy.version}`}>
            <strong>{policy.display_name}</strong>
            <span>
              {policy.policy_id}@{policy.version}
              {" · "}
              {policy.source === "repository" ? "Built-in" : "Platform release"}
              {" · "}
              {policy.content_sha256.slice(0, 12)}…
            </span>
          </div>
        ))}
      </div>
        </>
      )}

      <SettingsSubsection
        title={viewingPolicyKey ? "Supervisor details" : "Supervisor directory"}
        subtitle={viewingPolicyKey
          ? "Review the exact immutable Supervisor definition and its execution contract."
          : "Browse built-in packages and organization releases before creating or editing a draft."}
      />
      {visiblePublishedPolicies.map((policy) => {
        const key = `${policy.policy_id}@${policy.version}`;
        const detail = detailByPolicy[key];
        const expanded = viewingPolicyKey === key;
        return (
          <article className="settings-supervisor-published" key={key}>
            <div className="settings-agent-card-header">
              <div>
                <strong>{policy.display_name}</strong>
                <div className="settings-help">
                  {policy.policy_id} · {policy.version}
                </div>
              </div>
              <div className="settings-agents-actions">
                <span className="settings-supervisor-source">
                  {policy.source === "repository"
                    ? "Built-in"
                    : "Organization release"}
                </span>
                <button
                  type="button"
                  className="ghost"
                  aria-expanded={expanded}
                  onClick={() => viewPublished(policy)}
                  disabled={loadingPolicyKey === key}
                >
                  {loadingPolicyKey === key
                    ? "Loading…"
                    : studioMode
                      ? "View details"
                    : expanded
                      ? "Close"
                      : "View"}
                </button>
              </div>
            </div>
            <p>{policy.description}</p>
            {expanded && detail && (
              <div className="settings-supervisor-detail">
                <div>
                  <strong>Responsibilities</strong>
                  <ul>
                    {detail.responsibilities.map((responsibility) => (
                      <li key={responsibility}>{responsibility}</li>
                    ))}
                  </ul>
                </div>
                <div>
                  <strong>Agents</strong>
                  <ul>
                    {detail.agents.map((agent) => (
                      <li key={`${agent.definition_id}@${agent.version}`}>
                        {agent.definition_id}@{agent.version}
                        {" · "}spawn limit {agent.spawn_limit}
                      </li>
                    ))}
                  </ul>
                </div>
                <div>
                  <strong>Artifact contracts</strong>
                  <ul>
                    {detail.artifact_contracts.map((contract) => (
                      <li
                        key={`${contract.producer_agent}:${contract.artifact_type}`}
                      >
                        {contract.artifact_type}: {contract.producer_agent}
                        {" → "}
                        {contract.consumer_agents.join(", ")}
                        {contract.required ? " · required" : " · optional"}
                      </li>
                    ))}
                  </ul>
                </div>
                <div>
                  <strong>
                    Platform behavior contract ·{" "}
                    {detail.instruction_policy.policy_id}@
                    {detail.instruction_policy.version}
                  </strong>
                  <pre>{detail.platform_instructions}</pre>
                </div>
                <div>
                  <strong>Custom Supervisor instructions</strong>
                  <pre>{detail.custom_instructions}</pre>
                </div>
                <div className="settings-help">
                  Maximum active child Agents: {detail.max_active_child_agents}
                  {" · "}Content: {detail.content_sha256.slice(0, 12)}…{" · "}
                  Execution: {detail.execution_semantics_sha256.slice(0, 12)}…
                </div>
              </div>
            )}
          </article>
        );
      })}
      {!isLoading && publishedPolicies.length === 0 && (
        <div className="settings-help">
          No published Supervisors are available.
        </div>
      )}
        </>
      )}

      {(!studioMode || editorOpen) && (
        <>
      <SettingsSubsection
        title={editingDefinitionId ? "Edit draft" : "Create Supervisor draft"}
        subtitle="A policy id is permanent. Published versions cannot be edited."
      />
      <div className="settings-field settings-supervisor-editor">
        <div className="settings-supervisor-grid">
          <label className="settings-label">
            <AgentStudioFieldHeading help="Use a stable lowercase identifier with hyphens. It becomes permanent after the draft is created.">
              Policy ID
            </AgentStudioFieldHeading>
            <input
              className="settings-input"
              aria-label="Policy ID"
              value={draft.policy_id}
              onChange={(event) =>
                setDraft((current) => ({
                  ...current,
                  policy_id: event.target.value,
                }))
              }
              placeholder="network-planning-supervisor"
              disabled={editingDefinitionId != null}
            />
          </label>
          <label className="settings-label">
            <AgentStudioFieldHeading help="Use the next semantic version. Published Supervisor definitions are immutable and resolved by exact version.">
              Version
            </AgentStudioFieldHeading>
            <input
              className="settings-input"
              aria-label="Version"
              value={draft.version}
              onChange={(event) =>
                setDraft((current) => ({
                  ...current,
                  version: event.target.value,
                }))
              }
              placeholder="1.0.0"
            />
          </label>
        </div>
        <label className="settings-label">
          <AgentStudioFieldHeading help="The concise name users see when choosing a Supervisor for a Workspace or Thread.">
            Display name
          </AgentStudioFieldHeading>
          <input
            className="settings-input"
            aria-label="Display name"
            value={draft.display_name}
            onChange={(event) =>
              setDraft((current) => ({
                ...current,
                display_name: event.target.value,
              }))
            }
            placeholder="Network Planning Supervisor"
          />
        </label>
        <label className="settings-label">
          <AgentStudioFieldHeading help="Describe the final business outcome this Supervisor is responsible for delivering.">
            Description
          </AgentStudioFieldHeading>
          <textarea
            className="settings-agents-textarea settings-agents-textarea--compact"
            aria-label="Description"
            rows={2}
            value={draft.description}
            onChange={(event) =>
              setDraft((current) => ({
                ...current,
                description: event.target.value,
              }))
            }
            placeholder="What this Supervisor delivers."
          />
        </label>
        <label className="settings-label">
          <AgentStudioFieldHeading help="Enter one orchestration responsibility per line, such as decomposition, evidence review, or final synthesis.">
            Responsibilities
          </AgentStudioFieldHeading>
          <textarea
            className="settings-agents-textarea settings-agents-textarea--compact"
            aria-label="Responsibilities"
            rows={4}
            value={responsibilitiesText}
            onChange={(event) => setResponsibilitiesText(event.target.value)}
            placeholder={
              "One responsibility per line\nCoordinate bounded Agent assignments"
            }
          />
        </label>
        <label className="settings-label">
          <AgentStudioFieldHeading help="Select the immutable platform governance contract. It defines boundaries that custom instructions cannot override.">
            Platform behavior contract
          </AgentStudioFieldHeading>
          <select
            className="settings-input"
            aria-label="Platform behavior contract"
            value={`${draft.instruction_policy.policy_id}@${draft.instruction_policy.version}`}
            onChange={(event) => {
              const selected = instructionPolicies.find(
                (policy) =>
                  `${policy.policy_id}@${policy.version}` ===
                  event.target.value,
              );
              if (selected) {
                setDraft((current) => ({
                  ...current,
                  instruction_policy: {
                    policy_id: selected.policy_id,
                    version: selected.version,
                  },
                }));
              }
            }}
          >
            {instructionPolicies.map((policy) => (
              <option
                key={`${policy.policy_id}@${policy.version}`}
                value={`${policy.policy_id}@${policy.version}`}
              >
                {policy.display_name} · {policy.version}
              </option>
            ))}
          </select>
        </label>
        {instructionPolicyDetails[
          `${draft.instruction_policy.policy_id}@${draft.instruction_policy.version}`
        ] && (
          <div className="settings-supervisor-platform-contract">
            <strong>Read-only platform instructions</strong>
            <p>
              Platform managers publish this contract independently. Supervisor
              authors select an exact version and cannot override it here.
            </p>
            <pre>
              {
                instructionPolicyDetails[
                  `${draft.instruction_policy.policy_id}@${draft.instruction_policy.version}`
                ].platform_instructions
              }
            </pre>
          </div>
        )}
        <label className="settings-label">
          <AgentStudioFieldHeading help="Define delegation order, evidence requirements, conflict handling, stop conditions, partial-result behavior, and final report structure.">
            Custom Supervisor instructions
          </AgentStudioFieldHeading>
          <textarea
            className="settings-agents-textarea"
            aria-label="Custom Supervisor instructions"
            rows={8}
            value={draft.custom_instructions}
            onChange={(event) =>
              setDraft((current) => ({
                ...current,
                custom_instructions: event.target.value,
              }))
            }
            placeholder="Define authority, delegation order, approval behavior, stop conditions, and final report requirements."
          />
        </label>

        <div className="settings-supervisor-picker-title">
          <AgentStudioFieldHeading help="Choose exact published Agents needed by this workflow. Their reviewed definitions supply all Runtime capabilities.">
            Allowed Agents
          </AgentStudioFieldHeading>
        </div>
        {agents.map((agent) => {
          const selected = selectedAgentIds.has(agentIdentity(agent));
          const selection = draft.agents.find(
            (entry) =>
              entry.definition_id === agent.definition_id &&
              entry.version === agent.version,
          );
          return (
            <div
              className="settings-supervisor-agent-choice"
              key={agentIdentity(agent)}
            >
              <label className="settings-supervisor-option">
                <input
                  type="checkbox"
                  checked={selected}
                  onChange={(event) =>
                    updateAgentSelection(agent, event.target.checked)
                  }
                />
                <span>
                  <strong>{agent.display_name}</strong>
                  <small>{agent.description}</small>
                  <small>
                    {agent.source === "user_release"
                      ? "Organization release"
                      : "Built-in release"}
                    {" · "}
                    {agent.version}
                  </small>
                  <small>
                    Capabilities: {agent.required_capabilities.join(", ")}
                  </small>
                </span>
              </label>
              {selection && (
                <label className="settings-supervisor-inline-number">
                  <AgentStudioFieldHeading help="Maximum concurrent instances of this exact Agent role within one Supervisor execution.">
                    Spawn limit
                  </AgentStudioFieldHeading>
                  <input
                    className="settings-input settings-input--compact"
                    type="number"
                    min={1}
                    max={16}
                    aria-label={`Spawn limit for ${agent.display_name}`}
                    value={selection.spawn_limit}
                    onChange={(event) =>
                      updateAgentSpawnLimit(agent, Number(event.target.value))
                    }
                  />
                </label>
              )}
            </div>
          );
        })}
        {!isLoading && agents.length === 0 && (
          <div className="settings-help">
            No published Agent Definitions are available.
          </div>
        )}

        {availableContracts.length > 0 && (
          <>
            <div className="settings-supervisor-picker-title">
              <AgentStudioFieldHeading help="Select typed Artifacts that must be handed from their declared producer to compatible consumers.">
                Required deliverables
              </AgentStudioFieldHeading>
            </div>
            {availableContracts.map((contract) => {
              const key = `${contract.producer_agent}:${contract.artifact_type}`;
              const checked = draft.artifact_contracts.some(
                (entry) =>
                  entry.producer_agent === contract.producer_agent &&
                  entry.artifact_type === contract.artifact_type,
              );
              const selectedContract = draft.artifact_contracts.find(
                (entry) =>
                  entry.producer_agent === contract.producer_agent &&
                  entry.artifact_type === contract.artifact_type,
              );
              const consumers = compatibleConsumers(
                contract,
                draft.agents,
                agents,
              );
              return (
                <div className="settings-supervisor-contract-choice" key={key}>
                  <label className="settings-supervisor-option">
                    <input
                      type="checkbox"
                      checked={checked}
                      onChange={(event) =>
                        toggleDeliverable(contract, event.target.checked)
                      }
                    />
                    <span>
                      <strong>{contract.artifact_type}</strong>
                      <small>{contract.producer_agent}</small>
                    </span>
                  </label>
                  {selectedContract && (
                    <div className="settings-supervisor-contract-editor">
                      <label>
                        <input
                          type="checkbox"
                          checked={selectedContract.required}
                          aria-label={`Required ${contract.artifact_type}`}
                          onChange={(event) =>
                            updateDeliverable(contract, (current) => ({
                              ...current,
                              required: event.target.checked,
                            }))
                          }
                        />
                        Required deliverable
                      </label>
                      <div>
                        <strong>Consumers</strong>
                        {consumers.map((consumer) => {
                          const selectedConsumer =
                            selectedContract.consumer_agents.includes(consumer);
                          return (
                            <label key={consumer}>
                              <input
                                type="checkbox"
                                checked={selectedConsumer}
                                aria-label={`Deliver ${contract.artifact_type} to ${consumer}`}
                                onChange={(event) =>
                                  updateDeliverable(contract, (current) => {
                                    const nextConsumers = event.target.checked
                                      ? [...current.consumer_agents, consumer]
                                      : current.consumer_agents.filter(
                                          (candidate) => candidate !== consumer,
                                        );
                                    return nextConsumers.length > 0
                                      ? {
                                          ...current,
                                          consumer_agents: nextConsumers,
                                        }
                                      : current;
                                  })
                                }
                              />
                              {consumer === "supervisor"
                                ? "Supervisor"
                                : consumer}
                            </label>
                          );
                        })}
                      </div>
                    </div>
                  )}
                </div>
              );
            })}
          </>
        )}

        <label className="settings-label">
          <AgentStudioFieldHeading help="Global concurrency limit across all child Agents. Keep it as small as the workflow dependency graph permits.">
            Maximum active child Agents
          </AgentStudioFieldHeading>
          <input
            className="settings-input settings-input--compact"
            aria-label="Maximum active child Agents"
            type="number"
            min={1}
            max={16}
            value={draft.max_active_child_agents}
            onChange={(event) =>
              setDraft((current) => ({
                ...current,
                max_active_child_agents: Number(event.target.value),
              }))
            }
          />
        </label>
        <div className="settings-agents-actions">
          <button
            type="button"
            className="ghost"
            onClick={() => void save()}
            disabled={saving}
          >
            {saving
              ? "Saving…"
              : editingDefinitionId
                ? "Save draft"
                : "Create draft"}
          </button>
          {editingDefinitionId && (
            <button
              type="button"
              className="ghost"
              onClick={resetEditor}
              disabled={saving}
            >
              Cancel
            </button>
          )}
        </div>
      </div>
        </>
      )}

      {(!studioMode || (!editorOpen && !viewingPolicyKey)) && (
        <>
      <SettingsSubsection
        title="Your Supervisor definitions"
        subtitle="Open an editable draft, create the next version, or inspect its published releases."
      />
      {!studioMode && <div className="settings-agents-actions">
        <button
          type="button"
          className="ghost"
          onClick={onRefresh}
          disabled={isLoading}
        >
          {isLoading ? "Loading…" : "Refresh"}
        </button>
      </div>}
      {!isLoading && definitions.length === 0 && (
        <div className="settings-help">No user-authored Supervisors yet.</div>
      )}
      {definitions.map((definition) => {
        const validation = validationByDefinition[definition.id];
        const busy = actionDefinitionId === definition.id;
        return (
          <div className="settings-agent-card" key={definition.id}>
            <div className="settings-agent-card-header">
              <div>
                <strong>{definition.display_name}</strong>
                <div className="settings-help">
                  {definition.policy_id}
                  {definition.draft
                    ? ` · draft ${definition.draft.version}`
                    : " · no draft"}
                </div>
              </div>
              <div className="settings-agents-actions">
                {definition.draft && (
                  <>
                    <button
                      type="button"
                      className="ghost"
                      onClick={() =>
                        editDefinition(definition.id, definition.draft!)
                      }
                      disabled={busy}
                    >
                      Edit
                    </button>
                    <button
                      type="button"
                      className="ghost"
                      onClick={() => void onValidate(definition.id)}
                      disabled={busy}
                    >
                      Validate
                    </button>
                    <button
                      type="button"
                      className="ghost"
                      onClick={() => void onPublish(definition.id)}
                      disabled={busy || validation?.valid !== true}
                      title={
                        validation?.valid
                          ? "Publish immutable release"
                          : "Validate first"
                      }
                    >
                      Publish
                    </button>
                  </>
                )}
                {!definition.draft && (
                  <button
                    type="button"
                    className="ghost"
                    onClick={() =>
                      editDefinition(definition.id, {
                        ...emptyDraft(),
                        policy_id: definition.policy_id,
                        display_name: definition.display_name,
                        description: definition.description,
                      })
                    }
                    disabled={busy}
                  >
                    New version
                  </button>
                )}
              </div>
            </div>
            <p>{definition.description}</p>
            {validation && (
              <div
                className={
                  validation.valid ? "settings-help" : "settings-agents-error"
                }
              >
                {validation.valid
                  ? `Validated · content ${validation.content_sha256?.slice(0, 12)}… · execution ${validation.execution_semantics_sha256?.slice(0, 12)}…`
                  : validation.issues.map((issue) => issue.message).join(" ")}
              </div>
            )}
            {definition.releases.map((release) => (
              <div className="settings-supervisor-release" key={release.id}>
                Published {release.version} ·{" "}
                {release.content_sha256.slice(0, 12)}…
              </div>
            ))}
          </div>
        );
      })}
        </>
      )}
      {error && <div className="settings-agents-error">{error}</div>}
    </SettingsSection>
  );
}
