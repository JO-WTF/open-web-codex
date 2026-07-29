use std::collections::{BTreeMap, HashSet};
use std::time::Duration;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use open_web_codex_git_runtime::GitRuntime;
use open_web_codex_platform_contracts::error::PlatformError;
use open_web_codex_platform_contracts::{
    PythonCapabilityPublishRequest, PythonCapabilityPublishResponse,
    PythonCapabilityToolTestRequest, PythonCapabilityToolTestResponse,
    PythonCapabilityValidationIssue, PythonCapabilityValidationResult,
};
use open_web_codex_platform_store::AppState;
use serde_json::{json, Value};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use uuid::Uuid;

use crate::middleware::auth::AuthenticatedUser;

use super::workspaces::{audit_workspace_mutation, authorized_workspace, git_error};

type ApiError = (StatusCode, Json<PlatformError>);
type ApiResult<T> = Result<Json<T>, ApiError>;

const MAX_SOURCE_BYTES: usize = 128 * 1024;
const MAX_INSTRUCTIONS_BYTES: usize = 32 * 1024;
const MAX_TOOLS: usize = 16;
const MAX_PROCESS_OUTPUT_BYTES: usize = 256 * 1024;

pub async fn validate(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(workspace_id): Path<Uuid>,
    Json(request): Json<PythonCapabilityPublishRequest>,
) -> ApiResult<PythonCapabilityValidationResult> {
    authorized_workspace(&state, &auth, workspace_id, true).await?;
    Ok(Json(validate_and_probe(&request).await))
}

pub async fn test_tool(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(workspace_id): Path<Uuid>,
    Json(request): Json<PythonCapabilityToolTestRequest>,
) -> ApiResult<PythonCapabilityToolTestResponse> {
    authorized_workspace(&state, &auth, workspace_id, true).await?;
    let validation = validate_and_probe(&request.capability).await;
    if !validation.valid {
        return Err(bad_request(
            validation
                .issues
                .first()
                .map(|issue| issue.message.as_str())
                .unwrap_or("Python capability is invalid"),
        ));
    }
    if !request
        .capability
        .tools
        .iter()
        .any(|tool| tool.name == request.tool_name)
    {
        return Err(bad_request("The requested Tool is not declared"));
    }
    let responses = run_protocol_probe(
        &request.capability,
        Some((&request.tool_name, &request.arguments)),
    )
    .await
    .map_err(|message| bad_request(&message))?;
    let call = responses
        .last()
        .and_then(|response| response.get("result"))
        .ok_or_else(|| bad_request("The Tool test returned no result"))?;
    if call.get("isError").and_then(Value::as_bool) == Some(true) {
        let message = call
            .pointer("/content/0/text")
            .and_then(Value::as_str)
            .unwrap_or("The Tool test failed");
        return Err(bad_request(message));
    }
    let result = call
        .get("structuredContent")
        .cloned()
        .or_else(|| {
            call.pointer("/content/0/text")
                .and_then(Value::as_str)
                .and_then(|text| serde_json::from_str(text).ok())
        })
        .unwrap_or(Value::Null);
    Ok(Json(PythonCapabilityToolTestResponse {
        tool_name: request.tool_name,
        result,
    }))
}

pub async fn publish(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(workspace_id): Path<Uuid>,
    Extension(git): Extension<std::sync::Arc<GitRuntime>>,
    Json(request): Json<PythonCapabilityPublishRequest>,
) -> ApiResult<PythonCapabilityPublishResponse> {
    let workspace_id = authorized_workspace(&state, &auth, workspace_id, true).await?;
    let validation = validate_and_probe(&request).await;
    if !validation.valid {
        return Err(bad_request(
            validation
                .issues
                .first()
                .map(|issue| issue.message.as_str())
                .unwrap_or("Python capability is invalid"),
        ));
    }
    let files = package_files(&request).map_err(|message| bad_request(&message))?;
    let written_files = git
        .publish_capability_package(workspace_id, &request.slug, &files)
        .await
        .map_err(git_error)?;
    audit_workspace_mutation(
        &state,
        &auth,
        workspace_id,
        workspace_id,
        "workspace.python_capability_published",
        &written_files,
    )
    .await?;
    Ok(Json(PythonCapabilityPublishResponse {
        package_id: request.slug.clone(),
        version: request.version,
        capability_root_id: format!("local-{}", request.slug),
        server_name: request.server_name,
        skill_name: request.skill.name,
        written_files,
    }))
}

async fn validate_and_probe(
    request: &PythonCapabilityPublishRequest,
) -> PythonCapabilityValidationResult {
    let mut issues = validate_shape(request);
    if issues.is_empty() {
        if let Err(message) = run_protocol_probe(request, None).await {
            issues.push(issue("runtime_probe_failed", message));
        }
    }
    PythonCapabilityValidationResult {
        valid: issues.is_empty(),
        tool_names: request.tools.iter().map(|tool| tool.name.clone()).collect(),
        issues,
    }
}

