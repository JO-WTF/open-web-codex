import { useRef, useEffect, useState } from "react";
import type { CSSProperties } from "react";
import type { ThreadTokenUsage } from "../../types";
import Bot from "lucide-react/dist/esm/icons/bot";
import ArrowUp from "lucide-react/dist/esm/icons/arrow-up";
import ChevronDown from "lucide-react/dist/esm/icons/chevron-down";
import CircleStop from "lucide-react/dist/esm/icons/circle-stop";
import Gauge from "lucide-react/dist/esm/icons/gauge";
import ImagePlus from "lucide-react/dist/esm/icons/image-plus";
import ShieldCheck from "lucide-react/dist/esm/icons/shield-check";

type Props = {
  draft: string;
  onDraftChange: (text: string) => void;
  onSend: () => void;
  onStop: () => void;
  running: boolean;
  stopping: boolean;
  busy: boolean;
  disabled: boolean;
  tokenUsage: ThreadTokenUsage | null;
  providers?: ModelProviderSummary[];
  currentProviderId?: string | null;
  models?: ModelSummary[];
  catalogLoading?: boolean;
  catalogError?: string | null;
  onRefreshCatalog?: () => void;
  onWriteProvider?: (input: Record<string, unknown>) => Promise<void>;
  selectedModelId?: string | null;
  onSelectModel?: (modelId: string) => void;
};

export type ModelProviderSummary = {
  id: string;
  name: string;
  kind: "builtIn" | "local" | "custom";
  isCurrent: boolean;
  modelCount: number;
  baseUrl?: string | null;
  envKey?: string | null;
  wireApi?: string;
  canEdit?: boolean;
  canDelete?: boolean;
  canFetchModels?: boolean;
  models?: { modelId: string; modelName?: string | null; contextWindow?: number | null }[];
};

export type ModelSummary = {
  id: string;
  displayName: string;
  model: string;
};

type ProviderCredentialMode = "environment" | "direct" | "preserve" | "none";

type ProviderDraft = {
  id: string;
  name: string;
  baseUrl: string;
  credentialMode: ProviderCredentialMode;
  envKey: string;
  apiKey: string;
  wireApi: string;
};

const EMPTY_PROVIDER_DRAFT: ProviderDraft = {
  id: "",
  name: "",
  baseUrl: "",
  credentialMode: "environment",
  envKey: "",
  apiKey: "",
  wireApi: "responses",
};

function formatTokens(value: number): string {
  return new Intl.NumberFormat("en-US").format(value);
}

