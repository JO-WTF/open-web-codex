use std::collections::{BTreeMap, HashSet};
use std::path::Path as FsPath;
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
use sha2::{Digest, Sha256};
use sqlx::Row;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use uuid::Uuid;

use crate::middleware::auth::AuthenticatedUser;

use super::workspaces::{authorized_workspace, git_error};

type ApiError = (StatusCode, Json<PlatformError>);
type ApiResult<T> = Result<Json<T>, ApiError>;

const MAX_SOURCE_BYTES: usize = 128 * 1024;
const MAX_INSTRUCTIONS_BYTES: usize = 32 * 1024;
const MAX_TOOLS: usize = 16;
const MAX_PROCESS_OUTPUT_BYTES: usize = 256 * 1024;
const PROTOCOL_PROBE_TIMEOUT: Duration = Duration::from_secs(5);

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
    Extension(git): Extension<std::sync::Arc<GitRuntime>>,
    Json(request): Json<PythonCapabilityToolTestRequest>,
) -> ApiResult<PythonCapabilityToolTestResponse> {
    let workspace_id = authorized_workspace(&state, &auth, workspace_id, true).await?;
    let workspace_root = git.workspace_path(workspace_id);
    if !workspace_root.is_dir() {
        return Err(bad_request("The authorized Workspace is unavailable"));
    }
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
        Some((workspace_id, &workspace_root)),
    )
    .await
    .map_err(|message| bad_request(&message))?;
    let call = responses
        .last()
        .and_then(|response| response.get("result"))
        .ok_or_else(|| bad_request("The Tool test returned no result"))?;
    if call.get("isError").and_then(Value::as_bool) == Some(true) {
        return Err(bad_request(
            "The Python Tool test failed. Review the implementation and test inputs.",
        ));
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
    let content_sha256 = package_content_sha256(&files);
    let capability_root_id = format!(
        "local-{}-{}",
        request.slug,
        request.version.replace(['.', '_'], "-")
    );
    if let Some(existing) = sqlx::query(
        "SELECT id, state, content_sha256 \
         FROM workspace_capability_package_releases \
         WHERE organization_id = $1 AND workspace_id = $2 AND idempotency_key = $3",
    )
    .bind(auth.organization_id)
    .bind(workspace_id)
    .bind(&request.idempotency_key)
    .fetch_optional(&state.db)
    .await
    .map_err(database_error)?
    {
        let release_id: Uuid = existing.get("id");
        if existing.get::<String, _>("content_sha256") != content_sha256 {
            return Err(conflict(
                "The idempotency key is already bound to different capability content",
            ));
        }
        match existing.get::<String, _>("state").as_str() {
            "published" => {
                return Ok(Json(publish_response(
                    release_id,
                    &request,
                    capability_root_id,
                    content_sha256,
                    package_written_files(&request, &files),
                )));
            }
            "publishing" => {
                if git
                    .capability_package_matches(
                        workspace_id,
                        &request.slug,
                        &request.version,
                        release_id,
                        &content_sha256,
                    )
                    .await
                    .map_err(git_error)?
                {
                    finalize_capability_release(
                        &state,
                        &auth,
                        workspace_id,
                        release_id,
                        &content_sha256,
                        "workspace.python_capability_recovered",
                    )
                    .await?;
                    return Ok(Json(publish_response(
                        release_id,
                        &request,
                        capability_root_id,
                        content_sha256,
                        package_written_files(&request, &files),
                    )));
                }
                return Err(conflict("Capability publication is still in progress"));
            }
            "failed" => {
                return Err(conflict(
                    "This capability publication failed; use a new version and request",
                ));
            }
            _ => return Err(database_error_message()),
        }
    }
    if sqlx::query(
        "SELECT id FROM workspace_capability_package_releases \
         WHERE workspace_id = $1 AND package_id = $2 AND version = $3",
    )
    .bind(workspace_id)
    .bind(&request.slug)
    .bind(&request.version)
    .fetch_optional(&state.db)
    .await
    .map_err(database_error)?
    .is_some()
    {
        return Err(conflict(
            "This capability package ID and version already exists",
        ));
    }

    let release_id = Uuid::now_v7();
    let tool_names = request
        .tools
        .iter()
        .map(|tool| tool.name.clone())
        .collect::<Vec<_>>();
    let capabilities = request
        .tools
        .iter()
        .map(|tool| format!("{}.{}", request.server_name, tool.name))
        .collect::<Vec<_>>();
    let mut transaction = state.db.begin().await.map_err(database_error)?;
    sqlx::query(
        "INSERT INTO workspace_capability_package_releases \
         (id, organization_id, workspace_id, owner_user_id, idempotency_key, package_id, \
          version, display_name, description, state, capability_root_id, server_name, \
          skill_name, tool_names, capabilities, input_artifact_types, output_artifact_types, \
          content_sha256) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'publishing', $10, $11, $12, \
                 $13, $14, $15, $16, $17)",
    )
    .bind(release_id)
    .bind(auth.organization_id)
    .bind(workspace_id)
    .bind(auth.user_id)
    .bind(&request.idempotency_key)
    .bind(&request.slug)
    .bind(&request.version)
    .bind(request.display_name.trim())
    .bind(request.description.trim())
    .bind(&capability_root_id)
    .bind(&request.server_name)
    .bind(&request.skill.name)
    .bind(json!(tool_names))
    .bind(json!(capabilities))
    .bind(json!(request.input_artifact_types))
    .bind(json!(request.output_artifact_types))
    .bind(&content_sha256)
    .execute(&mut *transaction)
    .await
    .map_err(database_conflict)?;
    audit_capability_release(
        &mut transaction,
        &auth,
        release_id,
        "workspace.python_capability_started",
        "success",
        json!({
            "workspaceId": workspace_id,
            "packageId": request.slug,
            "version": request.version,
            "contentSha256": content_sha256,
        }),
    )
    .await?;
    transaction.commit().await.map_err(database_error)?;

    let mut publish_files = files.clone();
    publish_files.insert(
        ".open-web-release.json".to_string(),
        serde_json::to_string_pretty(&json!({
            "schemaVersion": "workspace.capability-package-release.v1",
            "workspaceId": workspace_id,
            "releaseId": release_id,
            "packageId": request.slug,
            "version": request.version,
            "contentSha256": content_sha256,
        }))
        .map_err(|_| database_error_message())?
            + "\n",
    );
    let written_files = match git
        .publish_capability_package(
            workspace_id,
            &request.slug,
            &request.version,
            &publish_files,
        )
        .await
    {
        Ok(written_files) => written_files,
        Err(error) => {
            mark_capability_release_failed(
                &state,
                &auth,
                workspace_id,
                release_id,
                capability_failure_code(&error),
            )
            .await?;
            return Err(git_error(error));
        }
    };
    finalize_capability_release(
        &state,
        &auth,
        workspace_id,
        release_id,
        &content_sha256,
        "workspace.python_capability_published",
    )
    .await?;
    Ok(Json(PythonCapabilityPublishResponse {
        release_id,
        package_id: request.slug.clone(),
        version: request.version,
        capability_root_id,
        server_name: request.server_name,
        skill_name: request.skill.name,
        content_sha256,
        written_files,
    }))
}

