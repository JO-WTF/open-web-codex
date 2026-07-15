import { useState, type FormEvent } from "react";
import Bot from "lucide-react/dist/esm/icons/bot";
import ChevronDown from "lucide-react/dist/esm/icons/chevron-down";
import Loader2 from "lucide-react/dist/esm/icons/loader-2";
import Lock from "lucide-react/dist/esm/icons/lock";
import Mail from "lucide-react/dist/esm/icons/mail";
import Server from "lucide-react/dist/esm/icons/server";

type GatewayState = "checking" | "online" | "offline";

type Props = {
  baseUrl: string;
  onBaseUrlChange: (value: string) => void;
  email: string;
  onEmailChange: (value: string) => void;
  password: string;
  onPasswordChange: (value: string) => void;
  authError: string | null;
  busy: boolean;
  gatewayState: GatewayState;
  gatewayVersion: string | null;
  onLogin: () => void;
  onBootstrap: () => void;
};

function gatewayLabel(state: GatewayState, version: string | null) {
  if (state === "online") {
    return version ? `Platform online · v${version}` : "Platform online";
  }
  if (state === "offline") {
    return "Platform unreachable";
  }
  return "Checking platform…";
}

export default function PlatformAuthScreen({
  baseUrl,
  onBaseUrlChange,
  email,
  onEmailChange,
  password,
  onPasswordChange,
  authError,
  busy,
  gatewayState,
  gatewayVersion,
  onLogin,
  onBootstrap,
}: Props) {
  const [advancedOpen, setAdvancedOpen] = useState(false);

  const handleSubmit = (event: FormEvent) => {
    event.preventDefault();
    if (!busy) {
      onLogin();
    }
  };

  return (
    <div className="web-auth-shell">
      <div className="web-auth-glow" aria-hidden="true" />
      <div className="web-auth-frame">
        <header className="web-auth-brand">
          <div className="web-auth-mark" aria-hidden="true">
            <Bot size={22} strokeWidth={2.2} />
          </div>
          <div className="web-auth-brand-copy">
            <h1>open-web-codex</h1>
            <p>Browser workbench for multi-user Codex tasks.</p>
          </div>
          <span className={`web-auth-gateway is-${gatewayState}`}>
            <span className="web-auth-gateway-dot" aria-hidden="true" />
            {gatewayLabel(gatewayState, gatewayVersion)}
          </span>
        </header>

        <form className="web-auth-card" onSubmit={handleSubmit}>
          <div className="web-auth-card-head">
            <h2>Sign in</h2>
            <p>Use your platform account to open projects, tasks, and runs.</p>
          </div>

          <label className="web-auth-field">
            <span className="web-auth-label">Email</span>
            <span className="web-auth-input-wrap">
              <Mail size={16} aria-hidden="true" />
              <input
                value={email}
                onChange={(event) => onEmailChange(event.target.value)}
                autoComplete="username"
                placeholder="you@company.com"
                disabled={busy}
              />
            </span>
          </label>

          <label className="web-auth-field">
            <span className="web-auth-label">Password</span>
            <span className="web-auth-input-wrap">
              <Lock size={16} aria-hidden="true" />
              <input
                type="password"
                value={password}
                onChange={(event) => onPasswordChange(event.target.value)}
                autoComplete="current-password"
                placeholder="Enter your password"
                disabled={busy}
              />
            </span>
          </label>

          <div className="web-auth-advanced">
            <button
              type="button"
              className="web-auth-advanced-toggle"
              aria-expanded={advancedOpen}
              onClick={() => setAdvancedOpen((open) => !open)}
            >
              <Server size={14} aria-hidden="true" />
              <span>Server settings</span>
              <ChevronDown size={14} className={advancedOpen ? "is-open" : ""} aria-hidden="true" />
            </button>
            {advancedOpen ? (
              <label className="web-auth-field web-auth-field-advanced">
                <span className="web-auth-label">API base URL</span>
                <span className="web-auth-input-wrap">
                  <Server size={16} aria-hidden="true" />
                  <input
                    value={baseUrl}
                    onChange={(event) => onBaseUrlChange(event.target.value)}
                    placeholder="http://127.0.0.1:4800"
                    autoComplete="off"
                    spellCheck={false}
                    disabled={busy}
                  />
                </span>
              </label>
            ) : null}
          </div>

          {authError ? (
            <div className="web-auth-error" role="alert">
              {authError}
            </div>
          ) : null}

          <div className="web-auth-actions">
            <button type="submit" className="web-auth-primary" disabled={busy}>
              {busy ? <Loader2 size={16} className="web-auth-spinner" aria-hidden="true" /> : null}
              <span>{busy ? "Signing in…" : "Sign in"}</span>
            </button>
            <button
              type="button"
              className="web-auth-secondary"
              disabled={busy}
              onClick={() => onBootstrap()}
            >
              Bootstrap first user
            </button>
          </div>
        </form>

        <p className="web-auth-footnote">
          First install? Create the initial owner account, then sign in with the same credentials.
        </p>
      </div>
    </div>
  );
}
