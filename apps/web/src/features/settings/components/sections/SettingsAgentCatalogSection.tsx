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
import {
  agentStudioUtf8ByteLength,
  AgentStudioCreateButton,
  AgentStudioDerivedNotice,
  AgentStudioFieldHeading,
  isAgentStudioIdentifier,
} from "./AgentStudioControls";

type SettingsAgentCatalogSectionComponentProps =
  SettingsAgentCatalogSectionProps & {
    studioMode?: boolean;
  };

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
  publishedAgents,
  templates,
  isLoading,
  actionDefinitionId,
  loadingAgentKey,
  error,
  validationByDefinition,
  detailByAgent,
  onRefresh,
  onSaveDraft,
  onValidate,
  onPublish,
  onLoadPublished,
  studioMode = false,
}: SettingsAgentCatalogSectionComponentProps) {
  const [editingDefinitionId, setEditingDefinitionId] = useState<string | null>(null);
  const [viewingAgentKey, setViewingAgentKey] = useState<string | null>(null);
  const [editorOpen, setEditorOpen] = useState(false);
  const [draft, setDraft] = useState<AgentDefinitionDraftRequest>(() =>
    emptyDraft(templates[0])
  );
  const [responsibilitiesText, setResponsibilitiesText] = useState("");
  const [showEditorValidation, setShowEditorValidation] = useState(false);
  const selectedTemplate = useMemo(
    () =>
      templates.find(
        (template) =>
          template.definition_id === draft.capability_template.definition_id
          && template.version === draft.capability_template.version,
      ) ?? null,
    [draft.capability_template, templates],
  );
  const responsibilities = responsibilitiesText
    .split("\n")
    .map((value) => value.trim())
    .filter(Boolean);
  const artifactSelectionIssue = (() => {
    if (!selectedTemplate) {
      return "Choose a reviewed capability template before configuring Artifact contracts.";
    }
    if (draft.output_artifact_types.length === 0) {
      return "Select at least one Artifact output. Every published Agent must declare what it can deliver.";
    }
    const allowedInputs = new Set(selectedTemplate.input_artifact_types);
    const allowedOutputs = new Set(selectedTemplate.output_artifact_types);
    if (
      draft.input_artifact_types.some((value) => !allowedInputs.has(value))
      || draft.output_artifact_types.some((value) => !allowedOutputs.has(value))
    ) {
      return "The selected Artifacts no longer match this capability template. Review the current template contracts.";
    }
    return null;
  })();
  const editorIssue = (() => {
    if (!isAgentStudioIdentifier(draft.definition_id, { maximumBytes: 96 })) {
      return "Enter a valid Agent ID using lowercase letters, numbers, hyphens, or underscores.";
    }
    if (
      !isAgentStudioIdentifier(draft.version, {
        allowPeriod: true,
        maximumBytes: 64,
      })
    ) {
      return "Enter a valid version using lowercase letters, numbers, periods, hyphens, or underscores.";
    }
    if (
      !draft.display_name.trim()
      || agentStudioUtf8ByteLength(draft.display_name) > 160
    ) {
      return "Enter a display name within the 160-byte platform limit.";
    }
    if (
      !draft.description.trim()
      || agentStudioUtf8ByteLength(draft.description) > 512
    ) {
      return "Enter a description within the 512-byte platform limit.";
    }
    if (
      responsibilities.length === 0
      || responsibilities.length > 32
      || responsibilities.some(
        (value) => agentStudioUtf8ByteLength(value) > 512,
      )
    ) {
      return "Enter 1–32 responsibilities, one per line and no more than 512 characters each.";
    }
    if (
      !draft.developer_instructions.trim()
      || agentStudioUtf8ByteLength(draft.developer_instructions) > 16 * 1024
      || draft.developer_instructions.includes("'''")
    ) {
      return "Enter custom Agent instructions within 16 KB; triple single quotes are not supported.";
    }
    return artifactSelectionIssue;
  })();

  const resetEditor = () => {
    setEditingDefinitionId(null);
    setDraft(emptyDraft(templates[0]));
    setResponsibilitiesText("");
    setShowEditorValidation(false);
    setEditorOpen(false);
  };

  const createDefinition = () => {
    setViewingAgentKey(null);
    setEditingDefinitionId(null);
    setDraft(emptyDraft(templates[0]));
    setResponsibilitiesText("");
    setShowEditorValidation(false);
    setEditorOpen(true);
  };

  const editDefinition = (
    definitionId: string,
    nextDraft: AgentDefinitionDraftRequest,
  ) => {
    setEditingDefinitionId(definitionId);
    setDraft(nextDraft);
    setResponsibilitiesText(nextDraft.responsibilities.join("\n"));
    setShowEditorValidation(false);
    setViewingAgentKey(null);
    setEditorOpen(true);
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
    setShowEditorValidation(true);
    if (editorIssue) return;
    const saved = await onSaveDraft(
      { ...draft, responsibilities },
      editingDefinitionId ?? undefined,
    );
    if (saved) resetEditor();
  };

  const saving = actionDefinitionId === (editingDefinitionId ?? "new");

  const viewPublished = (agent: AgentDefinitionSummary) => {
    const key = identity(agent);
    if (!studioMode && viewingAgentKey === key) {
      setViewingAgentKey(null);
      return;
    }
    setViewingAgentKey(key);
    setEditorOpen(false);
    if (!detailByAgent[key]) {
      void onLoadPublished(agent);
    }
  };
  const visiblePublishedAgents = studioMode && viewingAgentKey
    ? publishedAgents.filter((agent) => identity(agent) === viewingAgentKey)
    : publishedAgents;

  return (
    <SettingsSection
      title="Agent Catalog"
      subtitle="Create a governed Agent, bind a reviewed capability template, and publish an immutable version for Supervisors."
    >
      <div className="settings-help">
        The template fixes the available Tools and data access. Your Agent inherits those Runtime
        capabilities exactly and can narrow only its Artifact inputs and outputs.
      </div>

      {studioMode && (
        <div className="settings-agents-actions settings-studio-page-actions">
          {viewingAgentKey ? (
            <button
              type="button"
              className="ghost"
              onClick={() => setViewingAgentKey(null)}
            >
              Back to Agent directory
            </button>
          ) : editorOpen ? (
            <button type="button" className="ghost" onClick={resetEditor}>
              Back to Agent directory
            </button>
          ) : (
            <>
              <AgentStudioCreateButton onClick={createDefinition}>
                New Agent
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
          <SettingsSubsection
            title={viewingAgentKey ? "Agent details" : "Agent directory"}
            subtitle={viewingAgentKey
              ? "Review the exact immutable Agent definition and its execution contract."
              : "Browse built-in packages and organization releases before creating or editing a draft."}
          />
      {visiblePublishedAgents.map((agent) => {
        const key = identity(agent);
        const detail = detailByAgent[key];
        const expanded = viewingAgentKey === key;
        return (
          <article className="settings-supervisor-published" key={key}>
            <div className="settings-agent-card-header">
              <div>
                <strong>{agent.display_name}</strong>
                <div className="settings-help">{key}</div>
              </div>
              <div className="settings-agents-actions">
                <span className="settings-supervisor-source">
                  {agent.source === "repository" ? "Built-in" : "Organization release"}
                </span>
                <button
                  type="button"
                  className="ghost"
                  aria-expanded={expanded}
                  onClick={() => viewPublished(agent)}
                  disabled={loadingAgentKey === key}
                >
                  {loadingAgentKey === key ? "Loading…" : studioMode ? "View details" : expanded ? "Close" : "View"}
                </button>
              </div>
            </div>
            <p>{agent.description}</p>
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
                  <strong>Required capabilities</strong>
                  <ul>
                    {detail.required_capabilities.map((capability) => (
                      <li key={capability}>{capability}</li>
                    ))}
                  </ul>
                </div>
                <div className="settings-agent-artifact-grid">
                  <div>
                    <strong>Artifact inputs</strong>
                    {detail.input_artifact_types.length > 0 ? (
                      <ul>
                        {detail.input_artifact_types.map((artifact) => (
                          <li key={artifact}>{artifact}</li>
                        ))}
                      </ul>
                    ) : <div className="settings-help">None</div>}
                  </div>
                  <div>
                    <strong>Artifact outputs</strong>
                    <ul>
                      {detail.output_artifact_types.map((artifact) => (
                        <li key={artifact}>{artifact}</li>
                      ))}
                    </ul>
                  </div>
                </div>
                {detail.capability_template && (
                  <div className="settings-help">
                    Capability template: {identity(detail.capability_template)}
                  </div>
                )}
                <div>
                  <strong>Agent instructions</strong>
                  <pre>{detail.developer_instructions}</pre>
                </div>
                <div className="settings-help">
                  Content: {detail.content_sha256.slice(0, 12)}…
                  {" · "}Execution: {detail.execution_semantics_sha256.slice(0, 12)}…
                </div>
              </div>
            )}
          </article>
        );
      })}
      {!isLoading && publishedAgents.length === 0 && (
        <div className="settings-help">No published Agents are available.</div>
      )}
        </>
      )}

      {(!studioMode || editorOpen) && (
        <>
      <SettingsSubsection
        title={editingDefinitionId ? "Edit Agent draft" : "Create Agent draft"}
        subtitle="The Agent ID is permanent. Published versions cannot be edited."
      />
      <div className="settings-field settings-supervisor-editor">
        <div className="settings-supervisor-grid">
          <label className="settings-label">
            <AgentStudioFieldHeading help="Use a stable lowercase identifier with hyphens. It becomes permanent after the draft is created.">
              Agent ID
            </AgentStudioFieldHeading>
            <input
              className="settings-input"
              aria-label="Agent ID"
              aria-invalid={showEditorValidation && !draft.definition_id.trim()}
              value={draft.definition_id}
              onChange={(event) =>
                setDraft((current) => ({ ...current, definition_id: event.target.value }))
              }
              placeholder="regional-data-reviewer"
              disabled={editingDefinitionId != null}
              maxLength={96}
            />
          </label>
          <label className="settings-label">
            <AgentStudioFieldHeading help="Use the next semantic version for this immutable definition. Published versions are never overwritten.">
              Version
            </AgentStudioFieldHeading>
            <input
              className="settings-input"
              aria-label="Version"
              value={draft.version}
              onChange={(event) =>
                setDraft((current) => ({ ...current, version: event.target.value }))
              }
              placeholder="1.0.0"
              maxLength={64}
            />
          </label>
        </div>
        <label className="settings-label">
          <AgentStudioFieldHeading help="The concise human-readable name shown in directories and Supervisor selection.">
            Display name
          </AgentStudioFieldHeading>
          <input
            className="settings-input"
            aria-label="Display name"
            value={draft.display_name}
            onChange={(event) =>
              setDraft((current) => ({ ...current, display_name: event.target.value }))
            }
            placeholder="Regional Data Reviewer"
            maxLength={160}
          />
        </label>
        <label className="settings-label">
          <AgentStudioFieldHeading help="Describe the observable outcome this Agent owns in one or two sentences.">
            Description
          </AgentStudioFieldHeading>
          <textarea
            className="settings-agents-textarea settings-agents-textarea--compact"
            aria-label="Description"
            rows={2}
            value={draft.description}
            onChange={(event) =>
              setDraft((current) => ({ ...current, description: event.target.value }))
            }
            placeholder="What this Agent is responsible for delivering."
            maxLength={512}
          />
        </label>
        <label className="settings-label">
          <AgentStudioFieldHeading help="Enter one bounded responsibility per line. Avoid broad personas or department-level ownership.">
            Responsibilities
          </AgentStudioFieldHeading>
          <textarea
            className="settings-agents-textarea settings-agents-textarea--compact"
            aria-label="Responsibilities"
            rows={4}
            value={responsibilitiesText}
            onChange={(event) => setResponsibilitiesText(event.target.value)}
            placeholder={"One responsibility per line\nValidate regional planning inputs"}
          />
        </label>
        <label className="settings-label">
          <AgentStudioFieldHeading help="Define the method, required evidence, limits, stop conditions, and delivery format. Instructions cannot grant Tools or data access.">
            Custom Agent instructions
          </AgentStudioFieldHeading>
          <textarea
            className="settings-agents-textarea"
            aria-label="Custom Agent instructions"
            rows={8}
            value={draft.developer_instructions}
            onChange={(event) =>
              setDraft((current) => ({
                ...current,
                developer_instructions: event.target.value,
              }))
            }
            placeholder="Define the method, evidence requirements, limits, stop conditions, and delivery format."
            maxLength={16 * 1024}
          />
        </label>
        <label className="settings-label">
          <AgentStudioFieldHeading help="Select the reviewed Runtime capability boundary. The template fixes available Tools and data access.">
            Reviewed capability template
          </AgentStudioFieldHeading>
          <select
            className="settings-select"
            aria-label="Reviewed capability template"
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
            <div className="settings-supervisor-picker-title">
              <AgentStudioFieldHeading help="Artifact type IDs come from the reviewed template and cannot be entered manually. Keep only the inputs this Agent consumes and outputs it can actually deliver.">
                Artifact contracts
              </AgentStudioFieldHeading>
            </div>
            <AgentStudioDerivedNotice title={`Provided by ${selectedTemplate.display_name}`}>
              Type IDs are fixed by the reviewed template. Select the contracts
              this Agent actually uses; at least one output is required.
            </AgentStudioDerivedNotice>
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
            {showEditorValidation && artifactSelectionIssue && (
              <div className="settings-studio-validation" role="alert">
                {artifactSelectionIssue}
              </div>
            )}
          </>
        )}
        {showEditorValidation && !selectedTemplate && artifactSelectionIssue && (
          <div className="settings-studio-validation" role="alert">
            {artifactSelectionIssue}
          </div>
        )}
        {showEditorValidation && editorIssue && editorIssue !== artifactSelectionIssue && (
          <div className="settings-studio-validation" role="alert">
            {editorIssue}
          </div>
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
        </>
      )}

      {(!studioMode || (!editorOpen && !viewingAgentKey)) && (
        <>
      <SettingsSubsection
        title="Your Agent definitions"
        subtitle="Open an editable draft, create the next version, or inspect its published releases."
      />
      {!studioMode && <div className="settings-agents-actions">
        <button type="button" className="ghost" onClick={onRefresh} disabled={isLoading}>
          {isLoading ? "Loading…" : "Refresh"}
        </button>
      </div>}
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
                  ? `Validated · content ${validation.content_sha256?.slice(0, 12)}… · execution ${validation.execution_semantics_sha256?.slice(0, 12)}…`
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
        </>
      )}
      {error && <div className="settings-agents-error">{error}</div>}
    </SettingsSection>
  );
}
