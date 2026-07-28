import { useMemo, useState } from "react";
import type {
  AgentDefinitionSummary,
  SupervisorAgentSelection,
  SupervisorArtifactContractInput,
  SupervisorDraftRequest,
} from "../../../../../browser/types";
import {
  SettingsSection,
  SettingsSubsection,
} from "@/features/design-system/components/settings/SettingsPrimitives";
import type { SettingsSupervisorsSectionProps } from "@settings/hooks/useSettingsSupervisorsSection";

function agentIdentity(agent: Pick<AgentDefinitionSummary, "definition_id" | "version">) {
  return `${agent.definition_id}@${agent.version}`;
}

function emptyDraft(): SupervisorDraftRequest {
  return {
    policy_id: "",
    version: "1.0.0",
    display_name: "",
    description: "",
    responsibilities: [],
    developer_instructions: "",
    agents: [],
    artifact_contracts: [],
    max_active_child_agents: 2,
  };
}

function contractsForAgents(
  selections: SupervisorAgentSelection[],
  catalog: AgentDefinitionSummary[],
): SupervisorArtifactContractInput[] {
  const selected = catalog.filter((agent) =>
    selections.some(
      (selection) =>
        selection.definition_id === agent.definition_id
        && selection.version === agent.version,
    )
  );
  return selected.flatMap((producer) =>
    producer.output_artifact_types.map((artifactType) => {
      const consumers = selected
        .filter(
          (candidate) =>
            agentIdentity(candidate) !== agentIdentity(producer)
            && candidate.input_artifact_types.includes(artifactType),
        )
        .map(agentIdentity);
      return {
        artifact_type: artifactType,
        producer_agent: agentIdentity(producer),
        consumer_agents: consumers.length > 0 ? consumers : ["supervisor"],
        required: true,
      };
    })
  );
}

