import { useMemo, useState } from "react";
import type {
  AgentDefinitionDraftRequest,
  AgentDefinitionSummary,
} from "../../../../../browser/types";
import {
  SettingsSection,
  SettingsSubsection,
} from "@/features/design-system/components/settings/SettingsPrimitives";
import type { SettingsAgentCatalogSectionProps } from "@settings/hooks/useSettingsAgentCatalogSection";

function identity(agent: Pick<AgentDefinitionSummary, "definition_id" | "version">) {
  return `${agent.definition_id}@${agent.version}`;
}

function emptyDraft(template?: AgentDefinitionSummary): AgentDefinitionDraftRequest {
  return {
    definition_id: "",
    version: "1.0.0",
    display_name: "",
    description: "",
    responsibilities: [],
    developer_instructions: "",
    input_artifact_types: template?.input_artifact_types ?? [],
    output_artifact_types: template?.output_artifact_types ?? [],
    capability_template: {
      definition_id: template?.definition_id ?? "",
      version: template?.version ?? "",
    },
  };
}

export function SettingsAgentCatalogSection({
  definitions,
  templates,
  isLoading,
  actionDefinitionId,
  error,
  validationByDefinition,
  onRefresh,
  onSaveDraft,
  onValidate,
  onPublish,
}: SettingsAgentCatalogSectionProps) {
  const [editingDefinitionId, setEditingDefinitionId] = useState<string | null>(null);
  const [draft, setDraft] = useState<AgentDefinitionDraftRequest>(() =>
    emptyDraft(templates[0])
  );
  const [responsibilitiesText, setResponsibilitiesText] = useState("");
  const selectedTemplate = useMemo(
    () =>
      templates.find(
        (template) =>
          template.definition_id === draft.capability_template.definition_id
          && template.version === draft.capability_template.version,
      ) ?? null,
    [draft.capability_template, templates],
  );

  const resetEditor = () => {
    setEditingDefinitionId(null);
    setDraft(emptyDraft(templates[0]));
    setResponsibilitiesText("");
  };

  const editDefinition = (
    definitionId: string,
    nextDraft: AgentDefinitionDraftRequest,
  ) => {
    setEditingDefinitionId(definitionId);
    setDraft(nextDraft);
    setResponsibilitiesText(nextDraft.responsibilities.join("\n"));
  };

  const selectTemplate = (templateIdentity: string) => {
    const template = templates.find((candidate) => identity(candidate) === templateIdentity);
    if (!template) return;
    setDraft((current) => ({
      ...current,
      capability_template: {
        definition_id: template.definition_id,
        version: template.version,
      },
      input_artifact_types: template.input_artifact_types,
      output_artifact_types: template.output_artifact_types,
    }));
  };

  const toggleArtifact = (
    field: "input_artifact_types" | "output_artifact_types",
    artifactType: string,
    checked: boolean,
  ) => {
    setDraft((current) => ({
      ...current,
      [field]: checked
        ? [...current[field], artifactType]
        : current[field].filter((value) => value !== artifactType),
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

  const saving = actionDefinitionId === (editingDefinitionId ?? "new");

  return (
    <SettingsSection
      title="Agent Catalog"
      subtitle="Create a governed Agent, bind a reviewed capability template, and publish an immutable version for Supervisors."
    >
      <div className="settings-help">
        The template fixes the available Tools and data access. Your Agent inherits those Runtime
        capabilities exactly and can narrow only its Artifact inputs and outputs.
      </div>

      <SettingsSubsection
        title={editingDefinitionId ? "Edit Agent draft" : "Create Agent draft"}
        subtitle="The Agent ID is permanent. Published versions cannot be edited."
      />
      <div className="settings-field settings-supervisor-editor">
        <div className="settings-supervisor-grid">
          <label className="settings-label">
            Agent ID
            <input
              className="settings-input"
              value={draft.definition_id}
              onChange={(event) =>
                setDraft((current) => ({ ...current, definition_id: event.target.value }))
              }
              placeholder="regional-data-reviewer"
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
            placeholder="Regional Data Reviewer"
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
            placeholder="What this Agent is responsible for delivering."
          />
        </label>
        <label className="settings-label">
          Responsibilities
          <textarea
            className="settings-agents-textarea settings-agents-textarea--compact"
            rows={4}
            value={responsibilitiesText}
            onChange={(event) => setResponsibilitiesText(event.target.value)}
            placeholder={"One responsibility per line\nValidate regional planning inputs"}
          />
        </label>
        <label className="settings-label">
          Agent instructions
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
            placeholder="Define the method, evidence requirements, limits, stop conditions, and delivery format."
          />
        </label>
        <label className="settings-label">
          Reviewed capability template
          <select
            className="settings-select"
            value={
              draft.capability_template.definition_id
                ? `${draft.capability_template.definition_id}@${draft.capability_template.version}`
                : ""
            }
            onChange={(event) => selectTemplate(event.target.value)}
          >
            <option value="">Select a capability template</option>
            {templates.map((template) => (
              <option value={identity(template)} key={identity(template)}>
                {template.display_name} · {template.version}
              </option>
            ))}
          </select>
        </label>
        {selectedTemplate && (
          <>
            <div className="settings-supervisor-option">
              <span>
                <strong>Available capabilities</strong>
                <small>{selectedTemplate.required_capabilities.join(", ")}</small>
              </span>
            </div>
            <div className="settings-supervisor-picker-title">Artifact contracts</div>
            {selectedTemplate.input_artifact_types.map((artifactType) => (
              <label className="settings-supervisor-option" key={`input:${artifactType}`}>
                <input
                  type="checkbox"
                  checked={draft.input_artifact_types.includes(artifactType)}
                  onChange={(event) =>
                    toggleArtifact("input_artifact_types", artifactType, event.target.checked)
                  }
                />
                <span><strong>Input · {artifactType}</strong></span>
              </label>
            ))}
            {selectedTemplate.output_artifact_types.map((artifactType) => (
              <label className="settings-supervisor-option" key={`output:${artifactType}`}>
                <input
                  type="checkbox"
                  checked={draft.output_artifact_types.includes(artifactType)}
                  onChange={(event) =>
                    toggleArtifact("output_artifact_types", artifactType, event.target.checked)
                  }
                />
                <span><strong>Output · {artifactType}</strong></span>
              </label>
            ))}
          </>
        )}
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
        subtitle="Validate the current draft before publishing it for Supervisor use."
      />
      <div className="settings-agents-actions">
        <button type="button" className="ghost" onClick={onRefresh} disabled={isLoading}>
          {isLoading ? "Loading…" : "Refresh"}
        </button>
      </div>
      {!isLoading && definitions.length === 0 && (
        <div className="settings-help">No user-authored Agent Definitions yet.</div>
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
                  {definition.definition_id}
                  {definition.draft ? ` · draft ${definition.draft.version}` : " · no draft"}
                </div>
              </div>
              <div className="settings-agents-actions">
                {definition.draft ? (
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
                ) : (
                  <button
                    type="button"
                    className="ghost"
                    onClick={() =>
                      editDefinition(definition.id, {
                        ...emptyDraft(templates[0]),
                        definition_id: definition.definition_id,
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
