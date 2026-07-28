import type { ReactNode } from "react";
import CircleHelp from "lucide-react/dist/esm/icons/circle-help";
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
