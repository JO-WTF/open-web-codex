import { useEffect, useMemo, useState } from "react";
import ListChecks from "lucide-react/dist/esm/icons/list-checks";
import type {
  McpFormContent,
  McpFormFieldSummary,
  McpFormResponseAction,
  PendingMcpFormSummary,
} from "../../../browser/types";

type FieldValue = string | boolean | string[] | undefined;

type Props = {
  requests: PendingMcpFormSummary[];
  submittingIds: Set<string>;
  onSubmit: (
    requestId: string,
    version: number,
    action: McpFormResponseAction,
    content?: McpFormContent,
  ) => Promise<void> | void;
};

function sourceLabel(request: PendingMcpFormSummary) {
  return request.source.kind === "agent"
    ? `${request.source.displayTitle} needs details`
    : "Supervisor needs details";
}

function initialFieldValue(field: McpFormFieldSummary): FieldValue {
  switch (field.schema.kind) {
    case "string":
    case "singleSelect":
      return field.schema.default ?? "";
    case "number":
    case "integer":
      return field.schema.default === null ? "" : String(field.schema.default);
    case "boolean":
      return field.schema.default ?? undefined;
    case "multiSelect":
      return field.schema.default === null ? undefined : [...field.schema.default];
  }
}

function initialValues(request: PendingMcpFormSummary) {
  return Object.fromEntries(
    request.fields.map((field) => [field.name, initialFieldValue(field)]),
  ) as Record<string, FieldValue>;
}

function fieldIsValid(field: McpFormFieldSummary, value: FieldValue | undefined) {
  switch (field.schema.kind) {
    case "string": {
      const text = typeof value === "string" ? value : "";
      if (!text) return !field.required;
      const length = Array.from(text).length;
      return (field.schema.minLength === null || length >= field.schema.minLength)
        && (field.schema.maxLength === null || length <= field.schema.maxLength);
    }
    case "number":
    case "integer": {
      if (value === "" || value === undefined) return !field.required;
      const numeric = typeof value === "string" ? Number(value) : Number.NaN;
      return Number.isFinite(numeric)
        && (field.schema.kind !== "integer" || Number.isInteger(numeric))
        && (field.schema.minimum === null || numeric >= field.schema.minimum)
        && (field.schema.maximum === null || numeric <= field.schema.maximum);
    }
    case "boolean":
      return typeof value === "boolean" || (!field.required && value === undefined);
    case "singleSelect": {
      const schema = field.schema;
      return typeof value === "string"
        && (!field.required && !value
          || schema.options.some((option) => option.value === value));
    }
    case "multiSelect": {
      const schema = field.schema;
      if (!Array.isArray(value)) return !field.required && value === undefined;
      return (schema.minItems === null || value.length >= schema.minItems)
        && (schema.maxItems === null || value.length <= schema.maxItems)
        && new Set(value).size === value.length
        && value.every((entry) => schema.options.some((option) => option.value === entry));
    }
  }
}

function responseContent(
  fields: McpFormFieldSummary[],
  values: Record<string, FieldValue>,
): McpFormContent {
  const content: McpFormContent = {};
  for (const field of fields) {
    const value = values[field.name];
    if (field.schema.kind === "number" || field.schema.kind === "integer") {
      if (value !== "" && typeof value === "string") content[field.name] = Number(value);
    } else if (field.schema.kind === "string" || field.schema.kind === "singleSelect") {
      if (typeof value === "string" && (value || field.required)) content[field.name] = value;
    } else if (field.schema.kind === "boolean" && typeof value === "boolean") {
      content[field.name] = value;
    } else if (field.schema.kind === "multiSelect" && Array.isArray(value)) {
      content[field.name] = value;
    }
  }
  return content;
}

