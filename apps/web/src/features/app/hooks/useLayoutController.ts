import { useLayoutMode } from "../../layout/hooks/useLayoutMode";
import { useResizablePanels } from "../../layout/hooks/useResizablePanels";
import { useSidebarToggles } from "../../layout/hooks/useSidebarToggles";
import { usePanelVisibility } from "../../layout/hooks/usePanelVisibility";
import { usePanelShortcuts } from "../../layout/hooks/usePanelShortcuts";

export function useLayoutController({
  setActiveTab,
  setDebugOpen,
  toggleDebugPanelShortcut,
}: {
  setActiveTab: (tab: "home" | "projects" | "codex" | "git" | "log") => void;
  setDebugOpen: (value: boolean | ((prev: boolean) => boolean)) => void;
  toggleDebugPanelShortcut: string | null;
}) {
  const {
    appRef,
    isResizing,
    sidebarWidth,
    rightPanelWidth,
    chatDiffSplitPositionPercent,
    onSidebarResizeStart,
    onChatDiffSplitPositionResizeStart,
    onRightPanelResizeStart,
    planPanelHeight,
    onPlanPanelResizeStart,
    debugPanelHeight,
    onDebugPanelResizeStart,
  } = useResizablePanels();

  const layoutMode = useLayoutMode();
  const isCompact = layoutMode !== "desktop";
  const isTablet = layoutMode === "tablet";
  const isPhone = layoutMode === "phone";

  const {
    sidebarCollapsed,
    rightPanelCollapsed,
    collapseSidebar,
    expandSidebar,
    collapseRightPanel,
    expandRightPanel,
  } = useSidebarToggles({ isCompact });

  const {
    onToggleDebug: handleDebugClick,
  } = usePanelVisibility({
    isCompact,
    setActiveTab,
    setDebugOpen,
  });

  usePanelShortcuts({
    toggleDebugPanelShortcut,
    onToggleDebug: handleDebugClick,
  });

  return {
    appRef,
    isResizing,
    layoutMode,
    isCompact,
    isTablet,
    isPhone,
    sidebarWidth,
    rightPanelWidth,
    chatDiffSplitPositionPercent,
    planPanelHeight,
    debugPanelHeight,
    onSidebarResizeStart,
    onChatDiffSplitPositionResizeStart,
    onRightPanelResizeStart,
    onPlanPanelResizeStart,
    onDebugPanelResizeStart,
    sidebarCollapsed,
    rightPanelCollapsed,
    collapseSidebar,
    expandSidebar,
    collapseRightPanel,
    expandRightPanel,
    handleDebugClick,
  };
}
