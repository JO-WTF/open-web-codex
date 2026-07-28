import { useCallback, useEffect, useState } from "react";
import { platformClient } from "../../../../browser/session";
import type {
  AgentDefinitionDraftRequest,
  AgentDefinitionReleaseSummary,
  AgentDefinitionResourceSummary,
  AgentDefinitionSummary,
  AgentDefinitionValidationResult,
} from "../../../../browser/types";

export type SettingsAgentCatalogSectionProps = {
  definitions: AgentDefinitionResourceSummary[];
  templates: AgentDefinitionSummary[];
  isLoading: boolean;
  actionDefinitionId: string | null;
  error: string | null;
  validationByDefinition: Record<string, AgentDefinitionValidationResult>;
  onRefresh: () => void;
  onSaveDraft: (
    draft: AgentDefinitionDraftRequest,
    definitionId?: string,
  ) => Promise<AgentDefinitionResourceSummary | null>;
  onValidate: (definitionId: string) => Promise<AgentDefinitionValidationResult | null>;
  onPublish: (definitionId: string) => Promise<AgentDefinitionReleaseSummary | null>;
};

function errorMessage(value: unknown, fallback: string) {
  return value instanceof Error && value.message.trim() ? value.message : fallback;
}

export function useSettingsAgentCatalogSection(): SettingsAgentCatalogSectionProps {
  const [definitions, setDefinitions] = useState<AgentDefinitionResourceSummary[]>([]);
  const [templates, setTemplates] = useState<AgentDefinitionSummary[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [actionDefinitionId, setActionDefinitionId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [validationByDefinition, setValidationByDefinition] = useState<
    Record<string, AgentDefinitionValidationResult>
  >({});

  const refresh = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const [nextDefinitions, catalog] = await Promise.all([
        platformClient.listAgentDefinitionResources(),
        platformClient.listAgentDefinitions(),
      ]);
      setDefinitions(nextDefinitions);
      setTemplates(catalog.filter((definition) => definition.source === "repository"));
    } catch (value) {
      setError(errorMessage(value, "Unable to load Agent Catalog."));
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const onSaveDraft = useCallback(
    async (draft: AgentDefinitionDraftRequest, definitionId?: string) => {
      const actionId = definitionId ?? "new";
      setActionDefinitionId(actionId);
      setError(null);
      try {
        const saved = definitionId
          ? await platformClient.saveAgentDefinitionDraft(definitionId, draft)
          : await platformClient.createAgentDefinition(draft);
        if (definitionId) {
          setValidationByDefinition((current) => {
            const next = { ...current };
            delete next[definitionId];
            return next;
          });
        }
        await refresh();
        return saved;
      } catch (value) {
        setError(errorMessage(value, "Unable to save Agent Definition draft."));
        return null;
      } finally {
        setActionDefinitionId(null);
      }
    },
    [refresh],
  );

  const onValidate = useCallback(async (definitionId: string) => {
    setActionDefinitionId(definitionId);
    setError(null);
    try {
      const validation = await platformClient.validateAgentDefinitionDraft(definitionId);
      setValidationByDefinition((current) => ({
        ...current,
        [definitionId]: validation,
      }));
      return validation;
    } catch (value) {
      setError(errorMessage(value, "Unable to validate Agent Definition draft."));
      return null;
    } finally {
      setActionDefinitionId(null);
    }
  }, []);

  const onPublish = useCallback(
    async (definitionId: string) => {
      setActionDefinitionId(definitionId);
      setError(null);
      try {
        const release = await platformClient.publishAgentDefinitionDraft(definitionId);
        setValidationByDefinition((current) => {
          const next = { ...current };
          delete next[definitionId];
          return next;
        });
        await refresh();
        return release;
      } catch (value) {
        setError(errorMessage(value, "Unable to publish Agent Definition."));
        return null;
      } finally {
        setActionDefinitionId(null);
      }
    },
    [refresh],
  );

  return {
    definitions,
    templates,
    isLoading,
    actionDefinitionId,
    error,
    validationByDefinition,
    onRefresh: refresh,
    onSaveDraft,
    onValidate,
    onPublish,
  };
}
