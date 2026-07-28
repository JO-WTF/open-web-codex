import { useEffect, useMemo, useState, type ReactNode } from "react";
import Bot from "lucide-react/dist/esm/icons/bot";
import CheckCircle2 from "lucide-react/dist/esm/icons/check-circle-2";
import Library from "lucide-react/dist/esm/icons/library";
import Network from "lucide-react/dist/esm/icons/network";
import Puzzle from "lucide-react/dist/esm/icons/puzzle";
import ServerCog from "lucide-react/dist/esm/icons/server-cog";
import X from "lucide-react/dist/esm/icons/x";
import { SettingsAgentCatalogSection } from "@/features/settings/components/sections/SettingsAgentCatalogSection";
import { AgentStudioCreateButton } from "@/features/settings/components/sections/AgentStudioControls";
import { SettingsSupervisorsSection } from "@/features/settings/components/sections/SettingsSupervisorsSection";
import { useSettingsAgentCatalogSection } from "@/features/settings/hooks/useSettingsAgentCatalogSection";
import { useSettingsSupervisorsSection } from "@/features/settings/hooks/useSettingsSupervisorsSection";
import { platformClient } from "../../../browser/session";
import type { CapabilityPackageSummary } from "../../../browser/types";

type McpServerEntry = {
  name: string;
  status: string;
  error?: string | null;
  failureReason?: string | null;
};

type AgentStudioSection = "agents" | "supervisors" | "mcp" | "capabilities";

type Props = {
  mcpServers: Record<string, McpServerEntry>;
  onClose: () => void;
  onSupervisorCatalogChanged: () => void;
};

const sections: Array<{
  id: AgentStudioSection;
  label: string;
  description: string;
  icon: typeof Bot;
}> = [
  {
    id: "agents",
    label: "Agents",
    description: "Directory, details, drafts, and releases",
    icon: Bot,
  },
  {
    id: "supervisors",
    label: "Supervisors",
    description: "Directory, teams, contracts, and drafts",
    icon: Network,
  },
  {
    id: "mcp",
    label: "MCP",
    description: "Available servers and current Thread status",
    icon: ServerCog,
  },
  {
    id: "capabilities",
    label: "Capabilities",
    description: "Reviewed capability package directory",
    icon: Puzzle,
  },
];

function CapabilityCard({
  title,
  status,
  children,
}: {
  title: string;
  status: string;
  children: ReactNode;
}) {
  return (
    <article className="web-agent-studio-capability">
      <div className="web-agent-studio-capability-heading">
        <h3>{title}</h3>
        <span>{status}</span>
      </div>
      <p>{children}</p>
    </article>
  );
}

