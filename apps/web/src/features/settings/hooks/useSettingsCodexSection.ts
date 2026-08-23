import { useEffect, useState } from "react";
import type { Dispatch, SetStateAction } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  AppSettings,
  CodexDoctorResult,
  CodexUpdateResult,
  WorkspaceInfo,
} from "@/types";
import { useSettingsDefaultModels } from "./useSettingsDefaultModels";
import { normalizeCodexArgsInput } from "@/utils/codexArgsInput";

type UseSettingsCodexSectionArgs = {
  appSettings: AppSettings;
  projects: WorkspaceInfo[];
  onUpdateAppSettings: (next: AppSettings) => Promise<void>;
  onRunDoctor: (
    codexBin: string | null,
    codexArgs: string | null,
  ) => Promise<CodexDoctorResult>;
  onRunCodexUpdate?: (
    codexBin: string | null,
    codexArgs: string | null,
  ) => Promise<CodexUpdateResult>;
};

export type SettingsCodexSectionProps = {
  appSettings: AppSettings;
  onUpdateAppSettings: (next: AppSettings) => Promise<void>;
  defaultModels: ReturnType<typeof useSettingsDefaultModels>["models"];
  defaultModelsLoading: boolean;
  defaultModelsError: string | null;
  defaultModelsConnectedWorkspaceCount: number;
  onRefreshDefaultModels: () => void;
  codexPathDraft: string;
  codexArgsDraft: string;
  codexDirty: boolean;
  isSavingSettings: boolean;
  doctorState: {
    status: "idle" | "running" | "done";
    result: CodexDoctorResult | null;
  };
  codexUpdateState: {
    status: "idle" | "running" | "done";
    result: CodexUpdateResult | null;
  };
  onSetCodexPathDraft: Dispatch<SetStateAction<string>>;
  onSetCodexArgsDraft: Dispatch<SetStateAction<string>>;
  onBrowseCodex: () => Promise<void>;
  onSaveCodexSettings: () => Promise<void>;
  onRunDoctor: () => Promise<void>;
  onRunCodexUpdate: () => Promise<void>;
};

export const useSettingsCodexSection = ({
  appSettings,
  projects,
  onUpdateAppSettings,
  onRunDoctor,
  onRunCodexUpdate,
}: UseSettingsCodexSectionArgs): SettingsCodexSectionProps => {
  const [codexPathDraft, setCodexPathDraft] = useState(appSettings.codexBin ?? "");
  const [codexArgsDraft, setCodexArgsDraft] = useState(appSettings.codexArgs ?? "");
  const [isSavingSettings, setIsSavingSettings] = useState(false);
  const [doctorState, setDoctorState] = useState<{
    status: "idle" | "running" | "done";
    result: CodexDoctorResult | null;
  }>({ status: "idle", result: null });
  const [codexUpdateState, setCodexUpdateState] = useState<{
    status: "idle" | "running" | "done";
    result: CodexUpdateResult | null;
  }>({ status: "idle", result: null });

  const {
    models: defaultModels,
    isLoading: defaultModelsLoading,
    error: defaultModelsError,
    connectedWorkspaceCount: defaultModelsConnectedWorkspaceCount,
    refresh: refreshDefaultModels,
  } = useSettingsDefaultModels(projects);

  useEffect(() => {
    setCodexPathDraft(appSettings.codexBin ?? "");
  }, [appSettings.codexBin]);

  useEffect(() => {
    setCodexArgsDraft(appSettings.codexArgs ?? "");
  }, [appSettings.codexArgs]);

  const nextCodexBin = codexPathDraft.trim() ? codexPathDraft.trim() : null;
  const nextCodexArgs = normalizeCodexArgsInput(codexArgsDraft);
  const codexDirty =
    nextCodexBin !== (appSettings.codexBin ?? null) ||
    nextCodexArgs !== (appSettings.codexArgs ?? null);

  const handleBrowseCodex = async () => {
    const selection = await open({ multiple: false, directory: false });
    if (!selection || Array.isArray(selection)) {
      return;
    }
    setCodexPathDraft(selection);
  };

  const handleSaveCodexSettings = async () => {
    setIsSavingSettings(true);
    try {
      await onUpdateAppSettings({
        ...appSettings,
        codexBin: nextCodexBin,
        codexArgs: nextCodexArgs,
      });
    } finally {
      setIsSavingSettings(false);
    }
  };

  const handleRunDoctor = async () => {
    setDoctorState({ status: "running", result: null });
    try {
      const result = await onRunDoctor(nextCodexBin, nextCodexArgs);
      setDoctorState({ status: "done", result });
    } catch (error) {
      setDoctorState({
        status: "done",
        result: {
          ok: false,
          codexBin: nextCodexBin,
          version: null,
          appServerOk: false,
          details: error instanceof Error ? error.message : String(error),
          path: null,
          nodeOk: false,
          nodeVersion: null,
          nodeDetails: null,
        },
      });
    }
  };

  const handleRunCodexUpdate = async () => {
    setCodexUpdateState({ status: "running", result: null });
    try {
      if (!onRunCodexUpdate) {
        setCodexUpdateState({
          status: "done",
          result: {
            ok: false,
            method: "unknown",
            package: null,
            beforeVersion: null,
            afterVersion: null,
            upgraded: false,
            output: null,
            details: "Codex updates are not available in this build.",
          },
        });
        return;
      }

      const result = await onRunCodexUpdate(nextCodexBin, nextCodexArgs);
      setCodexUpdateState({ status: "done", result });
    } catch (error) {
      setCodexUpdateState({
        status: "done",
        result: {
          ok: false,
          method: "unknown",
          package: null,
          beforeVersion: null,
          afterVersion: null,
          upgraded: false,
          output: null,
          details: error instanceof Error ? error.message : String(error),
        },
      });
    }
  };

  return {
    appSettings,
    onUpdateAppSettings,
    defaultModels,
    defaultModelsLoading,
    defaultModelsError,
    defaultModelsConnectedWorkspaceCount,
    onRefreshDefaultModels: () => {
      void refreshDefaultModels();
    },
    codexPathDraft,
    codexArgsDraft,
    codexDirty,
    isSavingSettings,
    doctorState,
    codexUpdateState,
    onSetCodexPathDraft: setCodexPathDraft,
    onSetCodexArgsDraft: setCodexArgsDraft,
    onBrowseCodex: handleBrowseCodex,
    onSaveCodexSettings: handleSaveCodexSettings,
    onRunDoctor: handleRunDoctor,
    onRunCodexUpdate: handleRunCodexUpdate,
  };
};