fn package_content_sha256(files: &BTreeMap<String, String>) -> String {
    let mut digest = Sha256::new();
    for (relative, content) in files {
        for value in [relative.as_bytes(), content.as_bytes()] {
            digest.update((value.len() as u64).to_be_bytes());
            digest.update(value);
        }
    }
    hex::encode(digest.finalize())
}

fn package_written_files(
    request: &PythonCapabilityPublishRequest,
    files: &BTreeMap<String, String>,
) -> Vec<String> {
    let mut written = files
        .keys()
        .map(|relative| format!("tools/{}/{}/{relative}", request.slug, request.version))
        .collect::<Vec<_>>();
    written.push(format!(
        "tools/{}/{}/.open-web-release.json",
        request.slug, request.version
    ));
    written.sort();
    written
}

fn publish_response(
    release_id: Uuid,
    request: &PythonCapabilityPublishRequest,
    capability_root_id: String,
    content_sha256: String,
    written_files: Vec<String>,
) -> PythonCapabilityPublishResponse {
    PythonCapabilityPublishResponse {
        release_id,
        package_id: request.slug.clone(),
        version: request.version.clone(),
        capability_root_id,
        server_name: request.server_name.clone(),
        skill_name: request.skill.name.clone(),
        content_sha256,
        written_files,
    }
}

