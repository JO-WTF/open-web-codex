import { useCallback, useEffect, useState } from "react";
import { platformClient } from "../../../../browser/session";
import type {
  AgentDefinitionSummary,
  SupervisorDefinitionSummary,
  SupervisorDraftRequest,
  SupervisorReleaseSummary,
  SupervisorValidationResult,
} from "../../../../browser/types";

export type SettingsSupervisorsSectionProps = {
  definitions: SupervisorDefinitionSummary[];
  agents: AgentDefinitionSummary[];
  isLoading: boolean;
  actionDefinitionId: string | null;
  error: string | null;
  validationByDefinition: Record<string, SupervisorValidationResult>;
  onRefresh: () => void;
  onSaveDraft: (
    draft: SupervisorDraftRequest,
    definitionId?: string,
  ) => Promise<SupervisorDefinitionSummary | null>;
  onValidate: (definitionId: string) => Promise<SupervisorValidationResult | null>;
  onPublish: (definitionId: string) => Promise<SupervisorReleaseSummary | null>;
};

function errorMessage(value: unknown, fallback: string) {
  return value instanceof Error && value.message.trim() ? value.message : fallback;
}

export function useSettingsSupervisorsSection(): SettingsSupervisorsSectionProps {
  const [definitions, setDefinitions] = useState<SupervisorDefinitionSummary[]>([]);
  const [agents, setAgents] = useState<AgentDefinitionSummary[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [actionDefinitionId, setActionDefinitionId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [validationByDefinition, setValidationByDefinition] = useState<
    Record<string, SupervisorValidationResult>
  >({});

  const refresh = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const [nextDefinitions, nextAgents] = await Promise.all([
        platformClient.listSupervisorDefinitions(),
        platformClient.listAgentDefinitions(),
      ]);
      setDefinitions(nextDefinitions);
      setAgents(nextAgents);
    } catch (value) {
      setError(errorMessage(value, "Unable to load Supervisor Studio."));
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const onSaveDraft = useCallback(
    async (draft: SupervisorDraftRequest, definitionId?: string) => {
      const actionId = definitionId ?? "new";
      setActionDefinitionId(actionId);
      setError(null);
      try {
        const saved = definitionId
          ? await platformClient.saveSupervisorDraft(definitionId, draft)
          : await platformClient.createSupervisorDefinition(draft);
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
        setError(errorMessage(value, "Unable to save Supervisor draft."));
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
      const validation = await platformClient.validateSupervisorDraft(definitionId);
      setValidationByDefinition((current) => ({
        ...current,
        [definitionId]: validation,
      }));
      return validation;
    } catch (value) {
      setError(errorMessage(value, "Unable to validate Supervisor draft."));
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
        const release = await platformClient.publishSupervisorDraft(definitionId);
        setValidationByDefinition((current) => {
          const next = { ...current };
          delete next[definitionId];
          return next;
        });
        await refresh();
        return release;
      } catch (value) {
        setError(errorMessage(value, "Unable to publish Supervisor draft."));
        return null;
      } finally {
        setActionDefinitionId(null);
      }
    },
    [refresh],
  );

  return {
    definitions,
    agents,
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