export function SettingsSupervisorsSection({
  definitions,
  agents,
  isLoading,
  actionDefinitionId,
  error,
  validationByDefinition,
  onRefresh,
  onSaveDraft,
  onValidate,
  onPublish,
}: SettingsSupervisorsSectionProps) {
  const [editingDefinitionId, setEditingDefinitionId] = useState<string | null>(null);
  const [draft, setDraft] = useState<SupervisorDraftRequest>(emptyDraft);
  const [responsibilitiesText, setResponsibilitiesText] = useState("");
  const selectedAgentIds = useMemo(
    () => new Set(draft.agents.map((agent) => `${agent.definition_id}@${agent.version}`)),
    [draft.agents],
  );

  const resetEditor = () => {
    setEditingDefinitionId(null);
    setDraft(emptyDraft());
    setResponsibilitiesText("");
  };

  const editDefinition = (
    definitionId: string,
    nextDraft: SupervisorDraftRequest,
  ) => {
    setEditingDefinitionId(definitionId);
    setDraft(nextDraft);
    setResponsibilitiesText(nextDraft.responsibilities.join("\n"));
  };

  const updateAgentSelection = (agent: AgentDefinitionSummary, selected: boolean) => {
    setDraft((current) => {
      const nextAgents = selected
        ? [
            ...current.agents,
            {
              definition_id: agent.definition_id,
              version: agent.version,
              spawn_limit: 1,
            },
          ]
        : current.agents.filter(
            (entry) =>
              entry.definition_id !== agent.definition_id || entry.version !== agent.version,
          );
      return {
        ...current,
        agents: nextAgents,
        artifact_contracts: contractsForAgents(nextAgents, agents),
        max_active_child_agents: Math.max(
          1,
          Math.min(current.max_active_child_agents, nextAgents.length || 1),
        ),
      };
    });
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
              entry.artifact_type !== contract.artifact_type
              || entry.producer_agent !== contract.producer_agent,
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

  const availableContracts = contractsForAgents(draft.agents, agents);
  const saving = actionDefinitionId === (editingDefinitionId ?? "new");

  return (
    <SettingsSection
      title="Supervisor Studio"
      subtitle="Define responsibilities, select governed Agents, lock delivery contracts, and publish an immutable Supervisor version."
    >
      <div className="settings-help">
        Codex Runtime still owns Agent execution and Tool discovery. Publishing only succeeds when
        every selected Agent and Artifact handoff is valid.
      </div>

      <SettingsSubsection
        title={editingDefinitionId ? "Edit draft" : "Create Supervisor draft"}
        subtitle="A policy id is permanent. Published versions cannot be edited."
      />
      <div className="settings-field settings-supervisor-editor">
        <div className="settings-supervisor-grid">
          <label className="settings-label">
            Policy ID
            <input
              className="settings-input"
              value={draft.policy_id}
              onChange={(event) =>
                setDraft((current) => ({ ...current, policy_id: event.target.value }))
              }
              placeholder="network-planning-supervisor"
              disabled={editingDefinitionId != null}
            />
          </label>
          <label className="settings-label">
            Version
            <input
              className="settings-input"
              value={draft.version}
              onChange={(event) =>
                setDraft((current) => ({ ...current, version: event.target.value }))
              }
              placeholder="1.0.0"
            />
          </label>
        </div>
        <label className="settings-label">
          Display name
          <input
            className="settings-input"
            value={draft.display_name}
            onChange={(event) =>
              setDraft((current) => ({ ...current, display_name: event.target.value }))
            }
            placeholder="Network Planning Supervisor"
          />
        </label>
        <label className="settings-label">
          Description
          <textarea
            className="settings-agents-textarea settings-agents-textarea--compact"
            rows={2}
            value={draft.description}
            onChange={(event) =>
              setDraft((current) => ({ ...current, description: event.target.value }))
            }
            placeholder="What this Supervisor delivers."
          />
        </label>
        <label className="settings-label">
          Responsibilities
          <textarea
            className="settings-agents-textarea settings-agents-textarea--compact"
            rows={4}
            value={responsibilitiesText}
            onChange={(event) => setResponsibilitiesText(event.target.value)}
            placeholder={"One responsibility per line\nCoordinate bounded Agent assignments"}
          />
        </label>
        <label className="settings-label">
          Supervisor instructions
          <textarea
            className="settings-agents-textarea"
            rows={8}
            value={draft.developer_instructions}
            onChange={(event) =>
              setDraft((current) => ({
                ...current,
                developer_instructions: event.target.value,
              }))
            }
            placeholder="Define authority, delegation order, approval behavior, stop conditions, and final report requirements."
          />
        </label>

        <div className="settings-supervisor-picker-title">Allowed Agents</div>
        {agents.map((agent) => (
          <label className="settings-supervisor-option" key={agentIdentity(agent)}>
            <input
              type="checkbox"
              checked={selectedAgentIds.has(agentIdentity(agent))}
              onChange={(event) => updateAgentSelection(agent, event.target.checked)}
            />
            <span>
              <strong>{agent.display_name}</strong>
              <small>{agent.description}</small>
              <small>Capabilities: {agent.required_capabilities.join(", ")}</small>
            </span>
          </label>
        ))}
        {!isLoading && agents.length === 0 && (
          <div className="settings-help">No published Agent Definitions are available.</div>
        )}

        {availableContracts.length > 0 && (
          <>
            <div className="settings-supervisor-picker-title">Required deliverables</div>
            {availableContracts.map((contract) => {
              const key = `${contract.producer_agent}:${contract.artifact_type}`;
              const checked = draft.artifact_contracts.some(
                (entry) =>
                  entry.producer_agent === contract.producer_agent
                  && entry.artifact_type === contract.artifact_type,
              );
              return (
                <label className="settings-supervisor-option" key={key}>
                  <input
                    type="checkbox"
                    checked={checked}
                    onChange={(event) => toggleDeliverable(contract, event.target.checked)}
                  />
                  <span>
                    <strong>{contract.artifact_type}</strong>
                    <small>
                      {contract.producer_agent} → {contract.consumer_agents.join(", ")}
                    </small>
                  </span>
                </label>
              );
            })}
          </>
        )}

        <label className="settings-label">
          Maximum active child Agents
          <input
            className="settings-input settings-input--compact"
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
          <button type="button" className="ghost" onClick={() => void save()} disabled={saving}>
            {saving ? "Saving…" : editingDefinitionId ? "Save draft" : "Create draft"}
          </button>
          {editingDefinitionId && (
            <button type="button" className="ghost" onClick={resetEditor} disabled={saving}>
              Cancel
            </button>
          )}
        </div>
      </div>

      <SettingsSubsection
        title="Definitions and releases"
        subtitle="Validate the current draft before publishing it to the Run catalog."
      />
      <div className="settings-agents-actions">
        <button type="button" className="ghost" onClick={onRefresh} disabled={isLoading}>
          {isLoading ? "Loading…" : "Refresh"}
        </button>
      </div>
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
                  {definition.draft ? ` · draft ${definition.draft.version}` : " · no draft"}
                </div>
              </div>
              <div className="settings-agents-actions">
                {definition.draft && (
                  <>
                    <button
                      type="button"
                      className="ghost"
                      onClick={() => editDefinition(definition.id, definition.draft!)}
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
                      title={validation?.valid ? "Publish immutable release" : "Validate first"}
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
              <div className={validation.valid ? "settings-help" : "settings-agents-error"}>
                {validation.valid
                  ? `Validated · ${validation.content_sha256?.slice(0, 12)}…`
                  : validation.issues.map((issue) => issue.message).join(" ")}
              </div>
            )}
            {definition.releases.map((release) => (
              <div className="settings-supervisor-release" key={release.id}>
                Published {release.version} · {release.content_sha256.slice(0, 12)}…
              </div>
            ))}
          </div>
        );
      })}
      {error && <div className="settings-agents-error">{error}</div>}
    </SettingsSection>
  );
}