async fn finalize_capability_release(
    state: &AppState,
    auth: &AuthenticatedUser,
    workspace_id: Uuid,
    release_id: Uuid,
    content_sha256: &str,
    action: &str,
) -> Result<(), ApiError> {
    let mut transaction = state.db.begin().await.map_err(database_error)?;
    let updated = sqlx::query(
        "UPDATE workspace_capability_package_releases \
         SET state = 'published', published_at = now(), updated_at = now() \
         WHERE id = $1 AND organization_id = $2 AND workspace_id = $3 \
           AND state = 'publishing' AND content_sha256 = $4",
    )
    .bind(release_id)
    .bind(auth.organization_id)
    .bind(workspace_id)
    .bind(content_sha256)
    .execute(&mut *transaction)
    .await
    .map_err(database_error)?;
    if updated.rows_affected() != 1 {
        transaction.rollback().await.ok();
        return Err(conflict(
            "Capability Release no longer has a publishable lifecycle state",
        ));
    }
    audit_capability_release(
        &mut transaction,
        auth,
        release_id,
        action,
        "success",
        json!({
            "workspaceId": workspace_id,
            "contentSha256": content_sha256,
        }),
    )
    .await?;
    transaction.commit().await.map_err(database_error)
}

async fn mark_capability_release_failed(
    state: &AppState,
    auth: &AuthenticatedUser,
    workspace_id: Uuid,
    release_id: Uuid,
    failure_code: &str,
) -> Result<(), ApiError> {
    let mut transaction = state.db.begin().await.map_err(database_error)?;
    sqlx::query(
        "UPDATE workspace_capability_package_releases \
         SET state = 'failed', failure_code = $4, updated_at = now() \
         WHERE id = $1 AND organization_id = $2 AND workspace_id = $3 \
           AND state = 'publishing'",
    )
    .bind(release_id)
    .bind(auth.organization_id)
    .bind(workspace_id)
    .bind(failure_code)
    .execute(&mut *transaction)
    .await
    .map_err(database_error)?;
    audit_capability_release(
        &mut transaction,
        auth,
        release_id,
        "workspace.python_capability_failed",
        "failure",
        json!({
            "workspaceId": workspace_id,
            "failureCode": failure_code,
        }),
    )
    .await?;
    transaction.commit().await.map_err(database_error)
}

async fn audit_capability_release(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    auth: &AuthenticatedUser,
    release_id: Uuid,
    action: &str,
    outcome: &str,
    metadata: Value,
) -> Result<(), ApiError> {
    sqlx::query(
        "INSERT INTO audit_log \
         (organization_id, actor_id, action, target_type, target_id, metadata, outcome) \
         VALUES ($1, $2, $3, 'workspace_capability_package_release', $4, $5, $6)",
    )
    .bind(auth.organization_id)
    .bind(auth.user_id)
    .bind(action)
    .bind(release_id)
    .bind(metadata)
    .bind(outcome)
    .execute(&mut **transaction)
    .await
    .map_err(database_error)?;
    Ok(())
}

fn capability_failure_code(error: &open_web_codex_git_runtime::GitRuntimeError) -> &'static str {
    use open_web_codex_git_runtime::GitRuntimeError;
    match error {
        GitRuntimeError::UnsafePath(_) => "unsafe_workspace_path",
        GitRuntimeError::Conflict(_) => "workspace_conflict",
        GitRuntimeError::InvalidSource(_) | GitRuntimeError::InvalidRef(_) => {
            "invalid_workspace_identity"
        }
        GitRuntimeError::Git { .. } => "workspace_git_failed",
        GitRuntimeError::Io { .. } => "workspace_io_failed",
        GitRuntimeError::UnsupportedImage(_) | GitRuntimeError::ImageTooLarge => {
            "unsupported_workspace_content"
        }
        GitRuntimeError::NoChanges => "workspace_no_changes",
    }
}

