import { useEffect, useMemo, useState } from "react";
import { platformClient } from "../../../../../browser/session";
import type {
  CatalogDraftContent,
  CatalogResourceKind,
  CapabilityDraftDetail,
  CapabilityDraftSummary,
  CapabilityReleaseSummary,
  CapabilityReadinessSummary,
} from "../../../../../browser/types";
import { SettingsSection } from "@/features/design-system/components/settings/SettingsPrimitives";

type StudioKind = Extract<CatalogResourceKind, "tool_package" | "skill_package">;

const emptyDefinition = "{}";
const defaultSkill = `# 技能说明\n\n## 适用问题\n说明这个 Skill 解决什么业务问题。\n\n## 输入\n说明输入由谁提供，以及必须满足什么条件。\n\n## 工作方式\n说明应该调用哪些 Tool，如何处理失败和用户输入。\n\n## 交付件\n说明最终要发布哪些有界的结果或 Resource 引用。\n`;

const defaultServer = `from mcp.server.fastmcp import FastMCP\n\nmcp = FastMCP("my_tool")\n\n@mcp.tool()\ndef hello(name: str) -> dict:\n    """返回一个确定性的问候。"""\n    return {"message": f"你好，{name}"}\n\nif __name__ == "__main__":\n    mcp.run()\n`;

function defaultPlugin(resourceId: string) {
  return JSON.stringify(
    {
      name: resourceId || "my-tool",
      version: "0.1.0",
      description: "通过 Web 创建的 Python MCP Tool",
      mcpServers: "./.mcp.json",
    },
    null,
    2,
  );
}

function defaultMcp(resourceId: string) {
  return JSON.stringify(
    {
      mcpServers: {
        [resourceId || "my_tool"]: {
          command: "python3",
          args: ["./server.py"],
          cwd: ".",
          default_tools_approval_mode: "approve",
        },
      },
    },
    null,
    2,
  );
}

function makeContent(
  kind: StudioKind,
  pluginJson: string,
  mcpJson: string,
  serverSource: string,
  skillMarkdown: string,
  definitionText: string,
): CatalogDraftContent {
  let definition: unknown = {};
  try {
    definition = JSON.parse(definitionText || emptyDefinition) as unknown;
  } catch {
    definition = { "_invalid_json": true };
  }
  if (kind === "skill_package") {
    return {
      files: [{ path: "SKILL.md", content: skillMarkdown }],
      definition,
      dependencies: [],
    };
  }
  return {
    files: [
      { path: ".codex-plugin/plugin.json", content: pluginJson },
      { path: ".mcp.json", content: mcpJson },
      { path: "server.py", content: serverSource },
    ],
    definition,
    dependencies: [],
  };
}

function readFile(content: CatalogDraftContent, path: string) {
  return content.files.find((file) => file.path === path)?.content ?? "";
}

function errorText(value: unknown) {
  return value instanceof Error && value.message.trim()
    ? value.message
    : "操作失败，请查看服务端日志中的结构化错误。";
}

