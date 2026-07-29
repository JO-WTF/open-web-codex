import { useState } from "react";
import CheckCircle2 from "lucide-react/dist/esm/icons/check-circle-2";
import FlaskConical from "lucide-react/dist/esm/icons/flask-conical";
import PackageCheck from "lucide-react/dist/esm/icons/package-check";
import ShieldCheck from "lucide-react/dist/esm/icons/shield-check";
import type {
  PythonCapabilityPublishRequest,
  PythonCapabilityPublishResponse,
  PythonCapabilityTool,
  PythonCapabilityValidationResult,
} from "../../../browser/types";
import { platformClient } from "../../../browser/session";
import type { WorkspaceInfo } from "../../types";

type Props = {
  workspaces: WorkspaceInfo[];
  activeWorkspaceId: string | null;
  onPublished: () => void;
  onStartThread: (workspaceId: string) => void;
};

const stockTools: PythonCapabilityTool[] = [
  {
    name: "lookup_stock",
    description: "Resolve a stock name to its exchange and ticker symbol.",
    input_schema: {
      type: "object",
      properties: {
        name: { type: "string", description: "Company or stock name" },
      },
      required: ["name"],
      additionalProperties: false,
    },
  },
  {
    name: "get_price_history",
    description: "Fetch daily price history for a ticker and bounded date range.",
    input_schema: {
      type: "object",
      properties: {
        ticker: { type: "string" },
        start_date: { type: "string", description: "YYYY-MM-DD" },
        end_date: { type: "string", description: "YYYY-MM-DD" },
      },
      required: ["ticker", "start_date", "end_date"],
      additionalProperties: false,
    },
  },
];

const stockPython = `import json
from urllib.parse import quote, urlencode
from urllib.request import Request, urlopen

# Replace these two endpoints with the websites selected for your capability.
BASE_INFO_URL = "https://example.com/api/stocks/search"
HISTORY_URL = "https://example.com/api/stocks/history"

def _get_json(url):
    request = Request(url, headers={"Accept": "application/json", "User-Agent": "stock-history-skill/1.0"})
    with urlopen(request, timeout=15) as response:
        return json.load(response)

def lookup_stock(arguments):
    name = arguments["name"].strip()
    data = _get_json(f"{BASE_INFO_URL}?q={quote(name)}")
    # Adapt these fields to the first website's response.
    return {
        "name": data["name"],
        "ticker": data["ticker"],
        "exchange": data.get("exchange"),
        "source": BASE_INFO_URL,
    }

def get_price_history(arguments):
    query = urlencode({
        "ticker": arguments["ticker"],
        "start": arguments["start_date"],
        "end": arguments["end_date"],
    })
    data = _get_json(f"{HISTORY_URL}?{query}")
    # Keep source and requested range visible in the result.
    return {
        "ticker": arguments["ticker"],
        "start_date": arguments["start_date"],
        "end_date": arguments["end_date"],
        "records": data["records"],
        "source": HISTORY_URL,
    }
`;

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : "The operation failed.";
}

