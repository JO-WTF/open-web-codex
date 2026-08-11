// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { PendingMcpFormSummary } from "../../../browser/types";
import McpFormQueue from "./McpFormQueue";

afterEach(cleanup);

const request: PendingMcpFormSummary = {
  id: "approval-1",
  runId: "run-1",
  source: { kind: "agent", executionId: "execution-1", displayTitle: "Data Agent" },
  serverName: "supply_chain_data",
  message: "Choose the route calculation inputs.",
  fields: [
    {
      name: "name",
      title: "Scenario name",
      description: "A visible label",
      required: true,
      schema: { kind: "string", default: "Default plan", minLength: 3, maxLength: 30 },
    },
    {
      name: "factor",
      title: "Detour factor",
      description: "",
      required: true,
      schema: { kind: "number", default: 1.2, minimum: 1, maximum: 2 },
    },
    {
      name: "warehouses",
      title: "Warehouse count",
      description: "",
      required: true,
      schema: { kind: "integer", default: 5, minimum: 1, maximum: 20 },
    },
    {
      name: "navigation",
      title: "Use navigation",
      description: "",
      required: true,
      schema: { kind: "boolean", default: false },
    },
    {
      name: "method",
      title: "Method",
      description: "",
      required: true,
      schema: {
        kind: "singleSelect",
        options: [
          { value: "curve", label: "Curve distance" },
          { value: "navigation", label: "Navigation" },
        ],
        default: "curve",
      },
    },
    {
      name: "targets",
      title: "Service targets",
      description: "",
      required: true,
      schema: {
        kind: "multiSelect",
        options: [
          { value: "6h", label: "6 hours" },
          { value: "12h", label: "12 hours" },
        ],
        default: ["12h"],
        minItems: 1,
        maxItems: 2,
      },
    },
  ],
  state: "pending",
  version: 3,
  createdAt: "2026-08-09T00:00:00Z",
};

const optionalFieldsRequest: PendingMcpFormSummary = {
  ...request,
  id: "approval-optional-fields",
  fields: [
    ...request.fields,
    {
      name: "includeExistingWarehouses",
      title: "Include existing warehouses",
      description: "",
      required: false,
      schema: { kind: "boolean", default: null },
    },
    {
      name: "candidateTiers",
      title: "Candidate tiers",
      description: "",
      required: false,
      schema: {
        kind: "multiSelect",
        options: [{ value: "urban", label: "Urban candidates" }],
        default: null,
        minItems: null,
        maxItems: 1,
      },
    },
  ],
};

describe("McpFormQueue", () => {
  it("submits explicit non-default values as typed form content", () => {
    const onSubmit = vi.fn();
    render(<McpFormQueue requests={[request]} submittingIds={new Set()} onSubmit={onSubmit} />);

    fireEvent.change(screen.getByLabelText(/Scenario name/), { target: { value: "Actual plan" } });
    fireEvent.change(screen.getByLabelText(/Detour factor/), { target: { value: "1.35" } });
    fireEvent.change(screen.getByLabelText(/Use navigation/), { target: { value: "true" } });
    fireEvent.change(screen.getByLabelText(/Method/), { target: { value: "navigation" } });
    fireEvent.click(screen.getByRole("button", { name: "6 hours" }));
    expect(screen.getByText("Data Agent requests additional information")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Submit" }));

    expect(onSubmit).toHaveBeenCalledWith("approval-1", 3, "accept", {
      name: "Actual plan",
      factor: 1.35,
      warehouses: 5,
      navigation: true,
      method: "navigation",
      targets: ["12h", "6h"],
    });
  });

  it("keeps decline and cancel distinct and omits content", () => {
    const onSubmit = vi.fn();
    render(<McpFormQueue requests={[request]} submittingIds={new Set()} onSubmit={onSubmit} />);

    fireEvent.click(screen.getByRole("button", { name: "Decline" }));
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

    expect(onSubmit).toHaveBeenNthCalledWith(1, "approval-1", 3, "decline", undefined);
    expect(onSubmit).toHaveBeenNthCalledWith(2, "approval-1", 3, "cancel", undefined);
  });

  it("renders and accepts an official zero-field MCP tool approval", () => {
    const onSubmit = vi.fn();
    render(
      <McpFormQueue
        requests={[{
          ...request,
          id: "approval-tool-1",
          message: "Allow the supply_chain MCP server to run this tool?",
          fields: [],
        }]}
        submittingIds={new Set()}
        onSubmit={onSubmit}
      />,
    );

    expect(screen.getByText(/Allow the supply_chain MCP server/)).toBeTruthy();
    expect(screen.getByText("Data Agent requests approval to run an MCP tool")).toBeTruthy();
    expect(screen.getByText("MCP tool approval")).toBeTruthy();
    expect(screen.getByText("MCP server")).toBeTruthy();
    expect(screen.getByText("supply_chain_data").tagName).toBe("CODE");
    expect(screen.queryByText(/needs details/i)).toBeNull();
    expect(screen.getByRole("button", { name: "Approve" }).className)
      .toContain("web-mcp-form-action-primary");
    fireEvent.click(screen.getByRole("button", { name: "Approve" }));
    expect(onSubmit).toHaveBeenCalledWith("approval-tool-1", 3, "accept", {});
  });

  it("blocks acceptance when a bounded field is invalid", () => {
    render(<McpFormQueue requests={[request]} submittingIds={new Set()} onSubmit={vi.fn()} />);
    fireEvent.change(screen.getByLabelText(/Warehouse count/), { target: { value: "1.5" } });
    expect((screen.getByRole("button", { name: "Submit" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("omits untouched optional boolean and multi-select fields but keeps explicit false and empty values", () => {
    const onSubmit = vi.fn();
    render(<McpFormQueue requests={[optionalFieldsRequest]} submittingIds={new Set()} onSubmit={onSubmit} />);

    fireEvent.click(screen.getByRole("button", { name: "Submit" }));
    const omittedContent = onSubmit.mock.calls[0]?.[3];
    expect(omittedContent).not.toHaveProperty("includeExistingWarehouses");
    expect(omittedContent).not.toHaveProperty("candidateTiers");

    fireEvent.change(screen.getByLabelText(/Include existing warehouses/), { target: { value: "false" } });
    fireEvent.click(screen.getByRole("button", { name: "Urban candidates" }));
    fireEvent.click(screen.getByRole("button", { name: "Urban candidates" }));
    fireEvent.click(screen.getByRole("button", { name: "Submit" }));

    expect(onSubmit.mock.calls[1]?.[3]).toMatchObject({
      includeExistingWarehouses: false,
      candidateTiers: [],
    });
  });
});