async fn validate_and_probe(
    request: &PythonCapabilityPublishRequest,
) -> PythonCapabilityValidationResult {
    let mut issues = validate_shape(request);
    if issues.is_empty() {
        if let Err(message) = run_protocol_probe(request, None, None).await {
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
    if request.idempotency_key.len() < 8
        || request.idempotency_key.len() > 128
        || request.idempotency_key.chars().any(char::is_control)
    {
        issues.push(issue(
            "invalid_idempotency_key",
            "Publication request identity is invalid",
        ));
    }
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
    if request.input_artifact_types.len() > 32
        || request.output_artifact_types.is_empty()
        || request.output_artifact_types.len() > 32
        || request
            .input_artifact_types
            .iter()
            .chain(request.output_artifact_types.iter())
            .any(|artifact| !valid_artifact_type(artifact))
    {
        issues.push(issue(
            "invalid_artifact_types",
            "Declare up to 32 valid input Artifact types and between 1 and 32 output types",
        ));
    }
    let mut input_artifact_types = HashSet::new();
    let mut output_artifact_types = HashSet::new();
    if request
        .input_artifact_types
        .iter()
        .any(|artifact| !input_artifact_types.insert(artifact.as_str()))
        || request
            .output_artifact_types
            .iter()
            .any(|artifact| !output_artifact_types.insert(artifact.as_str()))
    {
        issues.push(issue(
            "duplicate_artifact_types",
            "Artifact type declarations must be unique within inputs and outputs",
        ));
    }
    issues
}

async fn run_protocol_probe(
    request: &PythonCapabilityPublishRequest,
    tool_call: Option<(&str, &Value)>,
    authorized_workspace: Option<(Uuid, &FsPath)>,
) -> Result<Vec<Value>, String> {
    run_protocol_probe_with_timeout(
        request,
        tool_call,
        authorized_workspace,
        PROTOCOL_PROBE_TIMEOUT,
    )
    .await
}

async fn run_protocol_probe_with_timeout(
    request: &PythonCapabilityPublishRequest,
    tool_call: Option<(&str, &Value)>,
    authorized_workspace: Option<(Uuid, &FsPath)>,
    timeout: Duration,
) -> Result<Vec<Value>, String> {
    let directory =
        tempfile::tempdir().map_err(|_| "Unable to prepare Python MCP validation".to_string())?;
    let files = package_files(request)?;
    for (relative, content) in files {
        let target = directory.path().join(relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|_| "Unable to prepare Python MCP validation files".to_string())?;
        }
        std::fs::write(target, content)
            .map_err(|_| "Unable to prepare Python MCP validation files".to_string())?;
    }
    let mut command = Command::new("python3");
    command
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
        .stderr(std::process::Stdio::piped());
    command.kill_on_drop(true);
    if let Some((workspace_id, workspace_root)) = authorized_workspace {
        command
            .env("OPEN_WEB_CODEX_AUTHORIZED_WORKSPACE_ROOT", workspace_root)
            .env(
                "OPEN_WEB_CODEX_AUTHORIZED_WORKSPACE_ID",
                workspace_id.to_string(),
            );
    }
    let mut child = command
        .spawn()
        .map_err(|_| "Unable to start Python MCP validation".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or("Python MCP stdout was unavailable")?;
    let stderr = child
        .stderr
        .take()
        .ok_or("Python MCP stderr was unavailable")?;
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
        .map_err(|_| "Python MCP input could not be delivered".to_string())?;
    let execution = tokio::time::timeout(timeout, async {
        tokio::try_join!(
            async {
                child
                    .wait()
                    .await
                    .map_err(|_| "Python MCP process could not be observed".to_string())
            },
            read_bounded_process_output(stdout, "stdout"),
            read_bounded_process_output(stderr, "stderr"),
        )
    })
    .await;
    let (status, stdout, stderr) = match execution {
        Ok(Ok(output)) => output,
        Ok(Err(message)) => {
            let _ = child.kill().await;
            return Err(message);
        }
        Err(_) => {
            let _ = child.kill().await;
            return Err("Python MCP validation timed out".to_string());
        }
    };
    if !status.success() {
        if let Some(tool_name) = missing_declared_tool(&stderr, request) {
            return Err(format!(
                "Python MCP implementation is missing declared Tool '{tool_name}'"
            ));
        }
        return Err("Python MCP exited before validation completed".to_string());
    }
    let responses = String::from_utf8(stdout)
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

async fn read_bounded_process_output<R>(reader: R, stream: &str) -> Result<Vec<u8>, String>
where
    R: AsyncRead + Unpin,
{
    let mut output = Vec::new();
    reader
        .take((MAX_PROCESS_OUTPUT_BYTES + 1) as u64)
        .read_to_end(&mut output)
        .await
        .map_err(|_| format!("Python MCP {stream} could not be read"))?;
    if output.len() > MAX_PROCESS_OUTPUT_BYTES {
        return Err(format!(
            "Python MCP {stream} exceeded the 256 KiB validation limit"
        ));
    }
    Ok(output)
}

fn missing_declared_tool(
    stderr: &[u8],
    request: &PythonCapabilityPublishRequest,
) -> Option<String> {
    const PREFIX: &str = "OPEN_WEB_CODEX_MISSING_TOOLS=";
    let stderr = std::str::from_utf8(stderr).ok()?;
    let declared = request
        .tools
        .iter()
        .map(|tool| tool.name.as_str())
        .collect::<HashSet<_>>();
    stderr
        .lines()
        .find_map(|line| line.strip_prefix(PREFIX))
        .and_then(|names| {
            names
                .split(',')
                .find(|name| declared.contains(name))
                .map(str::to_string)
        })
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
        "openWebCodex": {
            "inputArtifactTypes": request.input_artifact_types,
            "outputArtifactTypes": request.output_artifact_types,
        },
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
    let launcher = "#!/usr/bin/env sh\nset -eu\nroot=$(CDPATH= cd -- \"$(dirname -- \"$0\")/..\" && pwd)\nexec env -i PATH=\"${PATH:-/usr/bin:/bin}\" HOME=\"$root\" PYTHONDONTWRITEBYTECODE=1 python3 \"$root/server.py\"\n";
    let server = generated_server(&tools, &request.output_artifact_types, &request.server_name)?;
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
        (
            "workspace_data.py".to_string(),
            include_str!("python_workspace_data.py").to_string(),
        ),
        ("pyproject.toml".to_string(), pyproject),
        ("server.py".to_string(), server),
        (format!("skills/{}/SKILL.md", request.skill.name), skill),
    ]))
}

