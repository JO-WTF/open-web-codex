import ChevronLeft from "lucide-react/dist/esm/icons/chevron-left";
import X from "lucide-react/dist/esm/icons/x";
import type {
  AppSettings,
  CodexDoctorResult,
  CodexUpdateResult,
  DictationModelStatus,
  WorkspaceInfo,
} from "@/types";
import { useSettingsViewCloseShortcuts } from "@settings/hooks/useSettingsViewCloseShortcuts";
import { useSettingsViewNavigation } from "@settings/hooks/useSettingsViewNavigation";
import { useSettingsViewOrchestration } from "@settings/hooks/useSettingsViewOrchestration";
import { ModalShell } from "@/features/design-system/components/modal/ModalShell";
import { SettingsNav } from "./SettingsNav";
import type { CodexSection } from "./settingsTypes";
import { SETTINGS_SECTION_LABELS } from "./settingsViewConstants";
import { SettingsSectionContainers } from "./sections/SettingsSectionContainers";

export type SettingsViewProps = {
  workspaces: WorkspaceInfo[];
  onClose: () => void;
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
  initialSection?: CodexSection;
};

export function SettingsView({
  workspaces,
  onClose,
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
  initialSection,
}: SettingsViewProps) {
  const {
    activeSection,
    showMobileDetail,
    setShowMobileDetail,
    useMobileMasterDetail,
    handleSelectSection,
  } = useSettingsViewNavigation({ initialSection });

  const orchestration = useSettingsViewOrchestration({
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
  });

  useSettingsViewCloseShortcuts(onClose);

  const activeSectionLabel = SETTINGS_SECTION_LABELS[activeSection];
  const settingsBodyClassName = `settings-body${
    useMobileMasterDetail ? " settings-body-mobile-master-detail" : ""
  }${useMobileMasterDetail && showMobileDetail ? " is-detail-visible" : ""}`;

  return (
    <ModalShell
      className="settings-overlay"
      cardClassName="settings-window"
      onBackdropClick={onClose}
      ariaLabelledBy="settings-modal-title"
    >
      <div className="settings-titlebar">
        <div className="settings-title" id="settings-modal-title">
          Settings
        </div>
        <button
          type="button"
          className="ghost icon-button settings-close"
          onClick={onClose}
          aria-label="Close settings"
        >
          <X aria-hidden />
        </button>
      </div>
      <div className={settingsBodyClassName}>
        {(!useMobileMasterDetail || !showMobileDetail) && (
          <div className="settings-master">
            <SettingsNav
              activeSection={activeSection}
              onSelectSection={handleSelectSection}
              showDisclosure={useMobileMasterDetail}
            />
          </div>
        )}
        {(!useMobileMasterDetail || showMobileDetail) && (
          <div className="settings-detail">
            {useMobileMasterDetail && (
              <div className="settings-mobile-detail-header">
                <button
                  type="button"
                  className="settings-mobile-back"
                  onClick={() => setShowMobileDetail(false)}
                  aria-label="Back to settings sections"
                >
                  <ChevronLeft aria-hidden />
                  Sections
                </button>
                <div className="settings-mobile-detail-title">{activeSectionLabel}</div>
              </div>
            )}
            <div className="settings-content">
              <SettingsSectionContainers
                activeSection={activeSection}
                orchestration={orchestration}
              />
            </div>
          </div>
        )}
      </div>
    </ModalShell>
  );
}