export default function AgentStudioDialog({
  mcpServers,
  onClose,
  onSupervisorCatalogChanged,
}: Props) {
  const [activeSection, setActiveSection] = useState<AgentStudioSection>("agents");
  const agentCatalog = useSettingsAgentCatalogSection();
  const supervisors = useSettingsSupervisorsSection();
  const [capabilityPackages, setCapabilityPackages] = useState<
    CapabilityPackageSummary[]
  >([]);
  const [capabilityCatalogLoading, setCapabilityCatalogLoading] = useState(true);
  const [capabilityCatalogError, setCapabilityCatalogError] = useState<string | null>(null);
  const mcpDirectory = useMemo(
    () =>
      capabilityPackages.flatMap((capabilityPackage) =>
        capabilityPackage.mcp_server_names.map((serverName) => ({
          serverName,
          capabilityPackage,
          runtime: mcpServers[serverName] ?? null,
        })),
      ),
    [capabilityPackages, mcpServers],
  );

  useEffect(() => {
    let cancelled = false;
    setCapabilityCatalogLoading(true);
    void platformClient.listCapabilityPackages()
      .then((packages) => {
        if (!cancelled) {
          setCapabilityPackages(packages);
          setCapabilityCatalogError(null);
        }
      })
      .catch((error: unknown) => {
        if (!cancelled) {
          setCapabilityCatalogError(
            error instanceof Error ? error.message : "Unable to load capability packages.",
          );
        }
      })
      .finally(() => {
        if (!cancelled) setCapabilityCatalogLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const publishAgent = async (definitionId: string) => {
    const release = await agentCatalog.onPublish(definitionId);
    if (release) {
      supervisors.onRefresh();
    }
    return release;
  };

  const publishSupervisor = async (definitionId: string) => {
    const release = await supervisors.onPublish(definitionId);
    if (release) {
      onSupervisorCatalogChanged();
    }
    return release;
  };

  return (
    <section
      className="web-agent-studio-modal"
      role="dialog"
      aria-modal="true"
      aria-labelledby="web-agent-studio-title"
    >
      <header className="web-agent-studio-header">
        <div className="web-agent-studio-title">
          <span className="web-agent-studio-title-icon">
            <Library size={18} aria-hidden="true" />
          </span>
          <div>
            <h2 id="web-agent-studio-title">Agent Studio</h2>
            <p>Publish governed Agents and Supervisors without taking execution away from Codex Runtime.</p>
          </div>
        </div>
        <button
          type="button"
          className="web-settings-close"
          aria-label="Close Agent Studio"
          onClick={onClose}
        >
          <X size={17} />
        </button>
      </header>

      <div className="web-agent-studio-body">
        <nav className="web-agent-studio-nav" aria-label="Agent Studio sections">
          {sections.map((section) => {
            const Icon = section.icon;
            const active = section.id === activeSection;
            return (
              <button
                type="button"
                key={section.id}
                className={active ? "is-active" : undefined}
                aria-current={active ? "page" : undefined}
                onClick={() => setActiveSection(section.id)}
              >
                <Icon size={17} aria-hidden="true" />
                <span>
                  <strong>{section.label}</strong>
                  <small>{section.description}</small>
                </span>
              </button>
            );
          })}
          <div className="web-agent-studio-boundary">
            <CheckCircle2 size={15} aria-hidden="true" />
            <span>Published definitions are immutable and resolved by exact version.</span>
          </div>
        </nav>

        <div className="web-agent-studio-content">
          {activeSection === "agents" && (
            <SettingsAgentCatalogSection
              {...agentCatalog}
              onPublish={publishAgent}
              studioMode
            />
          )}
          {activeSection === "supervisors" && (
            <SettingsSupervisorsSection
              {...supervisors}
              onPublish={publishSupervisor}
              studioMode
            />
          )}
          {activeSection === "mcp" && (
            <section className="web-agent-studio-capabilities">
              <div className="settings-section-title">MCP servers</div>
              <div className="settings-section-subtitle">
                Browse platform-reviewed MCP declarations and distinguish them from the servers
                actually enabled in the selected Thread.
              </div>
              <div className="settings-agents-actions settings-studio-page-actions">
                <AgentStudioCreateButton
                  disabled
                  title="Safe MCP authoring and isolated validation are not available yet."
                >
                  New MCP server
                </AgentStudioCreateButton>
              </div>
              {capabilityCatalogLoading ? (
                <div className="web-agent-studio-empty">Loading MCP directory…</div>
              ) : capabilityCatalogError ? (
                <div className="settings-agents-error">{capabilityCatalogError}</div>
              ) : mcpDirectory.length > 0 ? (
                <div className="web-agent-studio-mcp-catalog">
                  {mcpDirectory.map(({ serverName, capabilityPackage, runtime }) => (
                    <article key={`${capabilityPackage.package_id}:${serverName}`}>
                      <div className="web-agent-studio-capability-heading">
                        <h3>{serverName}</h3>
                        <span className={runtime ? `is-${runtime.status}` : "is-inactive"}>
                          {runtime?.status ?? "Not active"}
                        </span>
                      </div>
                      <dl>
                        <div>
                          <dt>Capability package</dt>
                          <dd>{capabilityPackage.display_name}</dd>
                        </div>
                        <div>
                          <dt>Current Thread</dt>
                          <dd>{runtime?.status ?? "Not enabled in this Thread"}</dd>
                        </div>
                        {runtime?.failureReason && (
                          <div>
                            <dt>Failure reason</dt>
                            <dd>{runtime.failureReason}</dd>
                          </div>
                        )}
                        {runtime?.error && (
                          <div>
                            <dt>Error</dt>
                            <dd>{runtime.error}</dd>
                          </div>
                        )}
                      </dl>
                    </article>
                  ))}
                </div>
              ) : (
                <div className="web-agent-studio-empty">
                  <strong>No MCP server status is available for the selected Thread.</strong>
                  <p>
                    No reviewed MCP declarations are available.
                  </p>
                </div>
              )}
              <div className="web-agent-studio-boundary-note">
                “Not active” means the package exists but the selected Thread did not receive it.
                MCP editing remains unavailable until publication and isolated validation have a
                Web-safe typed contract.
              </div>
            </section>
          )}
          {activeSection === "capabilities" && (
            <section className="web-agent-studio-capabilities">
              <div className="settings-section-title">Runtime capabilities</div>
              <div className="settings-section-subtitle">
                This directory comes from checked-in, reviewed package declarations. Runtime status
                remains Thread-specific and is shown in MCP.
              </div>
              <div className="settings-agents-actions settings-studio-page-actions">
                <AgentStudioCreateButton
                  disabled
                  title="Capability package creation requires safe validation and publication."
                >
                  New capability package
                </AgentStudioCreateButton>
              </div>
              {capabilityCatalogLoading ? (
                <div className="web-agent-studio-empty">Loading capability directory…</div>
              ) : capabilityCatalogError ? (
                <div className="settings-agents-error">{capabilityCatalogError}</div>
              ) : (
                <div className="web-agent-studio-capability-directory">
                  {capabilityPackages.map((capabilityPackage) => (
                    <CapabilityCard
                      key={`${capabilityPackage.package_id}@${capabilityPackage.version}`}
                      title={capabilityPackage.display_name}
                      status="Built-in"
                    >
                      <span className="web-agent-studio-package-id">
                        {capabilityPackage.package_id}@{capabilityPackage.version}
                      </span>
                      <span>{capabilityPackage.description}</span>
                      <span>
                        Capabilities: {capabilityPackage.capabilities.join(", ")}
                      </span>
                      <span>
                        MCP: {capabilityPackage.mcp_server_names.join(", ")}
                        {capabilityPackage.includes_skills ? " · Includes Skills" : ""}
                      </span>
                    </CapabilityCard>
                  ))}
                </div>
              )}
              <div className="web-agent-studio-boundary-note">
                Package editing and creation are shown as unavailable until the platform can
                validate manifests, Skills, MCP launchers, permissions, and isolated execution
                without editing hidden Profile state.
              </div>
            </section>
          )}
        </div>
      </div>
    </section>
  );
}