fn generated_server(
    tools: &[Value],
    output_artifact_types: &[String],
    server_name: &str,
) -> Result<String, String> {
    let tools_json = serde_json::to_string(tools).map_err(|error| error.to_string())?;
    let tools_literal = serde_json::to_string(&tools_json).map_err(|error| error.to_string())?;
    let artifact_types_json =
        serde_json::to_string(output_artifact_types).map_err(|error| error.to_string())?;
    let artifact_types_literal =
        serde_json::to_string(&artifact_types_json).map_err(|error| error.to_string())?;
    let server_name_literal =
        serde_json::to_string(server_name).map_err(|error| error.to_string())?;
    Ok(format!(
        r#"#!/usr/bin/env python3
import hashlib
import importlib
import json
import sys

implementation = importlib.import_module("implementation")
TOOLS = json.loads({tools_literal})
TOOL_NAMES = {{tool["name"] for tool in TOOLS}}
OUTPUT_ARTIFACT_TYPES = set(json.loads({artifact_types_literal}))
SERVER_NAME = {server_name_literal}
RESOURCES = {{}}
missing_tools = [
    tool["name"]
    for tool in TOOLS
    if not callable(getattr(implementation, tool["name"], None))
]
if missing_tools:
    sys.stderr.write("OPEN_WEB_CODEX_MISSING_TOOLS=" + ",".join(missing_tools) + "\n")
    raise SystemExit(64)

def send(payload):
    sys.stdout.write(json.dumps(payload, separators=(",", ":")) + "\n")
    sys.stdout.flush()

def result(request_id, value):
    send({{"jsonrpc": "2.0", "id": request_id, "result": value}})

def error(request_id, code, message):
    send({{"jsonrpc": "2.0", "id": request_id, "error": {{"code": code, "message": message}}}})

def publish_resource(value):
    if not isinstance(value, dict):
        return value, None
    schema = value.get("schema_version")
    if schema not in OUTPUT_ARTIFACT_TYPES:
        return value, None
    if "resource_name" in value or "data_ref" in value:
        raise ValueError("Tool results must not define reserved Resource identity fields")
    text = json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
    encoded = text.encode("utf-8")
    digest = hashlib.sha256(encoded).hexdigest()
    resource_name = f"{{schema[:96]}}-{{digest[:24]}}"
    uri = f"open-web-python://resources/{{resource_name}}"
    RESOURCES[uri] = text
    structured = dict(value)
    structured["resource_name"] = resource_name
    structured["data_ref"] = {{
        "type": "mcp_resource",
        "server": SERVER_NAME,
        "uri": uri,
        "format": "json",
        "resource_schema": schema,
    }}
    link = {{
        "type": "resource_link",
        "name": resource_name,
        "title": schema,
        "uri": uri,
        "mimeType": "application/json",
        "size": len(encoded),
    }}
    embedded = {{
        "type": "resource",
        "resource": {{
            "uri": uri,
            "mimeType": "application/json",
            "text": text,
        }},
    }}
    return structured, (link, embedded)

for line in sys.stdin:
    if not line.strip():
        continue
    request = json.loads(line)
    request_id = request.get("id")
    method = request.get("method")
    if method == "initialize":
        result(request_id, {{
            "protocolVersion": request.get("params", {{}}).get("protocolVersion", "2025-06-18"),
            "capabilities": {{"tools": {{}}, "resources": {{}}}},
            "serverInfo": {{"name": "user-python-capability", "version": "1"}},
        }})
    elif method == "tools/list":
        result(request_id, {{"tools": TOOLS}})
    elif method == "resources/list":
        result(request_id, {{"resources": []}})
    elif method == "resources/read":
        uri = request.get("params", {{}}).get("uri")
        if uri not in RESOURCES:
            error(request_id, -32002, "Resource not found")
        else:
            result(request_id, {{
                "contents": [{{
                    "uri": uri,
                    "mimeType": "application/json",
                    "text": RESOURCES[uri],
                }}],
            }})
    elif method == "tools/call":
        params = request.get("params", {{}})
        name = params.get("name")
        if name not in TOOL_NAMES:
            result(request_id, {{
                "content": [{{"type": "text", "text": "Tool is not declared"}}],
                "isError": True,
            }})
            continue
        try:
            value = getattr(implementation, name)(params.get("arguments", {{}}))
            structured, resources = publish_resource(value)
            content = [{{"type": "text", "text": json.dumps(structured, ensure_ascii=False)}}]
            if resources is not None:
                content.extend(resources)
            result(request_id, {{
                "content": content,
                "structuredContent": structured,
                "isError": False,
            }})
        except Exception:
            result(request_id, {{
                "content": [{{"type": "text", "text": "Tool execution failed"}}],
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

fn valid_artifact_type(value: &str) -> bool {
    value.len() >= 3
        && value.len() <= 128
        && value.contains('.')
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-' | b'_')
        })
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

fn bad_request(message: &str) -> ApiError {
    (
        StatusCode::BAD_REQUEST,
        Json(PlatformError::bad_request(message)),
    )
}

fn conflict(message: &str) -> ApiError {
    (
        StatusCode::CONFLICT,
        Json(PlatformError::bad_request(message)),
    )
}

fn database_error(_error: sqlx::Error) -> ApiError {
    database_error_message()
}

fn database_error_message() -> ApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(PlatformError::internal(
            "Capability publication database operation failed",
        )),
    )
}

fn database_conflict(error: sqlx::Error) -> ApiError {
    if error
        .as_database_error()
        .is_some_and(|database| database.is_unique_violation())
    {
        conflict("Capability package identity or idempotency key already exists")
    } else {
        database_error(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use open_web_codex_platform_contracts::{PythonCapabilitySkill, PythonCapabilityTool};

    fn stock_capability() -> PythonCapabilityPublishRequest {
        PythonCapabilityPublishRequest {
            idempotency_key: "publish-stock-history-1".to_string(),
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
            input_artifact_types: Vec::new(),
            output_artifact_types: vec!["stock-history.report.v1".to_string()],
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
            None,
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
    async fn generated_mcp_rejects_undeclared_tools_and_hides_failures() {
        let mut capability = stock_capability();
        capability.python_source.push_str(
            r#"

def undeclared_helper(arguments):
    return {"secret": "must-not-be-returned"}
"#,
        );
        let undeclared =
            run_protocol_probe(&capability, Some(("undeclared_helper", &json!({}))), None)
                .await
                .expect("undeclared Tool response");
        assert_eq!(
            undeclared
                .last()
                .expect("Tool response")
                .pointer("/result/isError"),
            Some(&json!(true))
        );
        assert_eq!(
            undeclared
                .last()
                .expect("Tool response")
                .pointer("/result/content/0/text"),
            Some(&json!("Tool is not declared"))
        );

        capability.python_source = r#"
def lookup_stock(arguments):
    raise RuntimeError("/Users/private/credential sk-not-browser-visible")

def get_price_history(arguments):
    return []
"#
        .trim_start()
        .to_string();
        let failed = run_protocol_probe(&capability, Some(("lookup_stock", &json!({}))), None)
            .await
            .expect("failed Tool response");
        let encoded = serde_json::to_string(failed.last().expect("Tool response"))
            .expect("encoded Tool response");
        assert!(encoded.contains("Tool execution failed"));
        assert!(!encoded.contains("/Users/private"));
        assert!(!encoded.contains("sk-not-browser-visible"));
    }

    #[tokio::test]
    async fn protocol_probe_bounds_output_and_terminates_timeouts() {
        let mut capability = stock_capability();
        capability.python_source = r#"
import sys

def lookup_stock(arguments):
    sys.stderr.write("x" * 300000)
    sys.stderr.flush()
    while True:
        pass

def get_price_history(arguments):
    return []
"#
        .trim_start()
        .to_string();
        let output_error = run_protocol_probe_with_timeout(
            &capability,
            Some(("lookup_stock", &json!({}))),
            None,
            Duration::from_secs(1),
        )
        .await
        .expect_err("oversized stderr must terminate the probe");
        assert_eq!(
            output_error,
            "Python MCP stderr exceeded the 256 KiB validation limit"
        );

        capability.python_source = r#"
def lookup_stock(arguments):
    while True:
        pass

def get_price_history(arguments):
    return []
"#
        .trim_start()
        .to_string();
        let timeout_error = run_protocol_probe_with_timeout(
            &capability,
            Some(("lookup_stock", &json!({}))),
            None,
            Duration::from_millis(50),
        )
        .await
        .expect_err("hung Tool must terminate at the timeout");
        assert_eq!(timeout_error, "Python MCP validation timed out");
    }

    #[tokio::test]
    async fn tool_probe_reads_one_exact_authorized_dataset_release() {
        let workspace = tempfile::tempdir().expect("workspace");
        let release_id = Uuid::now_v7();
        let dataset_id = "delivery-audit";
        let version = "1.0.0";
        let display_name = "Delivery audit";
        let description = "Synthetic delivery rows.";
        let files_root = workspace
            .path()
            .join("datasets")
            .join(dataset_id)
            .join(version)
            .join("files");
        std::fs::create_dir_all(&files_root).expect("dataset files");
        let file_bytes = b"shipment_id\nS001\n";
        std::fs::write(files_root.join("deliveries.csv"), file_bytes).expect("dataset file");
        let file_sha256 = hex::encode(Sha256::digest(file_bytes));
        let descriptor = json!({
            "logicalName": "deliveries.csv",
            "role": "delivery_records",
            "mediaType": "text/csv",
            "byteSize": file_bytes.len(),
            "contentSha256": file_sha256,
            "relativePath": "files/deliveries.csv",
        });
        let mut release_digest = Sha256::new();
        let byte_size = file_bytes.len().to_string();
        for value in [
            "workspace.dataset-release.v1",
            dataset_id,
            version,
            display_name,
            description,
            "deliveries.csv",
            "delivery_records",
            "text/csv",
            byte_size.as_str(),
            descriptor["contentSha256"].as_str().expect("file hash"),
        ] {
            let encoded = value.as_bytes();
            release_digest.update((encoded.len() as u64).to_be_bytes());
            release_digest.update(encoded);
        }
        let content_sha256 = hex::encode(release_digest.finalize());
        let manifest = json!({
            "schemaVersion": "workspace.dataset-release.v1",
            "releaseId": release_id,
            "datasetId": dataset_id,
            "version": version,
            "displayName": display_name,
            "description": description,
            "contentSha256": content_sha256,
            "files": [descriptor],
        });
        std::fs::write(
            files_root
                .parent()
                .expect("release root")
                .join("release.json"),
            serde_json::to_vec(&manifest).expect("manifest"),
        )
        .expect("release manifest");

        let mut capability = stock_capability();
        capability.slug = "delivery-audit".to_string();
        capability.server_name = "delivery_audit".to_string();
        capability.python_source = r#"
from workspace_data import load_dataset_release

def inspect_release(arguments):
    release = load_dataset_release(
        workspace_id=arguments["workspace_id"],
        release_id=arguments["release_id"],
        dataset_id=arguments["dataset_id"],
        version=arguments["version"],
        content_sha256=arguments["content_sha256"],
    )
    source = release.require_file("deliveries.csv")
    return {
        "schema_version": "delivery_audit_report.v1",
        "release_id": release.release_id,
        "logical_name": source.logical_name,
        "byte_size": source.byte_size,
    }
"#
        .trim_start()
        .to_string();
        capability.tools = vec![PythonCapabilityTool {
            name: "inspect_release".to_string(),
            description: "Inspect one exact Dataset Release.".to_string(),
            input_schema: json!({"type": "object"}),
        }];
        capability.output_artifact_types = vec!["delivery_audit_report.v1".to_string()];
        let workspace_id = Uuid::now_v7();
        let arguments = json!({
            "workspace_id": workspace_id,
            "release_id": release_id,
            "dataset_id": dataset_id,
            "version": version,
            "content_sha256": content_sha256,
        });
        let responses = run_protocol_probe(
            &capability,
            Some(("inspect_release", &arguments)),
            Some((workspace_id, workspace.path())),
        )
        .await
        .expect("dataset Tool probe");
        assert_eq!(
            responses
                .last()
                .expect("Tool response")
                .pointer("/result/structuredContent/logical_name"),
            Some(&json!("deliveries.csv"))
        );
        assert_eq!(
            responses
                .last()
                .expect("Tool response")
                .pointer("/result/structuredContent/data_ref/resource_schema"),
            Some(&json!("delivery_audit_report.v1"))
        );
        assert_eq!(
            responses
                .last()
                .expect("Tool response")
                .pointer("/result/content/1/type"),
            Some(&json!("resource_link"))
        );

        let wrong_workspace_arguments = json!({
            "workspace_id": Uuid::now_v7(),
            "release_id": release_id,
            "dataset_id": dataset_id,
            "version": version,
            "content_sha256": content_sha256,
        });
        let rejected = run_protocol_probe(
            &capability,
            Some(("inspect_release", &wrong_workspace_arguments)),
            Some((workspace_id, workspace.path())),
        )
        .await
        .expect("rejected dataset Tool probe");
        assert_eq!(
            rejected
                .last()
                .expect("Tool error response")
                .pointer("/result/isError"),
            Some(&json!(true))
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

    #[test]
    fn permits_one_artifact_type_as_both_input_and_output() {
        let mut capability = stock_capability();
        capability.input_artifact_types = vec!["stock-history.report.v1".to_string()];
        assert!(validate_shape(&capability).is_empty());
    }
}