fn validate_shape(
    request: &PythonCapabilityPublishRequest,
) -> Vec<PythonCapabilityValidationIssue> {
    let mut issues = Vec::new();
    if !valid_slug(&request.slug) {
        issues.push(issue(
            "invalid_slug",
            "Package slug must use lowercase letters, digits and internal hyphens",
        ));
    }
    if !valid_identifier(&request.server_name) {
        issues.push(issue(
            "invalid_server_name",
            "MCP server name must be a Python-style identifier",
        ));
    }
    if !valid_semver(&request.version) {
        issues.push(issue(
            "invalid_version",
            "Version must use MAJOR.MINOR.PATCH",
        ));
    }
    if request.display_name.trim().is_empty() || request.display_name.len() > 120 {
        issues.push(issue(
            "invalid_display_name",
            "Display name must contain between 1 and 120 characters",
        ));
    }
    if request.description.trim().is_empty() || request.description.len() > 500 {
        issues.push(issue(
            "invalid_description",
            "Description must contain between 1 and 500 characters",
        ));
    }
    if request.python_source.trim().is_empty() || request.python_source.len() > MAX_SOURCE_BYTES {
        issues.push(issue(
            "invalid_python_source",
            "Python source must contain between 1 byte and 128 KiB",
        ));
    }
    if request.tools.is_empty() || request.tools.len() > MAX_TOOLS {
        issues.push(issue("invalid_tools", "Declare between 1 and 16 Tools"));
    }
    let mut names = HashSet::new();
    for tool in &request.tools {
        if !valid_identifier(&tool.name) || !names.insert(tool.name.as_str()) {
            issues.push(issue(
                "invalid_tool_name",
                "Tool names must be unique Python-style identifiers",
            ));
        }
        if tool.description.trim().is_empty() || tool.description.len() > 500 {
            issues.push(issue(
                "invalid_tool_description",
                "Every Tool needs a bounded description",
            ));
        }
        if !tool.input_schema.is_object()
            || tool.input_schema.get("type").and_then(Value::as_str) != Some("object")
        {
            issues.push(issue(
                "invalid_tool_schema",
                "Every Tool input schema must be a JSON object schema",
            ));
        }
    }
    if !valid_slug(&request.skill.name) {
        issues.push(issue(
            "invalid_skill_name",
            "Skill name must use lowercase letters, digits and internal hyphens",
        ));
    }
    if request.skill.description.trim().is_empty() || request.skill.description.len() > 500 {
        issues.push(issue(
            "invalid_skill_description",
            "Skill description must contain between 1 and 500 characters",
        ));
    }
    if request.skill.instructions.trim().is_empty()
        || request.skill.instructions.len() > MAX_INSTRUCTIONS_BYTES
    {
        issues.push(issue(
            "invalid_skill_instructions",
            "Skill instructions must contain between 1 byte and 32 KiB",
        ));
    }
    issues
}

async fn run_protocol_probe(
    request: &PythonCapabilityPublishRequest,
    tool_call: Option<(&str, &Value)>,
) -> Result<Vec<Value>, String> {
    let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
    let files = package_files(request)?;
    for (relative, content) in files {
        let target = directory.path().join(relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::write(target, content).map_err(|error| error.to_string())?;
    }
    let mut child = Command::new("python3")
        .arg(directory.path().join("server.py"))
        .current_dir(directory.path())
        .env_clear()
        .env(
            "PATH",
            std::env::var_os("PATH").unwrap_or_else(|| "/usr/bin:/bin".into()),
        )
        .env("HOME", directory.path())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| format!("Unable to start python3: {error}"))?;
    let mut requests = vec![
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "open-web-codex-validator", "version": "1"}
            }
        }),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized", "params": {}}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}),
    ];
    if let Some((tool_name, arguments)) = tool_call {
        requests.push(json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {"name": tool_name, "arguments": arguments}
        }));
    }
    let input = requests
        .into_iter()
        .map(|request| serde_json::to_string(&request).unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    child
        .stdin
        .take()
        .ok_or("Python MCP stdin was unavailable")?
        .write_all(input.as_bytes())
        .await
        .map_err(|error| error.to_string())?;
    let output = tokio::time::timeout(Duration::from_secs(5), child.wait_with_output())
        .await
        .map_err(|_| "Python MCP validation timed out".to_string())?
        .map_err(|error| error.to_string())?;
    if output.stdout.len() > MAX_PROCESS_OUTPUT_BYTES
        || output.stderr.len() > MAX_PROCESS_OUTPUT_BYTES
    {
        return Err("Python MCP validation output exceeded 256 KiB".to_string());
    }
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "Python MCP exited before validation completed: {}",
            bounded_message(&stderr)
        ));
    }
    let responses = String::from_utf8(output.stdout)
        .map_err(|_| "Python MCP returned non-UTF-8 output".to_string())?
        .lines()
        .map(|line| {
            serde_json::from_str::<Value>(line)
                .map_err(|_| "Python MCP returned invalid JSON-RPC output".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let tools = responses
        .iter()
        .find(|response| response.get("id") == Some(&json!(2)))
        .and_then(|response| response.pointer("/result/tools"))
        .and_then(Value::as_array)
        .ok_or("Python MCP did not return tools/list")?;
    let returned = tools
        .iter()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str))
        .collect::<HashSet<_>>();
    if request
        .tools
        .iter()
        .any(|tool| !returned.contains(tool.name.as_str()))
    {
        return Err("Python MCP tools/list omitted a declared Tool".to_string());
    }
    Ok(responses)
}

