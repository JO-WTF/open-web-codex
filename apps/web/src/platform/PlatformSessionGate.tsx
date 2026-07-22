import { useEffect, useState } from "react";
import type { FormEvent, ReactNode } from "react";
import { platformClient, getPlatformSessionToken, setPlatformSessionToken } from "./session";
import { startPlatformEventBridge } from "./browser/bridge";
import type { Me } from "./types";
import "./session-gate.css";

export default function PlatformSessionGate({ children }: { children: ReactNode }) {
  const [token, setToken] = useState(getPlatformSessionToken);
  const [me, setMe] = useState<Me | null>(null);
  const [checking, setChecking] = useState(Boolean(token));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [mode, setMode] = useState<"login" | "bootstrap">("login");

  useEffect(() => {
    if (!token) {
      setChecking(false);
      setMe(null);
      return;
    }
    let disposed = false;
    setChecking(true);
    platformClient.me().then((identity) => {
      if (!disposed) setMe(identity);
    }).catch((reason) => {
      if (disposed) return;
      setPlatformSessionToken("");
      setToken("");
      setError(reason instanceof Error ? reason.message : String(reason));
    }).finally(() => {
      if (!disposed) setChecking(false);
    });
    return () => { disposed = true; };
  }, [token]);

  useEffect(() => {
    if (!me) return;
    return startPlatformEventBridge();
  }, [me]);

  const authenticate = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const fields = new FormData(event.currentTarget);
    setBusy(true);
    setError(null);
    try {
      const email = String(fields.get("email") ?? "");
      const password = String(fields.get("password") ?? "");
      const session = mode === "bootstrap"
        ? await platformClient.bootstrap(String(fields.get("name") ?? ""), email, password)
        : await platformClient.login(email, password);
      setPlatformSessionToken(session.session_token);
      setToken(session.session_token);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  };

  if (me) return children;

  return (
    <main className="platform-session-gate">
      <section className="platform-session-card" aria-busy={checking || busy}>
        <p className="platform-session-eyebrow">OPEN WEB CODEX</p>
        <h1>Self-hosted Codex workbench</h1>
        <p>Sign in to your isolated Profile and authorized Git workspaces.</p>
        {error && <div className="platform-session-error" role="alert">{error}</div>}
        {checking ? <p>Checking session…</p> : (
          <form onSubmit={authenticate}>
            {mode === "bootstrap" && <input name="name" placeholder="Your name" required />}
            <input name="email" type="email" placeholder="Email" required />
            <input name="password" type="password" placeholder="Password" required />
            <button disabled={busy}>{mode === "login" ? "Sign in" : "Initialize instance"}</button>
          </form>
        )}
        {!checking && (
          <button className="platform-session-switch" type="button" onClick={() => setMode((value) => value === "login" ? "bootstrap" : "login")}>
            {mode === "login" ? "First run? Initialize the instance" : "Already initialized? Sign in"}
          </button>
        )}
      </section>
    </main>
  );
}
