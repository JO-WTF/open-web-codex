import { useCallback, type Dispatch, type SetStateAction } from "react";
import type { AppSettings } from "@/types";
import { OPEN_APP_STORAGE_KEY } from "@app/constants";

type UseMainAppSettingsActionsArgs = {
  setAppSettings: Dispatch<SetStateAction<AppSettings>>;
  queueSaveSettings: (next: AppSettings) => Promise<unknown>;
};

export function useMainAppSettingsActions({
  setAppSettings,
  queueSaveSettings,
}: UseMainAppSettingsActionsArgs) {
  const handleSelectOpenAppId = useCallback(
    (id: string) => {
      if (typeof window !== "undefined") {
        window.localStorage.setItem(OPEN_APP_STORAGE_KEY, id);
      }
      setAppSettings((current) => {
        if (current.selectedOpenAppId === id) {
          return current;
        }
        const nextSettings = {
          ...current,
          selectedOpenAppId: id,
        };
        void queueSaveSettings(nextSettings);
        return nextSettings;
      });
    },
    [queueSaveSettings, setAppSettings],
  );

  const handleToggleAutomaticAppUpdateChecks = useCallback(() => {
    setAppSettings((current) => {
      const nextSettings = {
        ...current,
        automaticAppUpdateChecksEnabled: !current.automaticAppUpdateChecksEnabled,
      };
      void queueSaveSettings(nextSettings);
      return nextSettings;
    });
  }, [queueSaveSettings, setAppSettings]);

  return {
    handleSelectOpenAppId,
    handleToggleAutomaticAppUpdateChecks,
  };
}
