import { useCallback, useEffect, useState } from "react";
import { platformClient } from "../../../../browser/session";
import type {
  AgentDefinitionSummary,
  SupervisorDefinitionSummary,
  SupervisorDraftRequest,
  SupervisorInstructionPolicyDetail,
  SupervisorInstructionPolicyPublishRequest,
  SupervisorInstructionPolicySummary,
  SupervisorPolicyDetail,
  SupervisorPolicySummary,
  SupervisorReleaseSummary,
  SupervisorValidationResult,
} from "../../../../browser/types";

export type SettingsSupervisorsSectionProps = {
  definitions: SupervisorDefinitionSummary[];
  publishedPolicies: SupervisorPolicySummary[];
  agents: AgentDefinitionSummary[];
  instructionPolicies: SupervisorInstructionPolicySummary[];
  instructionPolicyDetails: Record<string, SupervisorInstructionPolicyDetail>;
  canPublishInstructionPolicy: boolean;
  isPublishingInstructionPolicy: boolean;
  isLoading: boolean;
  actionDefinitionId: string | null;
  loadingPolicyKey: string | null;
  error: string | null;
  validationByDefinition: Record<string, SupervisorValidationResult>;
  detailByPolicy: Record<string, SupervisorPolicyDetail>;
  onRefresh: () => void;
  onSaveDraft: (
    draft: SupervisorDraftRequest,
    definitionId?: string,
    expectedRevision?: number,
  ) => Promise<SupervisorDefinitionSummary | null>;
  onValidate: (
    definitionId: string,
  ) => Promise<SupervisorValidationResult | null>;
  onPublish: (
    definitionId: string,
    expectedRevision?: number,
  ) => Promise<SupervisorReleaseSummary | null>;
  onLoadPublished: (
    policy: SupervisorPolicySummary,
  ) => Promise<SupervisorPolicyDetail | null>;
  onPublishInstructionPolicy: (
    request: SupervisorInstructionPolicyPublishRequest,
  ) => Promise<SupervisorInstructionPolicyDetail | null>;
};

function errorMessage(value: unknown, fallback: string) {
  return value instanceof Error && value.message.trim()
    ? value.message
    : fallback;
}

export function useSettingsSupervisorsSection(): SettingsSupervisorsSectionProps {
  const [definitions, setDefinitions] = useState<SupervisorDefinitionSummary[]>(
    [],
  );
  const [publishedPolicies, setPublishedPolicies] = useState<
    SupervisorPolicySummary[]
  >([]);
  const [agents, setAgents] = useState<AgentDefinitionSummary[]>([]);
  const [instructionPolicies, setInstructionPolicies] = useState<
    SupervisorInstructionPolicySummary[]
  >([]);
  const [instructionPolicyDetails, setInstructionPolicyDetails] = useState<
    Record<string, SupervisorInstructionPolicyDetail>
  >({});
  const [canPublishInstructionPolicy, setCanPublishInstructionPolicy] =
    useState(false);
  const [isPublishingInstructionPolicy, setIsPublishingInstructionPolicy] =
    useState(false);
  const [isLoading, setIsLoading] = useState(true);
  const [actionDefinitionId, setActionDefinitionId] = useState<string | null>(
    null,
  );
  const [loadingPolicyKey, setLoadingPolicyKey] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [detailByPolicy, setDetailByPolicy] = useState<
    Record<string, SupervisorPolicyDetail>
  >({});
  const [validationByDefinition, setValidationByDefinition] = useState<
    Record<string, SupervisorValidationResult>
  >({});

  const refresh = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const [
        nextDefinitions,
        nextPolicies,
        nextAgents,
        nextInstructionPolicies,
        currentUser,
      ] = await Promise.all([
        platformClient.listSupervisorDefinitions(),
        platformClient.listSupervisorPolicies(),
        platformClient.listAgentDefinitions(),
        platformClient.listSupervisorInstructionPolicies(),
        platformClient.me(),
      ]);
      const instructionDetails = await Promise.all(
        nextInstructionPolicies.map((policy) =>
          platformClient.getSupervisorInstructionPolicy(
            policy.policy_id,
            policy.version,
          ),
        ),
      );
      setDefinitions(nextDefinitions);
      setPublishedPolicies(nextPolicies);
      setAgents(nextAgents);
      setInstructionPolicies(nextInstructionPolicies);
      setCanPublishInstructionPolicy(currentUser.role === "owner");
      setInstructionPolicyDetails(
        Object.fromEntries(
          instructionDetails.map((detail) => [
            `${detail.policy_id}@${detail.version}`,
            detail,
          ]),
        ),
      );
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
    async (
      draft: SupervisorDraftRequest,
      definitionId?: string,
      expectedRevision?: number,
    ) => {
      const actionId = definitionId ?? "new";
      setActionDefinitionId(actionId);
      setError(null);
      try {
        const saved = definitionId
          ? await platformClient.saveSupervisorDraft(
              definitionId,
              draft,
              expectedRevision ?? 1,
            )
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
      const validation =
        await platformClient.validateSupervisorDraft(definitionId);
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
    async (definitionId: string, expectedRevision?: number) => {
      setActionDefinitionId(definitionId);
      setError(null);
      try {
        const release =
          await platformClient.publishSupervisorDraft(
            definitionId,
            expectedRevision ?? 1,
          );
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

  const onLoadPublished = useCallback(
    async (policy: SupervisorPolicySummary) => {
      const key = `${policy.policy_id}@${policy.version}`;
      setLoadingPolicyKey(key);
      setError(null);
      try {
        const detail = await platformClient.getSupervisorPolicy(
          policy.policy_id,
          policy.version,
        );
        setDetailByPolicy((current) => ({ ...current, [key]: detail }));
        return detail;
      } catch (value) {
        setError(
          errorMessage(value, "Unable to load the published Supervisor."),
        );
        return null;
      } finally {
        setLoadingPolicyKey(null);
      }
    },
    [],
  );

  const onPublishInstructionPolicy = useCallback(
    async (request: SupervisorInstructionPolicyPublishRequest) => {
      setIsPublishingInstructionPolicy(true);
      setError(null);
      try {
        const published =
          await platformClient.publishSupervisorInstructionPolicy(request);
        await refresh();
        return published;
      } catch (value) {
        setError(
          errorMessage(
            value,
            "Unable to publish the platform behavior contract.",
          ),
        );
        return null;
      } finally {
        setIsPublishingInstructionPolicy(false);
      }
    },
    [refresh],
  );

  return {
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
    onRefresh: refresh,
    onSaveDraft,
    onValidate,
    onPublish,
    onLoadPublished,
    onPublishInstructionPolicy,
  };
}