function McpFormCard({
  request,
  submitting,
  onSubmit,
}: {
  request: PendingMcpFormSummary;
  submitting: boolean;
  onSubmit: Props["onSubmit"];
}) {
  const [values, setValues] = useState<Record<string, FieldValue>>(() => initialValues(request));

  useEffect(() => setValues(initialValues(request)), [request]);

  const valid = useMemo(
    () => request.fields.every((field) => fieldIsValid(field, values[field.name])),
    [request.fields, values],
  );

  const setValue = (name: string, value: FieldValue) => {
    setValues((current) => ({ ...current, [name]: value }));
  };

  const submit = (action: McpFormResponseAction) => {
    const content = action === "accept" ? responseContent(request.fields, values) : undefined;
    void onSubmit(request.id, request.version, action, content);
  };

  return (
    <div className="web-user-input-card web-mcp-form-card" role="group" aria-label={sourceLabel(request)}>
      <div className="web-user-input-title">
        <ListChecks size={16} aria-hidden="true" />
        <span>{sourceLabel(request)}</span>
      </div>
      <div className="web-mcp-form-server">{request.serverName}</div>
      <p className="web-user-input-prompt">{request.message}</p>
      {request.fields.map((field) => (
        <fieldset className="web-user-input-question web-mcp-form-field" key={field.name}>
          <legend className="web-user-input-header">
            {field.title}{field.required ? " *" : ""}
          </legend>
          {field.description ? <small>{field.description}</small> : null}
          {field.schema.kind === "string" ? (
            <input
              className="web-user-input-note"
              type="text"
              aria-label={field.title}
              value={typeof values[field.name] === "string" ? values[field.name] as string : ""}
              minLength={field.schema.minLength ?? undefined}
              maxLength={field.schema.maxLength ?? undefined}
              disabled={submitting}
              onChange={(event) => setValue(field.name, event.target.value)}
            />
          ) : field.schema.kind === "number" || field.schema.kind === "integer" ? (
            <input
              className="web-user-input-note"
              type="number"
              aria-label={field.title}
              value={typeof values[field.name] === "string" ? values[field.name] as string : ""}
              min={field.schema.minimum ?? undefined}
              max={field.schema.maximum ?? undefined}
              step={field.schema.kind === "integer" ? 1 : "any"}
              disabled={submitting}
              onChange={(event) => setValue(field.name, event.target.value)}
            />
          ) : field.schema.kind === "boolean" ? (
            <select
              className="web-user-input-note"
              aria-label={field.title}
              value={values[field.name] === undefined ? "" : String(values[field.name])}
              disabled={submitting}
              onChange={(event) => {
                const value = event.target.value;
                setValue(field.name, value === "" ? undefined : value === "true");
              }}
            >
              {!field.required ? <option value="">Not specified</option> : null}
              {field.required && values[field.name] === undefined ? <option value="">Select…</option> : null}
              <option value="true">Yes</option>
              <option value="false">No</option>
            </select>
          ) : field.schema.kind === "singleSelect" ? (
            <select
              className="web-user-input-note"
              aria-label={field.title}
              value={typeof values[field.name] === "string" ? values[field.name] as string : ""}
              disabled={submitting}
              onChange={(event) => setValue(field.name, event.target.value)}
            >
              {!field.required ? <option value="">Not specified</option> : null}
              {field.required && !field.schema.default ? <option value="">Select…</option> : null}
              {field.schema.options.map((option) => (
                <option key={option.value} value={option.value}>{option.label}</option>
              ))}
            </select>
          ) : (
            <span className="web-user-input-options">
              {field.schema.options.map((option) => {
                const selected = Array.isArray(values[field.name])
                  && (values[field.name] as string[]).includes(option.value);
                return (
                  <button
                    type="button"
                    className={`web-user-input-option${selected ? " is-selected" : ""}`}
                    key={option.value}
                    disabled={submitting}
                    onClick={() => {
                      const current = Array.isArray(values[field.name])
                        ? values[field.name] as string[]
                        : [];
                      setValue(
                        field.name,
                        selected
                          ? current.filter((value) => value !== option.value)
                          : [...current, option.value],
                      );
                    }}
                  >
                    {option.label}
                  </button>
                );
              })}
            </span>
          )}
        </fieldset>
      ))}
      <div className="web-user-input-actions web-mcp-form-actions">
        <button type="button" disabled={!valid || submitting} onClick={() => submit("accept")}>
          {submitting ? "Submitting…" : "Accept"}
        </button>
        <button type="button" disabled={submitting} onClick={() => submit("decline")}>
          Decline
        </button>
        <button type="button" disabled={submitting} onClick={() => submit("cancel")}>
          Cancel
        </button>
      </div>
    </div>
  );
}

export default function McpFormQueue({ requests, submittingIds, onSubmit }: Props) {
  if (requests.length === 0) return null;
  return (
    <div className="web-user-input-queue" aria-label="Pending MCP forms">
      {requests.map((request) => (
        <McpFormCard
          key={request.id}
          request={request}
          submitting={submittingIds.has(request.id)}
          onSubmit={onSubmit}
        />
      ))}
    </div>
  );
}
