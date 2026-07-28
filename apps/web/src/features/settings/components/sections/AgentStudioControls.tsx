import type { ReactNode } from "react";
import CircleHelp from "lucide-react/dist/esm/icons/circle-help";
import Info from "lucide-react/dist/esm/icons/info";
import Plus from "lucide-react/dist/esm/icons/plus";

export function AgentStudioCreateButton({
  children,
  disabled = false,
  onClick,
  title,
}: {
  children: ReactNode;
  disabled?: boolean;
  onClick?: () => void;
  title?: string;
}) {
  return (
    <button
      type="button"
      className="settings-studio-create-button"
      disabled={disabled}
      onClick={onClick}
      title={title}
    >
      <span className="settings-studio-create-button-icon" aria-hidden="true">
        <Plus size={14} strokeWidth={2.2} />
      </span>
      <span>{children}</span>
    </button>
  );
}

export function AgentStudioFieldHelp({
  label,
  children,
}: {
  label: string;
  children: ReactNode;
}) {
  return (
    <span className="settings-field-help">
      <span
        className="settings-field-help-trigger"
        tabIndex={0}
        aria-label={`Help for ${label}`}
        onClick={(event) => {
          event.preventDefault();
          event.stopPropagation();
          event.currentTarget.focus();
        }}
      >
        <CircleHelp size={13} strokeWidth={1.9} aria-hidden="true" />
      </span>
      <span className="settings-field-help-tooltip" role="tooltip">
        <strong>{label}</strong>
        <span>{children}</span>
      </span>
    </span>
  );
}

export function AgentStudioFieldHeading({
  children,
  help,
}: {
  children: ReactNode;
  help: ReactNode;
}) {
  return (
    <span className="settings-field-heading">
      <span>{children}</span>
      <AgentStudioFieldHelp label={String(children)}>
        {help}
      </AgentStudioFieldHelp>
    </span>
  );
}

export function AgentStudioDerivedNotice({
  title,
  children,
}: {
  title: string;
  children: ReactNode;
}) {
  return (
    <div className="settings-studio-derived-notice">
      <Info size={15} strokeWidth={2} aria-hidden="true" />
      <span>
        <strong>{title}</strong>
        <span>{children}</span>
      </span>
    </div>
  );
}

export function isAgentStudioIdentifier(
  value: string,
  options: { allowPeriod?: boolean; maximumBytes: number },
) {
  const { allowPeriod = false, maximumBytes } = options;
  if (
    value.length === 0
    || new TextEncoder().encode(value).length > maximumBytes
    || value === "."
    || value === ".."
    || value.includes("..")
    || !/^[a-z0-9]/.test(value)
    || !/[a-z0-9]$/.test(value)
  ) {
    return false;
  }
  return allowPeriod
    ? /^[a-z0-9._-]+$/.test(value)
    : /^[a-z0-9_-]+$/.test(value);
}

export function agentStudioUtf8ByteLength(value: string) {
  return new TextEncoder().encode(value).length;
}
