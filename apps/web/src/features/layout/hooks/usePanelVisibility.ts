import { useCallback } from "react";

type UsePanelVisibilityOptions = {
  isCompact: boolean;
  setActiveTab: (tab: "home" | "codex" | "git" | "log" | "projects") => void;
  setDebugOpen: (value: boolean | ((prev: boolean) => boolean)) => void;
};

export function usePanelVisibility({
  isCompact,
  setActiveTab,
  setDebugOpen,
}: UsePanelVisibilityOptions) {
  const onToggleDebug = useCallback(() => {
    if (isCompact) {
      setActiveTab("log");
      return;
    }
    setDebugOpen((prev) => !prev);
  }, [isCompact, setActiveTab, setDebugOpen]);

  return {
    onToggleDebug,
  };
}
