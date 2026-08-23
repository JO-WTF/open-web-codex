import { useMemo } from "react";
import type {
  AppSettings,
  CodexDoctorResult,
  CodexUpdateResult,
  DictationModelStatus,
  WorkspaceInfo,
} from "@/types";
import { isMacPlatform, isWindowsPlatform } from "@utils/platformPaths";
import { useSettingsOpenAppDrafts } from "./useSettingsOpenAppDrafts";
import { useSettingsShortcutDrafts } from "./useSettingsShortcutDrafts";
import { useSettingsCodexSection } from "./useSettingsCodexSection";
import { useSettingsDisplaySection } from "./useSettingsDisplaySection";
import { useSettingsFeaturesSection } from "./useSettingsFeaturesSection";
import { useSettingsGitSection } from "./useSettingsGitSection";
import { useSettingsServerSection } from "./useSettingsServerSection";
import {
  COMPOSER_PRESET_CONFIGS,
  COMPOSER_PRESET_LABELS,
  DICTATION_MODELS,
} from "@settings/components/settingsViewConstants";

type UseSettingsViewOrchestrationArgs = {
  workspaces: WorkspaceInfo[];
  reduceTransparency: boolean;
  onToggleTransparency: (value: boolean) => void;
  appSettings: AppSettings;
  openAppIconById: Record<string, string>;
  onUpdateAppSettings: (next: AppSettings) => Promise<void>;
  onToggleAutomaticAppUpdateChecks?: () => void;
  onRunDoctor: (
    codexBin: string | null,
    codexArgs: string | null,
  ) => Promise<CodexDoctorResult>;
  onRunCodexUpdate?: (
    codexBin: string | null,
    codexArgs: string | null,
  ) => Promise<CodexUpdateResult>;
  scaleShortcutTitle: string;
  scaleShortcutText: string;
  onTestNotificationSound: () => void;
  onTestSystemNotification: () => void;
  onMobileConnectSuccess?: () => Promise<void> | void;
  dictationModelStatus?: DictationModelStatus | null;
  onDownloadDictationModel?: () => void;
  onCancelDictationDownload?: () => void;
  onRemoveDictationModel?: () => void;
};

export function useSettingsViewOrchestration({
  workspaces,
  reduceTransparency,
  onToggleTransparency,
  appSettings,
  openAppIconById,
  onUpdateAppSettings,
  onToggleAutomaticAppUpdateChecks,
  onRunDoctor,
  onRunCodexUpdate,
  scaleShortcutTitle,
  scaleShortcutText,
  onTestNotificationSound,
  onTestSystemNotification,
  onMobileConnectSuccess,
  dictationModelStatus,
  onDownloadDictationModel,
  onCancelDictationDownload,
  onRemoveDictationModel,
}: UseSettingsViewOrchestrationArgs) {
  const featureWorkspaceId = useMemo(
    () => workspaces.find((workspace) => workspace.connected)?.id ?? null,
    [workspaces],
  );

  const optionKeyLabel = isMacPlatform() ? "Option" : "Alt";
  const metaKeyLabel = isMacPlatform()
    ? "Command"
    : isWindowsPlatform()
      ? "Windows"
      : "Meta";
  const followUpShortcutLabel = isMacPlatform()
    ? "Shift+Cmd+Enter"
    : "Shift+Ctrl+Enter";

  const selectedDictationModel = useMemo(() => {
    return (
      DICTATION_MODELS.find(
        (model) => model.id === appSettings.dictationModelId,
      ) ?? DICTATION_MODELS[1]
    );
  }, [appSettings.dictationModelId]);

  const dictationReady = dictationModelStatus?.state === "ready";

  const {
    openAppDrafts,
    openAppSelectedId,
    handleOpenAppDraftChange,
    handleOpenAppKindChange,
    handleCommitOpenAppsDrafts,
    handleMoveOpenApp,
    handleDeleteOpenApp,
    handleAddOpenApp,
    handleSelectOpenAppDefault,
  } = useSettingsOpenAppDrafts({
    appSettings,
    onUpdateAppSettings,
  });

  const { shortcutDrafts, handleShortcutKeyDown, clearShortcut } =
    useSettingsShortcutDrafts({
      appSettings,
      onUpdateAppSettings,
    });

  const displaySectionProps = useSettingsDisplaySection({
    appSettings,
    reduceTransparency,
    onToggleTransparency,
    onUpdateAppSettings,
    scaleShortcutTitle,
    scaleShortcutText,
    onTestNotificationSound,
    onTestSystemNotification,
  });

  const serverSectionProps = useSettingsServerSection({
    appSettings,
    onUpdateAppSettings,
    onMobileConnectSuccess,
  });

  const codexSectionProps = useSettingsCodexSection({
    appSettings,
    projects: workspaces,
    onUpdateAppSettings,
    onRunDoctor,
    onRunCodexUpdate,
  });

  const gitSectionProps = useSettingsGitSection({
    appSettings,
    onUpdateAppSettings,
  });

  const featuresSectionProps = useSettingsFeaturesSection({
    appSettings,
    featureWorkspaceId,
    onUpdateAppSettings,
  });

  return {
    aboutSectionProps: {
      appSettings,
      onToggleAutomaticAppUpdateChecks,
    },
    displaySectionProps,
    composerSectionProps: {
      appSettings,
      optionKeyLabel,
      followUpShortcutLabel,
      composerPresetLabels: COMPOSER_PRESET_LABELS,
      onComposerPresetChange: (
        preset: AppSettings["composerEditorPreset"],
      ) => {
        const config = COMPOSER_PRESET_CONFIGS[preset];
        void onUpdateAppSettings({
          ...appSettings,
          composerEditorPreset: preset,
          ...config,
        });
      },
      onUpdateAppSettings,
    },
    dictationSectionProps: {
      appSettings,
      optionKeyLabel,
      metaKeyLabel,
      dictationModels: DICTATION_MODELS,
      selectedDictationModel,
      dictationModelStatus,
      dictationReady,
      onUpdateAppSettings,
      onDownloadDictationModel,
      onCancelDictationDownload,
      onRemoveDictationModel,
    },
    shortcutsSectionProps: {
      shortcutDrafts,
      onShortcutKeyDown: handleShortcutKeyDown,
      onClearShortcut: clearShortcut,
    },
    openAppsSectionProps: {
      openAppDrafts,
      openAppSelectedId,
      openAppIconById,
      onOpenAppDraftChange: handleOpenAppDraftChange,
      onOpenAppKindChange: handleOpenAppKindChange,
      onCommitOpenApps: handleCommitOpenAppsDrafts,
      onMoveOpenApp: handleMoveOpenApp,
      onDeleteOpenApp: handleDeleteOpenApp,
      onAddOpenApp: handleAddOpenApp,
      onSelectOpenAppDefault: handleSelectOpenAppDefault,
    },
    gitSectionProps,
    serverSectionProps,
    codexSectionProps,
    featuresSectionProps,
  };
}

export type SettingsViewOrchestration = ReturnType<typeof useSettingsViewOrchestration>;
