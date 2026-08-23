import {
  ensureNamedImport,
  importPathToDesignSystem,
  normalizeClassTokens,
  runCodemod,
} from "./utils.mjs";

const DEFAULT_FILES = [
  "src/features/git/components/GitDiffPanel.tsx",
  "src/features/files/components/FileTreePanel.tsx",
];

const PANEL_PRIMITIVE_NAMES = [
  "PanelFrame",
  "PanelHeader",
  "PanelMeta",
  "PanelSearchField",
];

function resolvePanelDsClass(token) {
  if (token === "file-tree-meta") {
    return "ds-panel-meta";
  }
  if (token === "file-tree-search") {
    return "ds-panel-search";
  }
  if (token === "file-tree-search-icon") {
    return "ds-panel-search-icon";
  }
  if (token === "file-tree-search-input") {
    return "ds-panel-search-input";
  }
  return null;
}

runCodemod({
  name: "panel-shell-codemod",
  defaultFiles: DEFAULT_FILES,
  transform(source, filePath) {
    let updated = source;
    const usedPrimitives = PANEL_PRIMITIVE_NAMES.filter((name) =>
      updated.includes(`<${name}`),
    );
    if (usedPrimitives.length) {
      const importPath = importPathToDesignSystem(
        filePath,
        "src/features/design-system/components/panel/PanelPrimitives",
      );
      updated = ensureNamedImport(updated, importPath, usedPrimitives);
    }
    const alreadyUsesPanelSubPrimitives =
      updated.includes("<PanelMeta") || updated.includes("<PanelSearchField");
    if (!alreadyUsesPanelSubPrimitives) {
      updated = normalizeClassTokens(updated, resolvePanelDsClass);
    }
    return updated;
  },
});