export function SettingsCapabilityCatalogSection() {
  const [drafts, setDrafts] = useState<CapabilityDraftSummary[]>([]);
  const [releases, setReleases] = useState<CapabilityReleaseSummary[]>([]);
  const [selected, setSelected] = useState<CapabilityDraftDetail | null>(null);
  const [workspaceId, setWorkspaceId] = useState<string | null>(null);
  const [kind, setKind] = useState<StudioKind>("skill_package");
  const [resourceId, setResourceId] = useState("my-skill");
  const [displayName, setDisplayName] = useState("我的 Skill");
  const [description, setDescription] = useState("描述这个能力解决的业务问题。");
  const [pluginJson, setPluginJson] = useState(defaultPlugin("my-tool"));
  const [mcpJson, setMcpJson] = useState(defaultMcp("my_tool"));
  const [serverSource, setServerSource] = useState(defaultServer);
  const [skillMarkdown, setSkillMarkdown] = useState(defaultSkill);
  const [definitionText, setDefinitionText] = useState(emptyDefinition);
  const [validation, setValidation] = useState<{ valid: boolean; issues: string[] } | null>(null);
  const [readiness, setReadiness] = useState<CapabilityReadinessSummary | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = async () => {
    setError(null);
    try {
      const [nextDrafts, nextReleases, workspaces] = await Promise.all([
        platformClient.listCapabilityDrafts(),
        platformClient.listCapabilityReleases(),
        platformClient.listWorkspaces(),
      ]);
      setDrafts(nextDrafts);
      setReleases(nextReleases);
      setWorkspaceId(
        workspaces.find((workspace) =>
          workspace.state === "ready" || workspace.state === "retained",
        )?.id ?? null,
      );
    } catch (value) {
      setError(errorText(value));
    }
  };

  useEffect(() => {
    void refresh();
  }, []);

  const content = useMemo(
    () => makeContent(
      kind,
      pluginJson,
      mcpJson,
      serverSource,
      skillMarkdown,
      definitionText,
    ),
    [kind, resourceId, pluginJson, mcpJson, serverSource, skillMarkdown, definitionText],
  );

  const selectedRelease = releases.find(
    (release) => release.kind === kind && release.resourceId === resourceId,
  ) ?? null;

  const loadDraft = async (id: string) => {
    setBusy(true);
    setError(null);
    try {
      const detail = await platformClient.getCapabilityDraft(id);
      setSelected(detail);
      setKind(detail.summary.kind as StudioKind);
      setResourceId(detail.summary.resourceId);
      setDisplayName(detail.summary.displayName);
      setDescription(detail.summary.description);
      setPluginJson(readFile(detail.content, ".codex-plugin/plugin.json"));
      setMcpJson(readFile(detail.content, ".mcp.json"));
      setServerSource(readFile(detail.content, "server.py"));
      setSkillMarkdown(readFile(detail.content, "SKILL.md"));
      setDefinitionText(JSON.stringify(detail.content.definition, null, 2));
      setValidation(null);
      setReadiness(null);
    } catch (value) {
      setError(errorText(value));
    } finally {
      setBusy(false);
    }
  };

  const createDraft = async () => {
    setBusy(true);
    setError(null);
    try {
      const detail = await platformClient.createCapabilityDraft({
        kind,
        resourceId,
        displayName,
        description,
        content,
      });
      setSelected(detail);
      await refresh();
    } catch (value) {
      setError(errorText(value));
    } finally {
      setBusy(false);
    }
  };

  const saveDraft = async () => {
    if (!selected) return;
    setBusy(true);
    setError(null);
    try {
      const detail = await platformClient.saveCapabilityDraft(selected.summary.id, {
        expectedRevision: selected.summary.metadata.revision,
        displayName,
        description,
        content,
      });
      setSelected(detail);
      setValidation(null);
      await refresh();
    } catch (value) {
      setError(errorText(value));
    } finally {
      setBusy(false);
    }
  };

  const validateDraft = async () => {
    if (!selected) return;
    setBusy(true);
    setError(null);
    try {
      const result = await platformClient.validateCapabilityDraft(selected.summary.id);
      setValidation(result);
      await refresh();
    } catch (value) {
      setError(errorText(value));
    } finally {
      setBusy(false);
    }
  };

  const publishDraft = async () => {
    if (!selected) return;
    setBusy(true);
    setError(null);
    try {
      await platformClient.publishCapabilityDraft(
        selected.summary.id,
        selected.summary.metadata.revision,
      );
      await refresh();
    } catch (value) {
      setError(errorText(value));
    } finally {
      setBusy(false);
    }
  };

  const installRelease = async () => {
    if (!selectedRelease || !workspaceId) return;
    setBusy(true);
    setError(null);
    try {
      await platformClient.installCapabilityRelease(selectedRelease.id, workspaceId);
      setReadiness(
        await platformClient.getCapabilityReadiness(selectedRelease.id, workspaceId),
      );
    } catch (value) {
      setError(errorText(value));
    } finally {
      setBusy(false);
    }
  };

  return (
    <SettingsSection
      title="Tool & Skill Studio"
      subtitle="在 Web 上创建中文 Skill 或 Python MCP Tool。Draft 使用整数修订，发布版本由服务器分配。"
    >
      <div className="settings-field">
        <div className="settings-field-label">已有 Draft</div>
        <div className="settings-help">
          {drafts.length === 0 ? "还没有 Draft。" : "选择一个 Draft 继续编辑。"}
        </div>
        {drafts.map((draft) => (
          <button
            type="button"
            className="ghost settings-button-compact"
            key={draft.id}
            onClick={() => void loadDraft(draft.id)}
          >
            {draft.displayName} · {draft.kind} · revision {draft.metadata.revision}
          </button>
        ))}
      </div>

      <div className="settings-field">
        <label className="settings-field-label" htmlFor="capability-kind">类型</label>
        <select
          id="capability-kind"
          className="settings-select"
          value={kind}
          onChange={(event) => {
            const next = event.target.value as StudioKind;
            setKind(next);
            setSelected(null);
            setResourceId(next === "skill_package" ? "my-skill" : "my-tool");
          }}
        >
          <option value="skill_package">Skill</option>
          <option value="tool_package">Python MCP Tool</option>
        </select>
      </div>

      <div className="settings-field">
        <label className="settings-field-label" htmlFor="capability-resource-id">标识</label>
        <input
          id="capability-resource-id"
          className="settings-input"
          value={resourceId}
          onChange={(event) => setResourceId(event.target.value)}
          placeholder="例如 stock-history"
        />
        <div className="settings-help">只能使用小写字母、数字、点、下划线和连字符。</div>
      </div>

      <div className="settings-field">
        <label className="settings-field-label" htmlFor="capability-display-name">显示名称</label>
        <input
          id="capability-display-name"
          className="settings-input"
          value={displayName}
          onChange={(event) => setDisplayName(event.target.value)}
        />
      </div>

      <div className="settings-field">
        <label className="settings-field-label" htmlFor="capability-description">说明</label>
        <input
          id="capability-description"
          className="settings-input"
          value={description}
          onChange={(event) => setDescription(event.target.value)}
        />
      </div>

      {kind === "skill_package" ? (
        <div className="settings-field">
          <label className="settings-field-label" htmlFor="capability-skill">SKILL.md（中文）</label>
          <textarea
            id="capability-skill"
            className="settings-input"
            rows={14}
            value={skillMarkdown}
            onChange={(event) => setSkillMarkdown(event.target.value)}
          />
        </div>
      ) : (
        <>
          <div className="settings-field">
            <label className="settings-field-label" htmlFor="capability-server">server.py</label>
            <textarea
              id="capability-server"
              className="settings-input"
              rows={14}
              value={serverSource}
              onChange={(event) => setServerSource(event.target.value)}
            />
          </div>
          <div className="settings-field">
            <label className="settings-field-label" htmlFor="capability-plugin">.codex-plugin/plugin.json</label>
            <textarea
              id="capability-plugin"
              className="settings-input"
              rows={8}
              value={pluginJson}
              onChange={(event) => setPluginJson(event.target.value)}
            />
          </div>
          <div className="settings-field">
            <label className="settings-field-label" htmlFor="capability-mcp">.mcp.json</label>
            <textarea
              id="capability-mcp"
              className="settings-input"
              rows={10}
              value={mcpJson}
              onChange={(event) => setMcpJson(event.target.value)}
            />
          </div>
        </>
      )}

      <div className="settings-field">
        <label className="settings-field-label" htmlFor="capability-definition">定义 JSON（可选）</label>
        <textarea
          id="capability-definition"
          className="settings-input"
          rows={5}
          value={definitionText}
          onChange={(event) => setDefinitionText(event.target.value)}
        />
      </div>

      <div className="settings-field">
        <button type="button" className="primary settings-button-compact" disabled={busy} onClick={() => void (selected ? saveDraft() : createDraft())}>
          {selected ? "保存 Draft" : "创建 Draft"}
        </button>{" "}
        {selected && (
          <>
            <button type="button" className="ghost settings-button-compact" disabled={busy} onClick={() => void validateDraft()}>
              校验
            </button>{" "}
            <button type="button" className="ghost settings-button-compact" disabled={busy} onClick={() => void publishDraft()}>
              发布
            </button>
          </>
        )}
        {selected && (
          <div className="settings-help">
            Draft revision {selected.summary.metadata.revision} · {selected.summary.metadata.validationState}
          </div>
        )}
      </div>

      {validation && (
        <div className="settings-help" role="status">
          {validation.valid ? "校验通过。" : `校验失败：${validation.issues.join("；")}`}
        </div>
      )}

      <div className="settings-field">
        <div className="settings-field-label">最近发布</div>
        {selectedRelease ? (
          <>
            <div className="settings-help">
              {selectedRelease.displayName} · release {selectedRelease.releaseVersion}
            </div>
            <button
              type="button"
              className="primary settings-button-compact"
              disabled={busy || workspaceId == null}
              onClick={() => void installRelease()}
            >
              {workspaceId ? "安装到当前 Workspace" : "没有可用 Workspace"}
            </button>
            {readiness && (
              <div className="settings-help">
                安装状态：{readiness.installation?.state ?? "unknown"}；
                {readiness.runtimeDiscovered
                  ? "Runtime 已发现"
                  : `Runtime 尚未发现：${readiness.missingCapabilities.join("、") || "等待探针"}`}
              </div>
            )}
          </>
        ) : (
          <div className="settings-help">当前 Draft 尚未发布 Release。</div>
        )}
      </div>

      {error && <div className="settings-help settings-error" role="alert">{error}</div>}
    </SettingsSection>
  );
}
