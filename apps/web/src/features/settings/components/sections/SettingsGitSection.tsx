import type { AppSettings } from "@/types";
import {
  SettingsSection,
  SettingsToggleRow,
  SettingsToggleSwitch,
} from "@/features/design-system/components/settings/SettingsPrimitives";

type SettingsGitSectionProps = {
  appSettings: AppSettings;
  onUpdateAppSettings: (next: AppSettings) => Promise<void>;
};

export function SettingsGitSection({
  appSettings,
  onUpdateAppSettings,
}: SettingsGitSectionProps) {
  return (
    <SettingsSection
      title="Git"
      subtitle="Manage how diffs are loaded in the Git sidebar."
    >
      <SettingsToggleRow
        title="Preload git diffs"
        subtitle="Make viewing git diff faster."
      >
        <SettingsToggleSwitch
          pressed={appSettings.preloadGitDiffs}
          onClick={() =>
            void onUpdateAppSettings({
              ...appSettings,
              preloadGitDiffs: !appSettings.preloadGitDiffs,
            })
          }
        />
      </SettingsToggleRow>
      <SettingsToggleRow
        title="Ignore whitespace changes"
        subtitle="Hides whitespace-only changes in local and commit diffs."
      >
        <SettingsToggleSwitch
          pressed={appSettings.gitDiffIgnoreWhitespaceChanges}
          onClick={() =>
            void onUpdateAppSettings({
              ...appSettings,
              gitDiffIgnoreWhitespaceChanges: !appSettings.gitDiffIgnoreWhitespaceChanges,
            })
          }
        />
      </SettingsToggleRow>
    </SettingsSection>
  );
}