export default function Composer({ draft, onDraftChange, onSend, onStop, running, stopping, busy, disabled, tokenUsage, providers = [], currentProviderId = null, models = [], catalogLoading = false, catalogError = null, onRefreshCatalog, onWriteProvider, selectedModelId = null, onSelectModel }: Props) {
  const textRef = useRef<HTMLTextAreaElement>(null);
  const catalogRef = useRef<HTMLDivElement>(null);
  const composingRef = useRef(false);
  const [catalogOpen, setCatalogOpen] = useState(false);
  const [editingProvider, setEditingProvider] = useState<ModelProviderSummary | "new" | null>(null);
  const [providerDraft, setProviderDraft] = useState<ProviderDraft>(EMPTY_PROVIDER_DRAFT);
  const [contextDrafts, setContextDrafts] = useState<Record<string, string>>({});
  const currentProvider = providers.find((provider) => provider.id === currentProviderId);
  const writeProvider = (input: Record<string, unknown>) => {
    void onWriteProvider?.(input).catch(() => undefined);
  };
  const providerDraftValid = Boolean(
    providerDraft.id.trim() &&
    providerDraft.name.trim() &&
    providerDraft.baseUrl.trim() &&
    (providerDraft.credentialMode !== "environment" || providerDraft.envKey.trim()) &&
    (providerDraft.credentialMode !== "direct" || providerDraft.apiKey.trim()),
  );
  const saveProviderDraft = () => {
    if (!providerDraftValid) return;
    void onWriteProvider?.({
      action: "upsert",
      ...providerDraft,
      select: editingProvider === "new",
    }).then(() => setEditingProvider(null)).catch(() => undefined);
  };

  const openProviderForm = (provider?: ModelProviderSummary) => {
    setEditingProvider(provider ?? "new");
    setProviderDraft(provider ? {
      id: provider.id,
      name: provider.name,
      baseUrl: provider.baseUrl ?? "",
      credentialMode: provider.envKey ? "environment" : "preserve",
      envKey: provider.envKey ?? "",
      apiKey: "",
      wireApi: provider.wireApi ?? "responses",
    } : EMPTY_PROVIDER_DRAFT);
  };

  useEffect(() => {
    if (!catalogOpen) return;
    const closeOnOutsideClick = (event: MouseEvent) => {
      if (!catalogRef.current?.contains(event.target as Node)) setCatalogOpen(false);
    };
    window.addEventListener("mousedown", closeOnOutsideClick);
    return () => window.removeEventListener("mousedown", closeOnOutsideClick);
  }, [catalogOpen]);

  useEffect(() => {
    if (textRef.current) {
      textRef.current.style.height = "auto";
      textRef.current.style.height = `${Math.min(textRef.current.scrollHeight, 160)}px`;
    }
  }, [draft]);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (running) {
      if (!stopping) onStop();
      return;
    }
    if (!disabled && !busy) onSend();
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (composingRef.current || e.nativeEvent.isComposing || e.nativeEvent.keyCode === 229) {
      return;
    }
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      if (running) {
        if (draft.trim()) onSend();
        return;
      }
      handleSubmit(e);
    }
  };

  const contextWindow = tokenUsage?.modelContextWindow ?? 0;
  const lastTokens = tokenUsage?.last.totalTokens ?? 0;
  const contextUsage = tokenUsage
    ? lastTokens > 0 ? tokenUsage.last : tokenUsage.total
    : null;
  const usedTokens = contextUsage?.totalTokens ?? 0;
  const usedPercent = contextWindow > 0
    ? Math.min(Math.max((usedTokens / contextWindow) * 100, 0), 100)
    : null;
  const roundedPercent = usedPercent === null ? null : Math.round(usedPercent);
  const contextColor = usedPercent === null
    ? "#687183"
    : usedPercent >= 90
      ? "#ef6464"
      : usedPercent >= 70
        ? "#e9a04a"
        : "#82e63e";
  const contextLabel = usedPercent === null
    ? "Context usage unavailable"
    : `Context used ${roundedPercent}%: ${formatTokens(usedTokens)} of ${formatTokens(contextWindow)} tokens`;

  return (
    <form className="web-composer" onSubmit={handleSubmit}>
      <div className="web-composer-main">
        <span className="web-composer-utility" title="Image attachments are not available in Web mode" aria-hidden="true">
          <ImagePlus size={17} />
        </span>
        <div className="web-composer-inner">
          <textarea
            ref={textRef}
            value={draft}
            onChange={(e) => onDraftChange(e.target.value)}
            onKeyDown={handleKeyDown}
            onCompositionStart={() => {
              composingRef.current = true;
            }}
            onCompositionEnd={() => {
              composingRef.current = false;
            }}
            placeholder={running ? "Request follow-up changes…" : "Ask Codex to do something..."}
            disabled={busy}
            rows={1}
          />
        </div>
        <button
          type={running ? "button" : "submit"}
          className={`web-send-btn${running ? " is-stop" : ""}`}
          disabled={running ? stopping : disabled || busy}
          aria-label={running ? stopping ? "Stopping" : "Stop" : "Send"}
          onClick={running ? onStop : undefined}
        >
          {running ? (
            <CircleStop size={18} aria-hidden="true" />
          ) : (
            <ArrowUp size={20} strokeWidth={2.4} aria-hidden="true" />
          )}
        </button>
      </div>
      <div className="web-composer-footer" aria-label="Thread settings summary">
        <div className="web-composer-catalog" ref={catalogRef}>
          <button
            className="web-composer-chip web-composer-chip-button"
            type="button"
            aria-expanded={catalogOpen}
            aria-haspopup="dialog"
            onClick={() => setCatalogOpen((open) => !open)}
          >
            <Bot size={13} />
            {providers.find((provider) => provider.id === currentProviderId)?.name ?? "Codex"}
            <ChevronDown size={12} />
          </button>
          {catalogOpen ? (
            <section className="web-model-catalog" role="dialog" aria-label="Providers and models">
              <header>
                <div><strong>Provider & model</strong><span>Managed by this Codex Profile</span></div>
                <span className="web-model-catalog-actions"><button type="button" onClick={() => openProviderForm()} disabled={catalogLoading}>Add</button><button type="button" onClick={onRefreshCatalog} disabled={catalogLoading}>Refresh</button></span>
              </header>
              {catalogError ? <p className="web-model-catalog-error">{catalogError}</p> : null}
              <div className="web-model-catalog-section">
                <h3>Providers</h3>
                {providers.length === 0 ? <p>{catalogLoading ? "Loading providers…" : "No providers available"}</p> : providers.map((provider) => (
                  <div className={`web-model-catalog-row${provider.isCurrent ? " is-current" : ""}`} key={provider.id}>
                    <span><strong>{provider.name}</strong><small>{provider.kind} · {provider.modelCount} models</small></span>
                    <span className="web-model-catalog-actions">
                      {provider.isCurrent ? <em>Current</em> : <button type="button" onClick={() => writeProvider({ action: "select", id: provider.id })}>Use</button>}
                      {provider.canEdit ? <button type="button" onClick={() => openProviderForm(provider)}>Edit</button> : null}
                      {provider.canFetchModels ? <button type="button" onClick={() => writeProvider({ action: "fetch", id: provider.id })}>Fetch</button> : null}
                      {provider.canEdit ? <button type="button" onClick={() => { setEditingProvider("new"); setProviderDraft({ id: `${provider.id}-copy`, name: `${provider.name} Copy`, baseUrl: provider.baseUrl ?? "", credentialMode: provider.envKey ? "environment" : "none", envKey: provider.envKey ?? "", apiKey: "", wireApi: provider.wireApi ?? "responses" }); }}>Copy</button> : null}
                      {provider.canDelete ? <button className="is-danger" type="button" onClick={() => { if (window.confirm(`Delete provider '${provider.name}'?`)) writeProvider({ action: "delete", id: provider.id }); }}>Delete</button> : null}
                    </span>
                  </div>
                ))}
              </div>
              {editingProvider ? (
                <div className="web-provider-form" role="group" aria-label="Provider settings">
                  <h3>{editingProvider === "new" ? "Add provider" : `Edit ${editingProvider.name}`}</h3>
                  <label>ID<input value={providerDraft.id} disabled={editingProvider !== "new" && providerDraft.id === editingProvider.id} onChange={(event) => setProviderDraft((draft) => ({ ...draft, id: event.target.value }))} placeholder="deepseek" required /></label>
                  <label>Name<input value={providerDraft.name} onChange={(event) => setProviderDraft((draft) => ({ ...draft, name: event.target.value }))} placeholder="DeepSeek" required /></label>
                  <label>Base URL<input value={providerDraft.baseUrl} onChange={(event) => setProviderDraft((draft) => ({ ...draft, baseUrl: event.target.value }))} placeholder="https://api.deepseek.com" type="url" required /></label>
                  <label>Credential source<select value={providerDraft.credentialMode} onChange={(event) => setProviderDraft((draft) => ({ ...draft, credentialMode: event.target.value as ProviderCredentialMode }))}>
                    {editingProvider !== "new" ? <option value="preserve">Keep current credential</option> : null}
                    <option value="environment">Environment variable</option>
                    <option value="direct">Direct API key</option>
                    <option value="none">No API key</option>
                  </select></label>
                  {providerDraft.credentialMode === "environment" ? <label>API key environment variable<input value={providerDraft.envKey} onChange={(event) => setProviderDraft((draft) => ({ ...draft, envKey: event.target.value.toUpperCase() }))} placeholder="DEEPSEEK_API_KEY" required /><small>Codex reads the secret from this variable when making requests.</small></label> : null}
                  {providerDraft.credentialMode === "direct" ? <label>API key<input value={providerDraft.apiKey} onChange={(event) => setProviderDraft((draft) => ({ ...draft, apiKey: event.target.value }))} placeholder="Paste API key" type="password" autoComplete="off" required /><small>Stored directly in the Profile config. Environment variables are safer when available.</small></label> : null}
                  <label>Wire API<select value={providerDraft.wireApi} onChange={(event) => setProviderDraft((draft) => ({ ...draft, wireApi: event.target.value }))}><option value="responses">Responses</option><option value="chat">Chat</option></select></label>
                  <div><button type="button" onClick={() => setEditingProvider(null)}>Cancel</button><button type="button" onClick={saveProviderDraft} disabled={catalogLoading || !providerDraftValid}>Save provider</button></div>
                </div>
              ) : null}
              <div className="web-model-catalog-section">
                <h3>Models</h3>
                {models.length === 0 ? <p>{catalogLoading ? "Loading models…" : "No models returned by this provider"}</p> : models.map((model) => (
                  <div className={`web-model-catalog-row web-model-catalog-model${selectedModelId === model.id ? " is-current" : ""}`} key={model.id}>
                    <button type="button" onClick={() => onSelectModel?.(model.id)}>
                    <span><strong>{model.displayName || model.model}</strong><small>{model.model}</small></span>
                    {selectedModelId === model.id ? <em>Selected</em> : null}
                    </button>
                    {currentProvider?.kind === "custom" ? <span className="web-context-editor"><input aria-label={`Context window for ${model.model}`} inputMode="numeric" value={contextDrafts[model.model] ?? String(currentProvider.models?.find((entry) => entry.modelId === model.model)?.contextWindow ?? "")} placeholder="128000" onChange={(event) => setContextDrafts((drafts) => ({ ...drafts, [model.model]: event.target.value.replace(/\D/g, "") }))} /><button type="button" onClick={() => writeProvider({ action: "context", id: currentProvider.id, modelId: model.model, contextWindow: Number(contextDrafts[model.model] ?? currentProvider.models?.find((entry) => entry.modelId === model.model)?.contextWindow) })}>Set context</button></span> : null}
                  </div>
                ))}
              </div>
              <footer>Credentials are stored in the Codex Profile configuration or read from its environment. Secret values are never returned by the provider catalog.</footer>
            </section>
          ) : null}
        </div>
        <span className="web-composer-chip"><Gauge size={13} />medium<ChevronDown size={12} /></span>
        <span className="web-composer-chip"><ShieldCheck size={13} />Workspace access<ChevronDown size={12} /></span>
        <span className="web-composer-activity" tabIndex={0} aria-label={contextLabel}>
          <span
            className="web-composer-activity-ring"
            style={{
              "--context-used": usedPercent ?? 0,
              "--context-color": contextColor,
            } as CSSProperties}
          />
          <span className="web-composer-context-tooltip" role="tooltip">
            <strong>{usedPercent === null ? "Context unavailable" : `Context used ${roundedPercent}%`}</strong>
            <span>
              {usedPercent === null
                ? "Waiting for token usage data"
                : `${formatTokens(usedTokens)} / ${formatTokens(contextWindow)} tokens`}
            </span>
            {contextUsage ? (
              <span>
                Input {formatTokens(contextUsage.inputTokens)} · Output {formatTokens(contextUsage.outputTokens)}
              </span>
            ) : null}
          </span>
        </span>
      </div>
    </form>
  );
}