fn package_files(
    request: &PythonCapabilityPublishRequest,
) -> Result<BTreeMap<String, String>, String> {
    let tools = request
        .tools
        .iter()
        .map(|tool| {
            json!({
                "name": tool.name,
                "description": tool.description,
                "inputSchema": tool.input_schema,
            })
        })
        .collect::<Vec<_>>();
    let manifest = json!({
        "name": request.slug,
        "version": request.version,
        "description": request.description,
        "author": {"name": "Open Web Codex user"},
        "skills": "./skills/",
        "interface": {
            "displayName": request.display_name,
            "shortDescription": request.description,
            "longDescription": request.description,
            "developerName": "Open Web Codex user",
            "category": "Custom",
            "capabilities": request.tools.iter().map(|tool| tool.description.clone()).collect::<Vec<_>>(),
            "defaultPrompt": format!("Use ${} for this task.", request.skill.name),
        },
        "mcpServers": "./.mcp.json",
    });
    let mcp = json!({
        "mcpServers": {
            request.server_name.clone(): {
                "command": "./bin/launcher",
                "args": [],
                "cwd": ".",
                "startup_timeout_sec": 30,
                "tool_timeout_sec": 60,
                "default_tools_approval_mode": "prompt",
                "env_vars": [],
            }
        }
    });
    let skill_description =
        serde_json::to_string(&request.skill.description).map_err(|error| error.to_string())?;
    let skill = format!(
        "---\nname: {}\ndescription: {}\n---\n\n# {}\n\n{}\n",
        request.skill.name,
        skill_description,
        request.display_name.trim(),
        request.skill.instructions.trim()
    );
    let launcher = "#!/usr/bin/env sh\nset -eu\nroot=$(CDPATH= cd -- \"$(dirname -- \"$0\")/..\" && pwd)\nexec env -i PATH=\"${PATH:-/usr/bin:/bin}\" HOME=\"$root\" python3 \"$root/server.py\"\n";
    let server = generated_server(&tools)?;
    let pyproject = format!(
        "[project]\nname = \"{}\"\nversion = \"{}\"\nrequires-python = \">=3.11\"\n",
        request.slug, request.version
    );
    Ok(BTreeMap::from([
        (
            ".codex-plugin/plugin.json".to_string(),
            serde_json::to_string_pretty(&manifest).map_err(|error| error.to_string())? + "\n",
        ),
        (
            ".mcp.json".to_string(),
            serde_json::to_string_pretty(&mcp).map_err(|error| error.to_string())? + "\n",
        ),
        (
            ".gitignore".to_string(),
            "__pycache__/\n*.pyc\n".to_string(),
        ),
        ("bin/launcher".to_string(), launcher.to_string()),
        (
            "implementation.py".to_string(),
            request.python_source.clone(),
        ),
        ("pyproject.toml".to_string(), pyproject),
        ("server.py".to_string(), server),
        (format!("skills/{}/SKILL.md", request.skill.name), skill),
    ]))
}

fn generated_server(tools: &[Value]) -> Result<String, String> {
    let tools_json = serde_json::to_string(tools).map_err(|error| error.to_string())?;
    let tools_literal = serde_json::to_string(&tools_json).map_err(|error| error.to_string())?;
    Ok(format!(
        r#"#!/usr/bin/env python3
import importlib
import json
import sys

implementation = importlib.import_module("implementation")
TOOLS = json.loads({tools_literal})
for tool in TOOLS:
    function = getattr(implementation, tool["name"], None)
    if not callable(function):
        raise RuntimeError(f'implementation.py must define {{tool["name"]}}(arguments)')

def send(payload):
    sys.stdout.write(json.dumps(payload, separators=(",", ":")) + "\n")
    sys.stdout.flush()

def result(request_id, value):
    send({{"jsonrpc": "2.0", "id": request_id, "result": value}})

for line in sys.stdin:
    if not line.strip():
        continue
    request = json.loads(line)
    request_id = request.get("id")
    method = request.get("method")
    if method == "initialize":
        result(request_id, {{
            "protocolVersion": request.get("params", {{}}).get("protocolVersion", "2025-06-18"),
            "capabilities": {{"tools": {{}}}},
            "serverInfo": {{"name": "user-python-capability", "version": "1"}},
        }})
    elif method == "tools/list":
        result(request_id, {{"tools": TOOLS}})
    elif method == "tools/call":
        params = request.get("params", {{}})
        name = params.get("name")
        try:
            value = getattr(implementation, name)(params.get("arguments", {{}}))
            result(request_id, {{
                "content": [{{"type": "text", "text": json.dumps(value, ensure_ascii=False)}}],
                "structuredContent": value,
                "isError": False,
            }})
        except Exception as error:
            result(request_id, {{
                "content": [{{"type": "text", "text": str(error)}}],
                "isError": True,
            }})
    elif request_id is not None:
        result(request_id, {{}})
"#
    ))
}

fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && !value.starts_with('-')
        && !value.ends_with('-')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn valid_identifier(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        && value.len() <= 64
}

fn valid_semver(value: &str) -> bool {
    let parts = value.split('.').collect::<Vec<_>>();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

fn issue(code: impl Into<String>, message: impl Into<String>) -> PythonCapabilityValidationIssue {
    PythonCapabilityValidationIssue {
        code: code.into(),
        message: message.into(),
    }
}

fn bounded_message(value: &str) -> String {
    value.trim().chars().take(500).collect()
}

fn bad_request(message: &str) -> ApiError {
    (
        StatusCode::BAD_REQUEST,
        Json(PlatformError::bad_request(message)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use open_web_codex_platform_contracts::{PythonCapabilitySkill, PythonCapabilityTool};

    fn stock_capability() -> PythonCapabilityPublishRequest {
        PythonCapabilityPublishRequest {
            slug: "stock-history".to_string(),
            version: "1.0.0".to_string(),
            display_name: "Stock history".to_string(),
            description: "Looks up a stock and returns bounded price history.".to_string(),
            server_name: "stock_data".to_string(),
            python_source: r#"
def lookup_stock(arguments):
    return {"ticker": "DEMO", "exchange": "TEST", "name": arguments["name"]}

def get_price_history(arguments):
    return [{"date": "2026-07-29", "close": 10.5}]
"#
            .trim_start()
            .to_string(),
            tools: vec![
                PythonCapabilityTool {
                    name: "lookup_stock".to_string(),
                    description: "Resolve a stock name to its code.".to_string(),
                    input_schema: json!({
                        "type": "object",
                        "properties": {"name": {"type": "string"}},
                        "required": ["name"],
                        "additionalProperties": false
                    }),
                },
                PythonCapabilityTool {
                    name: "get_price_history".to_string(),
                    description: "Return bounded historical prices.".to_string(),
                    input_schema: json!({
                        "type": "object",
                        "properties": {"ticker": {"type": "string"}}
                    }),
                },
            ],
            skill: PythonCapabilitySkill {
                name: "stock-history".to_string(),
                description: "Use for stock history questions.".to_string(),
                instructions: "Call stock_data.lookup_stock, then stock_data.get_price_history."
                    .to_string(),
            },
        }
    }

    #[tokio::test]
    async fn validates_and_calls_generated_python_mcp() {
        let capability = stock_capability();
        let validation = validate_and_probe(&capability).await;
        assert_eq!(validation.issues, Vec::new());
        assert!(validation.valid);
        let responses = run_protocol_probe(
            &capability,
            Some(("lookup_stock", &json!({"name": "Example"}))),
        )
        .await
        .expect("tool probe");
        assert_eq!(
            responses
                .last()
                .unwrap()
                .pointer("/result/structuredContent/ticker"),
            Some(&json!("DEMO"))
        );
    }

    #[tokio::test]
    async fn rejects_missing_implementation_function() {
        let mut capability = stock_capability();
        capability.python_source = "def lookup_stock(arguments): return {}".to_string();
        assert!(validate_shape(&capability).is_empty());
        let validation = validate_and_probe(&capability).await;
        assert!(!validation.valid);
        assert_eq!(validation.issues[0].code, "runtime_probe_failed");
        assert!(validation.issues[0].message.contains("get_price_history"));
    }

    #[test]
    fn emits_standard_plugin_package() {
        let files = package_files(&stock_capability()).expect("package");
        assert!(files.contains_key(".codex-plugin/plugin.json"));
        assert!(files.contains_key(".mcp.json"));
        assert!(files.contains_key("skills/stock-history/SKILL.md"));
        assert!(files[".mcp.json"].contains("\"./bin/launcher\""));
        assert!(files[".mcp.json"].contains("\"prompt\""));
        assert!(files["bin/launcher"].contains("env -i"));
    }
}
