import type { AppSettings } from "@/types";

type UseSettingsGitSectionArgs = {
  appSettings: AppSettings;
  onUpdateAppSettings: (next: AppSettings) => Promise<void>;
};

export type SettingsGitSectionProps = {
  appSettings: AppSettings;
  onUpdateAppSettings: (next: AppSettings) => Promise<void>;
};

export const useSettingsGitSection = ({
  appSettings,
  onUpdateAppSettings,
}: UseSettingsGitSectionArgs): SettingsGitSectionProps => {
  return {
    appSettings,
    onUpdateAppSettings,
  };
};