export default function PythonCapabilityEditor({
  workspaces,
  activeWorkspaceId,
  onPublished,
  onStartThread,
}: Props) {
  const initialWorkspaceId =
    activeWorkspaceId && workspaces.some((workspace) => workspace.id === activeWorkspaceId)
      ? activeWorkspaceId
      : workspaces[0]?.id ?? "";
  const [workspaceId, setWorkspaceId] = useState(initialWorkspaceId);
  const [slug, setSlug] = useState("stock-history");
  const [version, setVersion] = useState("1.0.0");
  const [displayName, setDisplayName] = useState("Stock history");
  const [description, setDescription] = useState(
    "Resolve a stock and fetch its bounded historical prices from declared sources.",
  );
  const [serverName, setServerName] = useState("stock_data");
  const [pythonSource, setPythonSource] = useState(stockPython);
  const [toolsSource, setToolsSource] = useState(JSON.stringify(stockTools, null, 2));
  const [skillName, setSkillName] = useState("stock-history");
  const [skillDescription, setSkillDescription] = useState(
    "Use when a user asks for stock details or historical prices.",
  );
  const [skillInstructions, setSkillInstructions] = useState(
    "First call stock_data.lookup_stock with the company name. Use the returned ticker to call stock_data.get_price_history for the requested date range. Report both source fields and do not invent missing records.",
  );
  const [testToolName, setTestToolName] = useState("lookup_stock");
  const [testArguments, setTestArguments] = useState('{"name":"Example Corp"}');
  const [validation, setValidation] =
    useState<PythonCapabilityValidationResult | null>(null);
  const [testResult, setTestResult] = useState<unknown>(null);
  const [published, setPublished] =
    useState<PythonCapabilityPublishResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busyAction, setBusyAction] = useState<"validate" | "test" | "publish" | null>(
    null,
  );

  const buildRequest = (): PythonCapabilityPublishRequest => {
    const parsedTools: unknown = JSON.parse(toolsSource);
    if (!Array.isArray(parsedTools)) {
      throw new Error("Tools must be a JSON array.");
    }
    return {
      slug,
      version,
      display_name: displayName,
      description,
      server_name: serverName,
      python_source: pythonSource,
      tools: parsedTools as PythonCapabilityTool[],
      skill: {
        name: skillName,
        description: skillDescription,
        instructions: skillInstructions,
      },
    };
  };

  const runAction = async (
    action: "validate" | "test" | "publish",
  ) => {
    if (!workspaceId) {
      setError("Select a Workspace before continuing.");
      return;
    }
    setBusyAction(action);
    setError(null);
    setPublished(null);
    try {
      const capability = buildRequest();
      if (action === "validate") {
        const result = await platformClient.validatePythonCapability(
          workspaceId,
          capability,
        );
        setValidation(result);
        return;
      }
      if (action === "test") {
        const argumentsValue: unknown = JSON.parse(testArguments);
        if (
          !argumentsValue
          || typeof argumentsValue !== "object"
          || Array.isArray(argumentsValue)
        ) {
          throw new Error("Test arguments must be a JSON object.");
        }
        const result = await platformClient.testPythonCapability(workspaceId, {
          capability,
          tool_name: testToolName,
          arguments: argumentsValue as Record<string, unknown>,
        });
        setTestResult(result.result);
        setValidation({
          valid: true,
          tool_names: capability.tools.map((tool) => tool.name),
          issues: [],
        });
        return;
      }
      const result = await platformClient.publishPythonCapability(
        workspaceId,
        capability,
      );
      setPublished(result);
      setValidation({
        valid: true,
        tool_names: capability.tools.map((tool) => tool.name),
        issues: [],
      });
      onPublished();
    } catch (actionError) {
      setError(errorMessage(actionError));
    } finally {
      setBusyAction(null);
    }
  };

  return (
    <section className="web-python-capability-editor">
      <div className="web-python-capability-callout">
        <ShieldCheck size={17} aria-hidden="true" />
        <p>
          The platform fixes the package location, launcher, and MCP protocol. Your
          Python file only implements the declared Tool functions.
        </p>
      </div>

      <div className="web-python-capability-grid">
        <label className="settings-label">
          Workspace
          <select
            className="settings-select"
            value={workspaceId}
            onChange={(event) => setWorkspaceId(event.target.value)}
          >
            <option value="">Select a Workspace</option>
            {workspaces.map((workspace) => (
              <option key={workspace.id} value={workspace.id}>
                {workspace.name}
              </option>
            ))}
          </select>
        </label>
        <label className="settings-label">
          Package ID
          <input
            className="settings-input"
            value={slug}
            onChange={(event) => setSlug(event.target.value)}
          />
        </label>
        <label className="settings-label">
          Version
          <input
            className="settings-input"
            value={version}
            onChange={(event) => setVersion(event.target.value)}
          />
        </label>
        <label className="settings-label">
          MCP server name
          <input
            className="settings-input"
            value={serverName}
            onChange={(event) => setServerName(event.target.value)}
          />
        </label>
        <label className="settings-label">
          Display name
          <input
            className="settings-input"
            value={displayName}
            onChange={(event) => setDisplayName(event.target.value)}
          />
        </label>
        <label className="settings-label web-python-capability-wide">
          Description
          <input
            className="settings-input"
            value={description}
            onChange={(event) => setDescription(event.target.value)}
          />
        </label>
      </div>

      <label className="settings-label">
        Tools
        <span className="web-python-capability-hint">
          JSON array with name, description, and input_schema for each function.
        </span>
        <textarea
          className="settings-agents-textarea"
          value={toolsSource}
          onChange={(event) => setToolsSource(event.target.value)}
          spellCheck={false}
        />
      </label>

      <label className="settings-label">
        Python implementation
        <span className="web-python-capability-hint">
          Standard-library Python. Define one function per Tool; each receives an
          arguments dictionary and returns JSON-compatible data.
        </span>
        <textarea
          className="settings-agents-textarea web-python-capability-code"
          value={pythonSource}
          onChange={(event) => setPythonSource(event.target.value)}
          spellCheck={false}
        />
      </label>

      <div className="web-python-capability-grid">
        <label className="settings-label">
          Skill name
          <input
            className="settings-input"
            value={skillName}
            onChange={(event) => setSkillName(event.target.value)}
          />
        </label>
        <label className="settings-label web-python-capability-wide">
          Skill description
          <input
            className="settings-input"
            value={skillDescription}
            onChange={(event) => setSkillDescription(event.target.value)}
          />
        </label>
      </div>
      <label className="settings-label">
        Skill instructions
        <textarea
          className="settings-agents-textarea settings-agents-textarea--compact"
          value={skillInstructions}
          onChange={(event) => setSkillInstructions(event.target.value)}
        />
      </label>

      <div className="web-python-capability-test">
        <label className="settings-label">
          Tool to test
          <input
            className="settings-input"
            value={testToolName}
            onChange={(event) => setTestToolName(event.target.value)}
          />
        </label>
        <label className="settings-label">
          Arguments
          <input
            className="settings-input"
            value={testArguments}
            onChange={(event) => setTestArguments(event.target.value)}
          />
        </label>
      </div>

      <div className="settings-agents-actions web-python-capability-actions">
        <button
          type="button"
          className="ghost"
          disabled={busyAction !== null}
          onClick={() => void runAction("validate")}
        >
          <CheckCircle2 size={15} aria-hidden="true" />
          {busyAction === "validate" ? "Validating…" : "Validate"}
        </button>
        <button
          type="button"
          className="ghost"
          disabled={busyAction !== null}
          onClick={() => void runAction("test")}
        >
          <FlaskConical size={15} aria-hidden="true" />
          {busyAction === "test" ? "Testing…" : "Test Tool"}
        </button>
        <button
          type="button"
          className="settings-studio-create-button"
          disabled={busyAction !== null}
          onClick={() => void runAction("publish")}
        >
          <PackageCheck size={16} aria-hidden="true" />
          {busyAction === "publish" ? "Publishing…" : "Publish package"}
        </button>
      </div>

      {error && <div className="settings-agents-error">{error}</div>}
      {validation && !validation.valid && (
        <div className="settings-agents-error">
          {validation.issues.map((issue) => (
            <p key={`${issue.code}:${issue.message}`}>{issue.message}</p>
          ))}
        </div>
      )}
      {validation?.valid && !published && (
        <div className="web-python-capability-success">
          <CheckCircle2 size={16} aria-hidden="true" />
          MCP startup and Tool discovery passed for: {validation.tool_names.join(", ")}.
        </div>
      )}
      {testResult !== null && (
        <pre className="web-python-capability-result">
          {JSON.stringify(testResult, null, 2)}
        </pre>
      )}
      {published && (
        <div className="web-python-capability-published">
          <div>
            <strong>
              {published.package_id}@{published.version} published
            </strong>
            <span>
              Includes MCP {published.server_name} and Skill ${published.skill_name}.
            </span>
          </div>
          <button
            type="button"
            className="ghost"
            onClick={() => onStartThread(workspaceId)}
          >
            Start test Thread
          </button>
        </div>
      )}
    </section>
  );
}
