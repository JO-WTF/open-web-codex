import { useMemo, useState } from "react";
import type {
  CopilotInstallationSummary,
  CopilotProfileStatus,
} from "../../../browser/types";

type Props = {
  status: CopilotProfileStatus | null;
  loading: boolean;
  error: string | null;
  onRefresh: () => Promise<void>;
  onActivate: (packageId: string) => Promise<void>;
  onDeactivate: (packageId: string) => Promise<void>;
};

const STATE_LABELS: Record<CopilotInstallationSummary["state"], string> = {
  installed: "Installed",
  configured: "Configured",
  unavailable: "Unavailable",
  failed: "Failed",
};

export default function CopilotManager({
  status,
  loading,
  error,
  onRefresh,
  onActivate,
  onDeactivate,
}: Props) {
  const [pendingPackageId, setPendingPackageId] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const actionInFlight = pendingPackageId !== null;
  const entries = useMemo(() => {
    const packages = new Map(
      (status?.packages ?? []).map((item) => [item.packageId, item]),
    );
    const installations = new Map(
      (status?.installations ?? []).map((item) => [item.packageId, item]),
    );
    const packageIds = new Set([...packages.keys(), ...installations.keys()]);
    return [...packageIds].sort().map((packageId) => ({
      packageId,
      available: packages.get(packageId)?.available ?? false,
      displayName: packages.get(packageId)?.displayName ?? packageId,
      installation: installations.get(packageId) ?? null,
    }));
  }, [status]);

  const changeDesiredState = async (packageId: string, active: boolean) => {
    setPendingPackageId(packageId);
    setActionError(null);
    try {
      if (active) await onActivate(packageId);
      else await onDeactivate(packageId);
    } catch (cause) {
      setActionError(cause instanceof Error ? cause.message : "Copilot update failed.");
    } finally {
      setPendingPackageId(null);
    }
  };

  return (
    <section className="web-copilot-manager" aria-label="Copilot installations">
      <div className="web-copilot-manager-header">
        <div>
          <h3>Copilots</h3>
          <p>Manage desired Profile activation for trusted packages.</p>
        </div>
        <button
          type="button"
          onClick={() => void onRefresh()}
          disabled={loading || actionInFlight}
        >
          Refresh
        </button>
      </div>
      <p className="web-copilot-manager-boundary">
        Source or configuration changes require an operator sync and a cold restart before the Runtime uses them.
      </p>
      {loading && !status ? <div role="status">Loading Copilots…</div> : null}
      {error ? <div className="web-copilot-manager-error" role="alert">{error}</div> : null}
      {actionError ? <div className="web-copilot-manager-error" role="alert">{actionError}</div> : null}
      {!loading && status && entries.length === 0 ? (
        <p className="web-copilot-manager-empty">No trusted Copilot packages are available.</p>
      ) : null}
      <div className="web-copilot-manager-list">
        {entries.map(({ packageId, available, displayName, installation }) => {
          const active = installation?.active ?? false;
          const pending = pendingPackageId === packageId;
          const state = installation?.state ?? (available ? "installed" : "unavailable");
          return (
            <article className="web-copilot-manager-card" key={packageId}>
              <div className="web-copilot-manager-title">
                <div>
                  <strong>{displayName}</strong>
                  <code>{packageId}</code>
                </div>
                <span className={`web-copilot-state is-${state}`}>{STATE_LABELS[state]}</span>
              </div>
              <dl className="web-copilot-manager-facts">
                <div><dt>Source</dt><dd>{available ? "Available" : "Unavailable"}</dd></div>
                <div><dt>Activation</dt><dd>{active ? "Active" : "Inactive"}</dd></div>
                <div>
                  <dt>Managed Skills</dt>
                  <dd>{installation?.managedSkillIds.length
                    ? installation.managedSkillIds.map((id) => <code key={id}>{id}</code>)
                    : "None"}</dd>
                </div>
                <div>
                  <dt>Managed Roles</dt>
                  <dd>{installation?.managedAgentRoleIds.length
                    ? installation.managedAgentRoleIds.map((id) => <code key={id}>{id}</code>)
                    : "None"}</dd>
                </div>
              </dl>
              {installation?.failureCode ? (
                <p className="web-copilot-failure">Failure code: <code>{installation.failureCode}</code></p>
              ) : null}
              {installation?.restartRequired ? (
                <p className="web-copilot-restart" role="status">
                  Cold restart required for this activation change.
                </p>
              ) : null}
              <div className="web-copilot-manager-actions">
                {active ? (
                  <button
                    type="button"
                    disabled={actionInFlight}
                    onClick={() => void changeDesiredState(packageId, false)}
                    aria-label={`Deactivate ${displayName}`}
                  >
                    {pending ? "Updating…" : "Deactivate"}
                  </button>
                ) : (
                  <button
                    type="button"
                    disabled={actionInFlight || !available}
                    onClick={() => void changeDesiredState(packageId, true)}
                    aria-label={`Activate ${displayName}`}
                  >
                    {pending ? "Updating…" : "Activate"}
                  </button>
                )}
              </div>
            </article>
          );
        })}
      </div>
    </section>
  );
}
