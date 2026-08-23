import { useEffect } from "react";
import { matchesShortcut } from "../../../utils/shortcuts";

type UsePanelShortcutsOptions = {
  toggleDebugPanelShortcut: string | null;
  onToggleDebug: () => void;
};

export function usePanelShortcuts({
  toggleDebugPanelShortcut,
  onToggleDebug,
}: UsePanelShortcutsOptions) {
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.repeat || event.defaultPrevented) {
        return;
      }
      const target = event.target;
      if (
        target instanceof HTMLElement &&
        (target.isContentEditable ||
          target.closest("input, textarea, select, [contenteditable='true']"))
      ) {
        return;
      }
      if (matchesShortcut(event, toggleDebugPanelShortcut)) {
        event.preventDefault();
        onToggleDebug();
        return;
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [onToggleDebug, toggleDebugPanelShortcut]);
}
