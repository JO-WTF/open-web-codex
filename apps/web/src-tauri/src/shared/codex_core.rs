use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::sync::oneshot::error::TryRecvError;
use tokio::sync::{oneshot, Mutex};
use tokio::time::timeout;
use tokio::time::Instant;

use crate::backend::app_server::WorkspaceSession;
use crate::codex::config as codex_config;
use crate::codex::home::{resolve_default_codex_home, resolve_workspace_codex_home};
use crate::rules;
use crate::shared::account::{build_account_response, read_auth_account};
use crate::types::WorkspaceEntry;

const LOGIN_START_TIMEOUT: Duration = Duration::from_secs(30);
#[allow(dead_code)]
const MAX_INLINE_IMAGE_BYTES: u64 = 50 * 1024 * 1024;
const THREAD_LIST_SOURCE_KINDS: &[&str] = &[
    "cli",
    "vscode",
    "appServer",
    "subAgentReview",
    "subAgentCompact",
    "subAgentThreadSpawn",
    "unknown",
];

#[allow(dead_code)]
fn image_extension_for_path(path: &str) -> Option<String> {
    Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
}

#[allow(dead_code)]
fn image_mime_type_for_path(path: &str) -> Option<&'static str> {
    let extension = image_extension_for_path(path)?;
    match extension.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "bmp" => Some("image/bmp"),
        "tiff" | "tif" => Some("image/tiff"),
        _ => None,
    }
}

#[allow(dead_code)]
fn should_inline_image_path_for_codex(path: &str) -> bool {
    matches!(
        image_extension_for_path(path).as_deref(),
        Some("heic") | Some("heif")
    )
}

#[cfg(target_os = "macos")]
fn temp_converted_image_path(path: &str, extension: &str) -> PathBuf {
    let stem = Path::new(path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("image");
    let safe_stem = stem
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>();
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis())
        .unwrap_or_default();
    std::env::temp_dir().join(format!("codex-monitor-image-{safe_stem}-{ts}.{extension}"))
}

#[cfg(target_os = "macos")]
fn convert_heif_image_to_jpeg_bytes(path: &str) -> Result<Vec<u8>, String> {
    let output_path = temp_converted_image_path(path, "jpg");
    let status = std::process::Command::new("/usr/bin/sips")
        .args(["-s", "format", "jpeg"])
        .arg(path)
        .arg("--out")
        .arg(&output_path)
        .status()
        .map_err(|err| format!("Failed to launch HEIC/HEIF conversion for {path}: {err}"))?;
    if !status.success() {
        let _ = std::fs::remove_file(&output_path);
        return Err(format!(
            "Failed to convert HEIC/HEIF image into a Codex-compatible JPEG: {path}"
        ));
    }
    let bytes = std::fs::read(&output_path).map_err(|err| {
        format!(
            "Failed to read converted JPEG for {path} at {}: {err}",
            output_path.display()
        )
    })?;
    let _ = std::fs::remove_file(&output_path);
    if bytes.is_empty() {
        return Err(format!(
            "Converted JPEG is empty after HEIC/HEIF conversion: {path}"
        ));
    }
    Ok(bytes)
}

#[allow(dead_code)]
pub(crate) fn normalize_file_path(raw: &str) -> String {
    let path = raw.trim();
    let file_uri_path = path
        .strip_prefix("file://localhost")
        .or_else(|| path.strip_prefix("file://"));
    let Some(path) = file_uri_path else {
        return path.to_string();
    };

    let mut decoded = Vec::with_capacity(path.len());
    let bytes = path.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hi = bytes[index + 1];
            let lo = bytes[index + 2];
            let hi_value = match hi {
                b'0'..=b'9' => Some(hi - b'0'),
                b'a'..=b'f' => Some(hi - b'a' + 10),
                b'A'..=b'F' => Some(hi - b'A' + 10),
                _ => None,
            };
            let lo_value = match lo {
                b'0'..=b'9' => Some(lo - b'0'),
                b'a'..=b'f' => Some(lo - b'a' + 10),
                b'A'..=b'F' => Some(lo - b'A' + 10),
                _ => None,
            };
            if let (Some(hi_nibble), Some(lo_nibble)) = (hi_value, lo_value) {
                decoded.push((hi_nibble << 4) | lo_nibble);
                index += 3;
                continue;
            }
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

#[allow(dead_code)]
pub(crate) fn read_image_as_data_url_core(path: &str) -> Result<String, String> {
    let trimmed_path = normalize_file_path(path);
    if trimmed_path.is_empty() {
        return Err("Image path is required".to_string());
    }
    if should_inline_image_path_for_codex(&trimmed_path) {
        #[cfg(target_os = "macos")]
        {
            let encoded = STANDARD.encode(convert_heif_image_to_jpeg_bytes(&trimmed_path)?);
            return Ok(format!("data:image/jpeg;base64,{encoded}"));
        }
        #[cfg(not(target_os = "macos"))]
        {
            return Err(format!(
                "HEIC/HEIF images are not supported on this platform; convert to JPEG or PNG first: {trimmed_path}"
            ));
        }
    }
    let mime_type = image_mime_type_for_path(&trimmed_path).ok_or_else(|| {
        format!("Unsupported or missing image extension for path: {trimmed_path}")
    })?;
    let metadata = std::fs::symlink_metadata(&trimmed_path)
        .map_err(|err| format!("Failed to stat image file at {trimmed_path}: {err}"))?;
    if metadata.file_type().is_symlink() {
        return Err(format!("Image path must not be a symlink: {trimmed_path}"));
    }
    if !metadata.is_file() {
        return Err(format!("Image path is not a file: {trimmed_path}"));
    }
    if metadata.len() > MAX_INLINE_IMAGE_BYTES {
        return Err(format!(
            "Image file exceeds maximum size of {MAX_INLINE_IMAGE_BYTES} bytes: {trimmed_path}"
        ));
    }
    let bytes = std::fs::read(&trimmed_path)
        .map_err(|err| format!("Failed to read image file at {trimmed_path}: {err}"))?;
    if bytes.is_empty() {
        return Err(format!("Image file is empty: {trimmed_path}"));
    }
    let encoded = STANDARD.encode(bytes);
    Ok(format!("data:{mime_type};base64,{encoded}"))
}

pub(crate) enum CodexLoginCancelState {
    PendingStart(oneshot::Sender<()>),
    LoginId(String),
}

async fn get_session_clone(
    sessions: &Mutex<HashMap<String, Arc<WorkspaceSession>>>,
    workspace_id: &str,
) -> Result<Arc<WorkspaceSession>, String> {
    let sessions = sessions.lock().await;
    sessions
        .get(workspace_id)
        .cloned()
        .ok_or_else(|| "workspace not connected".to_string())
}

async fn resolve_workspace_and_parent(
    workspaces: &Mutex<HashMap<String, WorkspaceEntry>>,
    workspace_id: &str,
) -> Result<(WorkspaceEntry, Option<WorkspaceEntry>), String> {
    let workspaces = workspaces.lock().await;
    let entry = workspaces
        .get(workspace_id)
        .cloned()
        .ok_or_else(|| "workspace not found".to_string())?;
    let parent_entry = entry
        .parent_id
        .as_ref()
        .and_then(|parent_id| workspaces.get(parent_id))
        .cloned();
    Ok((entry, parent_entry))
}

async fn resolve_codex_home_for_workspace_core(
    workspaces: &Mutex<HashMap<String, WorkspaceEntry>>,
    workspace_id: &str,
) -> Result<PathBuf, String> {
    let (entry, parent_entry) = resolve_workspace_and_parent(workspaces, workspace_id).await?;
    resolve_workspace_codex_home(&entry, parent_entry.as_ref())
        .or_else(resolve_default_codex_home)
        .ok_or_else(|| "Unable to resolve CODEX_HOME".to_string())
}

async fn resolve_workspace_path_core(
    workspaces: &Mutex<HashMap<String, WorkspaceEntry>>,
    workspace_id: &str,
) -> Result<String, String> {
    let workspaces = workspaces.lock().await;
    let entry = workspaces
        .get(workspace_id)
        .ok_or_else(|| "workspace not found".to_string())?;
    Ok(entry.path.clone())
}

pub(crate) async fn start_thread_core(
    sessions: &Mutex<HashMap<String, Arc<WorkspaceSession>>>,
    workspaces: &Mutex<HashMap<String, WorkspaceEntry>>,
    workspace_id: String,
) -> Result<Value, String> {
    let session = get_session_clone(sessions, &workspace_id).await?;
    let workspace_path = resolve_workspace_path_core(workspaces, &workspace_id).await?;
    let mut params = json!({
        "cwd": workspace_path,
        "approvalPolicy": "on-request"
    });
    if let Some(provider_id) = current_model_provider_id(&session, &workspace_id).await {
        params["modelProvider"] = json!(provider_id);
    }
    session
        .send_request_for_workspace(&workspace_id, "thread/start", params)
        .await
}

pub(crate) async fn resume_thread_core(
    sessions: &Mutex<HashMap<String, Arc<WorkspaceSession>>>,
    workspace_id: String,
    thread_id: String,
) -> Result<Value, String> {
    let session = get_session_clone(sessions, &workspace_id).await?;
    let mut params = json!({ "threadId": thread_id });
    if let Some(provider_id) = current_model_provider_id(&session, &workspace_id).await {
        params["modelProvider"] = json!(provider_id);
    }
    session
        .send_request_for_workspace(&workspace_id, "thread/resume", params)
        .await
}

pub(crate) async fn read_thread_core(
    sessions: &Mutex<HashMap<String, Arc<WorkspaceSession>>>,
    workspace_id: String,
    thread_id: String,
) -> Result<Value, String> {
    let session = get_session_clone(sessions, &workspace_id).await?;
    let params = json!({
        "threadId": thread_id,
        "includeTurns": true
    });
    session
        .send_request_for_workspace(&workspace_id, "thread/read", params)
        .await
}

pub(crate) async fn list_thread_turns_core(
    sessions: &Mutex<HashMap<String, Arc<WorkspaceSession>>>,
    workspace_id: String,
    thread_id: String,
    cursor: Option<String>,
) -> Result<Value, String> {
    let session = get_session_clone(sessions, &workspace_id).await?;
    let params = json!({
        "threadId": thread_id,
        "cursor": cursor,
        "limit": 100,
        "sortDirection": "desc",
        "itemsView": "full"
    });
    session
        .send_request_for_workspace(&workspace_id, "thread/turns/list", params)
        .await
}

pub(crate) async fn thread_live_subscribe_core(
    sessions: &Mutex<HashMap<String, Arc<WorkspaceSession>>>,
    workspace_id: String,
    thread_id: String,
) -> Result<(), String> {
    if thread_id.trim().is_empty() {
        return Err("threadId is required".to_string());
    }
    let _ = get_session_clone(sessions, &workspace_id).await?;
    Ok(())
}

pub(crate) async fn thread_live_unsubscribe_core(
    sessions: &Mutex<HashMap<String, Arc<WorkspaceSession>>>,
    workspace_id: String,
    thread_id: String,
) -> Result<(), String> {
    if thread_id.trim().is_empty() {
        return Err("threadId is required".to_string());
    }
    let _ = get_session_clone(sessions, &workspace_id).await?;
    Ok(())
}

pub(crate) async fn fork_thread_core(
    sessions: &Mutex<HashMap<String, Arc<WorkspaceSession>>>,
    workspace_id: String,
    thread_id: String,
) -> Result<Value, String> {
    let session = get_session_clone(sessions, &workspace_id).await?;
    let params = json!({ "threadId": thread_id });
    session
        .send_request_for_workspace(&workspace_id, "thread/fork", params)
        .await
}

pub(crate) async fn list_threads_core(
    sessions: &Mutex<HashMap<String, Arc<WorkspaceSession>>>,
    workspace_id: String,
    cursor: Option<String>,
    limit: Option<u32>,
    sort_key: Option<String>,
) -> Result<Value, String> {
    let session = get_session_clone(sessions, &workspace_id).await?;
    let params = json!({
        "cursor": cursor,
        "limit": limit,
        "sortKey": sort_key,
        // Keep interactive and sub-agent sessions visible across CLI versions so
        // thread/list refreshes do not drop valid historical conversations.
        // Intentionally exclude generic "subAgent" so parentless internal jobs
        // (for example memory consolidation) do not leak back into app state.
        "sourceKinds": THREAD_LIST_SOURCE_KINDS
    });
    session
        .send_request_for_workspace(&workspace_id, "thread/list", params)
        .await
}

pub(crate) async fn list_mcp_server_status_core(
    sessions: &Mutex<HashMap<String, Arc<WorkspaceSession>>>,
    workspace_id: String,
    cursor: Option<String>,
    limit: Option<u32>,
) -> Result<Value, String> {
    let session = get_session_clone(sessions, &workspace_id).await?;
    let params = json!({ "cursor": cursor, "limit": limit });
    session
        .send_request_for_workspace(&workspace_id, "mcpServerStatus/list", params)
        .await
}

pub(crate) async fn archive_thread_core(
    sessions: &Mutex<HashMap<String, Arc<WorkspaceSession>>>,
    workspace_id: String,
    thread_id: String,
) -> Result<Value, String> {
    let session = get_session_clone(sessions, &workspace_id).await?;
    let params = json!({ "threadId": thread_id });
    session
        .send_request_for_workspace(&workspace_id, "thread/archive", params)
        .await
}

pub(crate) async fn compact_thread_core(
    sessions: &Mutex<HashMap<String, Arc<WorkspaceSession>>>,
    workspace_id: String,
    thread_id: String,
) -> Result<Value, String> {
    let session = get_session_clone(sessions, &workspace_id).await?;
    let params = json!({ "threadId": thread_id });
    session
        .send_request_for_workspace(&workspace_id, "thread/compact/start", params)
        .await
}

pub(crate) async fn set_thread_name_core(
    sessions: &Mutex<HashMap<String, Arc<WorkspaceSession>>>,
    workspace_id: String,
    thread_id: String,
    name: String,
) -> Result<Value, String> {
    let session = get_session_clone(sessions, &workspace_id).await?;
    let params = json!({ "threadId": thread_id, "name": name });
    session
        .send_request_for_workspace(&workspace_id, "thread/name/set", params)
        .await
}

fn build_turn_input_items(
    text: String,
    images: Option<Vec<String>>,
    app_mentions: Option<Vec<Value>>,
) -> Result<Vec<Value>, String> {
    let trimmed_text = text.trim();
    let mut input: Vec<Value> = Vec::new();
    if !trimmed_text.is_empty() {
        input.push(json!({ "type": "text", "text": trimmed_text }));
    }
    if let Some(paths) = images {
        for path in paths {
            let trimmed = path.trim();
            if trimmed.is_empty() {
                continue;
            }
            if trimmed.starts_with("data:")
                || trimmed.starts_with("http://")
                || trimmed.starts_with("https://")
            {
                input.push(json!({ "type": "image", "url": trimmed }));
            } else if should_inline_image_path_for_codex(trimmed) {
                input.push(json!({
                    "type": "image",
                    "url": read_image_as_data_url_core(trimmed)?,
                }));
            } else {
                input.push(json!({ "type": "localImage", "path": trimmed }));
            }
        }
    }
    if let Some(mentions) = app_mentions {
        let mut seen_paths: HashSet<String> = HashSet::new();
        for mention in mentions {
            let object = mention
                .as_object()
                .ok_or_else(|| "invalid app mention payload".to_string())?;
            let name = object
                .get("name")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "invalid app mention name".to_string())?;
            let path = object
                .get("path")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "invalid app mention path".to_string())?;
            if !path.starts_with("app://") || path.len() <= "app://".len() {
                return Err("invalid app mention path".to_string());
            }
            if !seen_paths.insert(path.to_string()) {
                continue;
            }
            input.push(json!({ "type": "mention", "name": name, "path": path }));
        }
    }
    if input.is_empty() {
        return Err("empty user message".to_string());
    }
    Ok(input)
}

pub(crate) fn insert_optional_nullable_string(
    params: &mut Map<String, Value>,
    key: &str,
    value: Option<Option<String>>,
) {
    if let Some(value) = value {
        params.insert(key.to_string(), json!(value));
    }
}

pub(crate) async fn send_user_message_core(
    sessions: &Mutex<HashMap<String, Arc<WorkspaceSession>>>,
    workspaces: &Mutex<HashMap<String, WorkspaceEntry>>,
    workspace_id: String,
    thread_id: String,
    text: String,
    model: Option<String>,
    effort: Option<String>,
    service_tier: Option<Option<String>>,
    access_mode: Option<String>,
    images: Option<Vec<String>>,
    app_mentions: Option<Vec<Value>>,
    collaboration_mode: Option<Value>,
) -> Result<Value, String> {
    let session = get_session_clone(sessions, &workspace_id).await?;
    let workspace_path = resolve_workspace_path_core(workspaces, &workspace_id).await?;
    let access_mode = access_mode.unwrap_or_else(|| "current".to_string());
    let sandbox_policy = match access_mode.as_str() {
        "full-access" => json!({ "type": "dangerFullAccess" }),
        "read-only" => json!({ "type": "readOnly" }),
        _ => json!({
            "type": "workspaceWrite",
            "writableRoots": [workspace_path.clone()],
            "networkAccess": true
        }),
    };

    let approval_policy = if access_mode == "full-access" {
        "never"
    } else {
        "on-request"
    };

    let input = build_turn_input_items(text, images, app_mentions)?;

    let mut params = Map::new();
    params.insert("threadId".to_string(), json!(thread_id));
    params.insert("input".to_string(), json!(input));
    params.insert("cwd".to_string(), json!(workspace_path));
    params.insert("approvalPolicy".to_string(), json!(approval_policy));
    params.insert("sandboxPolicy".to_string(), json!(sandbox_policy));
    params.insert("model".to_string(), json!(model));
    if let Some(provider_id) = current_model_provider_id(&session, &workspace_id).await {
        params.insert("modelProvider".to_string(), json!(provider_id));
    }
    params.insert("effort".to_string(), json!(effort));
    insert_optional_nullable_string(&mut params, "serviceTier", service_tier);
    if let Some(mode) = collaboration_mode {
        if !mode.is_null() {
            params.insert("collaborationMode".to_string(), mode);
        }
    }
    session
        .send_request_for_workspace(&workspace_id, "turn/start", Value::Object(params))
        .await
}

pub(crate) async fn turn_steer_core(
    sessions: &Mutex<HashMap<String, Arc<WorkspaceSession>>>,
    workspace_id: String,
    thread_id: String,
    turn_id: String,
    text: String,
    images: Option<Vec<String>>,
    app_mentions: Option<Vec<Value>>,
) -> Result<Value, String> {
    if turn_id.trim().is_empty() {
        return Err("missing active turn id".to_string());
    }
    let session = get_session_clone(sessions, &workspace_id).await?;
    let input = build_turn_input_items(text, images, app_mentions)?;
    let params = json!({
        "threadId": thread_id,
        "expectedTurnId": turn_id,
        "input": input
    });
    session
        .send_request_for_workspace(&workspace_id, "turn/steer", params)
        .await
}

pub(crate) async fn collaboration_mode_list_core(
    sessions: &Mutex<HashMap<String, Arc<WorkspaceSession>>>,
    workspace_id: String,
) -> Result<Value, String> {
    let session = get_session_clone(sessions, &workspace_id).await?;
    session
        .send_request_for_workspace(&workspace_id, "collaborationMode/list", json!({}))
        .await
}

pub(crate) async fn turn_interrupt_core(
    sessions: &Mutex<HashMap<String, Arc<WorkspaceSession>>>,
    workspace_id: String,
    thread_id: String,
    turn_id: String,
) -> Result<Value, String> {
    let session = get_session_clone(sessions, &workspace_id).await?;
    let params = json!({ "threadId": thread_id, "turnId": turn_id });
    session
        .send_request_for_workspace(&workspace_id, "turn/interrupt", params)
        .await
}

pub(crate) async fn start_review_core(
    sessions: &Mutex<HashMap<String, Arc<WorkspaceSession>>>,
    workspace_id: String,
    thread_id: String,
    target: Value,
    delivery: Option<String>,
) -> Result<Value, String> {
    let session = get_session_clone(sessions, &workspace_id).await?;
    let mut params = Map::new();
    params.insert("threadId".to_string(), json!(thread_id));
    params.insert("target".to_string(), target);
    if let Some(delivery) = delivery {
        params.insert("delivery".to_string(), json!(delivery));
    }
    session
        .send_request_for_workspace(&workspace_id, "review/start", Value::Object(params))
        .await
}

pub(crate) async fn model_list_core(
    sessions: &Mutex<HashMap<String, Arc<WorkspaceSession>>>,
    workspace_id: String,
    force_refresh: bool,
) -> Result<Value, String> {
    let session = get_session_clone(sessions, &workspace_id).await?;
    let mut params = Map::new();
    if force_refresh {
        params.insert("forceRefresh".to_string(), json!(true));
    }
    session
        .send_request_for_workspace(&workspace_id, "model/list", Value::Object(params))
        .await
}

pub(crate) async fn thread_settings_update_core(
    sessions: &Mutex<HashMap<String, Arc<WorkspaceSession>>>,
    workspace_id: String,
    thread_id: String,
    settings: Value,
) -> Result<Value, String> {
    let session = get_session_clone(sessions, &workspace_id).await?;
    let object = settings
        .as_object()
        .ok_or_else(|| "thread settings must be an object".to_string())?;
    let mut params = Map::new();
    params.insert("threadId".to_string(), json!(thread_id));
    if let Some(model) = object.get("model").and_then(Value::as_str) {
        if !model.trim().is_empty() {
            params.insert("model".to_string(), json!(model));
        }
    }
    if let Some(effort) = object.get("effort").and_then(Value::as_str) {
        if !effort.trim().is_empty() {
            params.insert("effort".to_string(), json!(effort));
        }
    }
    if params.len() <= 1 {
        return Err("thread settings must include model and/or effort".to_string());
    }
    session
        .send_request_for_workspace(
            &workspace_id,
            "thread/settings/update",
            Value::Object(params),
        )
        .await
}

pub(crate) async fn model_provider_list_core(
    sessions: &Mutex<HashMap<String, Arc<WorkspaceSession>>>,
    workspace_id: String,
) -> Result<Value, String> {
    let session = get_session_clone(sessions, &workspace_id).await?;
    session
        .send_request_for_workspace(&workspace_id, "modelProvider/list", json!({}))
        .await
}

async fn current_model_provider_id(
    session: &WorkspaceSession,
    workspace_id: &str,
) -> Option<String> {
    let response = session
        .send_request_for_workspace(workspace_id, "modelProvider/list", json!({}))
        .await
        .ok()?;
    model_provider_id_from_catalog(&response)
}

fn model_provider_id_from_catalog(response: &Value) -> Option<String> {
    response
        .get("result")
        .unwrap_or(response)
        .get("currentProviderId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(crate) async fn model_provider_write_core(
    sessions: &Mutex<HashMap<String, Arc<WorkspaceSession>>>,
    workspace_id: String,
    input: Value,
) -> Result<Value, String> {
    let session = get_session_clone(sessions, &workspace_id).await?;
    let object = input
        .as_object()
        .ok_or_else(|| "provider mutation must be an object".to_string())?;
    let action = object
        .get("action")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing provider action".to_string())?;
    let id = object
        .get("id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "provider id is required".to_string())?;
    if !id
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err("provider id may only contain letters, numbers, '-' and '_'".to_string());
    }

    let catalog_response = session
        .send_request_for_workspace(&workspace_id, "modelProvider/list", json!({}))
        .await?;
    let catalog = catalog_response.get("result").unwrap_or(&catalog_response);
    let entries = catalog
        .get("data")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let existing = entries.iter().find(|entry| entry.get("id") == Some(&json!(id)));
    let is_built_in = existing
        .and_then(|entry| entry.get("kind"))
        .and_then(Value::as_str)
        == Some("builtIn");
    let current_id = catalog.get("currentProviderId").and_then(Value::as_str);
    let provider_exists = existing.is_some();

    let edit = |key_path: String, value: Value| {
        json!({ "keyPath": key_path, "value": value, "mergeStrategy": "replace" })
    };
    let mut edits = Vec::new();
    match action {
        "upsert" => {
            if is_built_in {
                return Err(format!("built-in provider '{id}' cannot be edited"));
            }
            let name = object.get("name").and_then(Value::as_str).map(str::trim)
                .filter(|value| !value.is_empty()).ok_or_else(|| "provider name is required".to_string())?;
            let base_url = object.get("baseUrl").and_then(Value::as_str).map(str::trim)
                .filter(|value| !value.is_empty()).ok_or_else(|| "provider base URL is required".to_string())?;
            let parsed_url = reqwest::Url::parse(base_url).map_err(|_| "provider base URL is invalid".to_string())?;
            if !matches!(parsed_url.scheme(), "http" | "https") {
                return Err("provider base URL must use http or https".to_string());
            }
            let wire_api = object.get("wireApi").and_then(Value::as_str).unwrap_or("responses");
            if !matches!(wire_api, "chat" | "responses") {
                return Err("wire API must be 'chat' or 'responses'".to_string());
            }
            let credentials = parse_provider_credentials(object, provider_exists)?;
            let mut provider = Map::new();
            provider.insert("name".to_string(), json!(name));
            provider.insert("base_url".to_string(), json!(base_url.trim_end_matches('/')));
            provider.insert("wire_api".to_string(), json!(wire_api));
            if !provider_exists {
                credentials.apply_to_new_provider(&mut provider);
                edits.push(edit(format!("model_providers.{}", serde_json::to_string(id).map_err(|err| err.to_string())?), Value::Object(provider)));
            } else {
                let provider_path = format!("model_providers.{}", serde_json::to_string(id).map_err(|err| err.to_string())?);
                edits.push(edit(format!("{provider_path}.name"), json!(name)));
                edits.push(edit(format!("{provider_path}.base_url"), json!(base_url.trim_end_matches('/'))));
                edits.push(edit(format!("{provider_path}.wire_api"), json!(wire_api)));
                credentials.append_existing_provider_edits(&provider_path, &edit, &mut edits);
            }
            if existing.is_none() || object.get("select").and_then(Value::as_bool) == Some(true) {
                edits.push(edit("model_provider".to_string(), json!(id)));
            }
        }
        "select" => {
            if existing.is_none() { return Err(format!("provider '{id}' does not exist")); }
            edits.push(edit("model_provider".to_string(), json!(id)));
        }
        "fetch" => {
            if existing.is_none() { return Err(format!("provider '{id}' does not exist")); }
            if is_built_in { return Err(format!("built-in provider '{id}' uses the bundled model catalog")); }
            edits.push(edit("model_provider".to_string(), json!(id)));
        }
        "context" => {
            if existing.is_none() { return Err(format!("provider '{id}' does not exist")); }
            if is_built_in { return Err("built-in model metadata cannot be edited".to_string()); }
            let model_id = object.get("modelId").and_then(Value::as_str).map(str::trim)
                .filter(|value| !value.is_empty()).ok_or_else(|| "model id is required".to_string())?;
            let context_window = object.get("contextWindow").and_then(Value::as_i64)
                .filter(|value| *value >= 1024).ok_or_else(|| "context window must be at least 1024 tokens".to_string())?;
            let raw_models = existing
                .and_then(|entry| entry.get("models"))
                .and_then(Value::as_array);
            let models = upsert_provider_model_context(raw_models, model_id, context_window);
            edits.push(edit(format!("model_providers.{}.models", serde_json::to_string(id).map_err(|err| err.to_string())?), json!(models)));
        }
        "delete" => {
            if existing.is_none() { return Err(format!("provider '{id}' does not exist")); }
            if is_built_in { return Err(format!("built-in provider '{id}' cannot be deleted")); }
            if current_id == Some(id) { return Err("select another provider before deleting the current provider".to_string()); }
            edits.push(edit(format!("model_providers.{}", serde_json::to_string(id).map_err(|err| err.to_string())?), Value::Null));
        }
        _ => return Err(format!("unsupported provider action '{action}'")),
    }
    let write_response = session
        .send_request_for_workspace(
            &workspace_id,
            "config/batchWrite",
            json!({ "edits": edits, "reloadUserConfig": true }),
        )
        .await?;
    if let Some(error) = write_response.get("error") {
        return Err(error.get("message").and_then(Value::as_str).unwrap_or("provider configuration write failed").to_string());
    }
    if write_response.pointer("/result/status").and_then(Value::as_str) == Some("error") {
        let message = write_response.pointer("/result/error/message").and_then(Value::as_str)
            .or_else(|| write_response.pointer("/result/message").and_then(Value::as_str))
            .unwrap_or("provider configuration write failed");
        return Err(message.to_string());
    }
    if action == "fetch" {
        let models_response = session
            .send_request_for_workspace(
                &workspace_id,
                "model/list",
                json!({ "forceRefresh": true }),
            )
            .await?;
        let models = model_list_data(&models_response);
        if models.is_empty() { return Err(format!("provider '{id}' returned no models")); }
        let persisted_models = models
            .iter()
            .filter_map(provider_model_config_from_catalog)
            .collect::<Vec<_>>();
        session.send_request_for_workspace(
            &workspace_id,
            "config/batchWrite",
            json!({ "edits": [edit(format!("model_providers.{}.models", serde_json::to_string(id).map_err(|err| err.to_string())?), json!(persisted_models))], "reloadUserConfig": true }),
        ).await?;
        return Ok(models_response);
    }
    session
        .send_request_for_workspace(&workspace_id, "modelProvider/list", json!({}))
        .await
}

fn model_list_data(response: &Value) -> Vec<Value> {
    response
        .get("result")
        .unwrap_or(response)
        .get("data")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn provider_model_config_from_catalog(value: &Value) -> Option<Value> {
    let model = value.as_object()?;
    let model_id = model
        .get("model")
        .or_else(|| model.get("id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let mut persisted = Map::new();
    persisted.insert("model_id".to_string(), json!(model_id));
    persisted.insert(
        "model_name".to_string(),
        model
            .get("displayName")
            .or_else(|| model.get("display_name"))
            .filter(|value| !value.is_null())
            .cloned()
            .unwrap_or_else(|| json!(model_id)),
    );
    persisted.insert("show_in_picker".to_string(), json!(true));
    if let Some(context_window) = model
        .get("contextWindow")
        .or_else(|| model.get("context_window"))
        .filter(|value| !value.is_null())
    {
        persisted.insert("context_window".to_string(), context_window.clone());
    }
    Some(Value::Object(persisted))
}

fn upsert_provider_model_context(
    raw_models: Option<&Vec<Value>>,
    model_id: &str,
    context_window: i64,
) -> Vec<Value> {
    let mut found = false;
    let mut models = raw_models
        .into_iter()
        .flatten()
        .map(|value| {
            let model = value.as_object().cloned().unwrap_or_default();
            let current_model_id = model
                .get("modelId")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let next_context = if current_model_id == model_id {
                found = true;
                Some(context_window)
            } else {
                model.get("contextWindow").and_then(Value::as_i64)
            };
            let mut persisted = Map::new();
            persisted.insert("model_id".to_string(), json!(current_model_id));
            persisted.insert(
                "model_name".to_string(),
                model
                    .get("modelName")
                    .filter(|value| !value.is_null())
                    .cloned()
                    .unwrap_or_else(|| json!(current_model_id)),
            );
            persisted.insert(
                "show_in_picker".to_string(),
                json!(model.get("showInPicker").and_then(Value::as_bool).unwrap_or(true)),
            );
            for (source, target) in [
                ("maxTokenLen", "max_token_len"),
                ("maxOutputTokens", "max_output_tokens"),
            ] {
                if let Some(value) = model.get(source).filter(|value| !value.is_null()) {
                    persisted.insert(target.to_string(), value.clone());
                }
            }
            if let Some(next_context) = next_context {
                persisted.insert("context_window".to_string(), json!(next_context));
            }
            Value::Object(persisted)
        })
        .collect::<Vec<_>>();

    if !found {
        models.push(json!({
            "model_id": model_id,
            "model_name": model_id,
            "show_in_picker": true,
            "context_window": context_window,
        }));
    }
    models
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ProviderCredentials {
    Preserve,
    Environment(String),
    Direct(String),
    None,
}

impl ProviderCredentials {
    fn apply_to_new_provider(&self, provider: &mut Map<String, Value>) {
        match self {
            Self::Environment(env_key) => {
                provider.insert("env_key".to_string(), json!(env_key));
            }
            Self::Direct(api_key) => {
                provider.insert("experimental_bearer_token".to_string(), json!(api_key));
            }
            Self::Preserve | Self::None => {}
        }
    }

    fn append_existing_provider_edits<F>(
        &self,
        provider_path: &str,
        edit: &F,
        edits: &mut Vec<Value>,
    ) where
        F: Fn(String, Value) -> Value,
    {
        match self {
            Self::Preserve => {}
            Self::Environment(env_key) => {
                edits.push(edit(format!("{provider_path}.env_key"), json!(env_key)));
                edits.push(edit(format!("{provider_path}.experimental_bearer_token"), Value::Null));
            }
            Self::Direct(api_key) => {
                edits.push(edit(format!("{provider_path}.env_key"), Value::Null));
                edits.push(edit(format!("{provider_path}.experimental_bearer_token"), json!(api_key)));
            }
            Self::None => {
                edits.push(edit(format!("{provider_path}.env_key"), Value::Null));
                edits.push(edit(format!("{provider_path}.experimental_bearer_token"), Value::Null));
            }
        }
    }
}

fn parse_provider_credentials(
    object: &Map<String, Value>,
    provider_exists: bool,
) -> Result<ProviderCredentials, String> {
    let mode = object
        .get("credentialMode")
        .and_then(Value::as_str)
        .or_else(|| object.get("envKey").and_then(Value::as_str).map(|_| "environment"))
        .unwrap_or(if provider_exists { "preserve" } else { "none" });
    match mode {
        "preserve" if provider_exists => Ok(ProviderCredentials::Preserve),
        "preserve" => Err("cannot preserve credentials for a new provider".to_string()),
        "environment" => {
            let env_key = object
                .get("envKey")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "API key environment variable is required".to_string())?;
            if !env_key.chars().all(|character| {
                character.is_ascii_uppercase() || character.is_ascii_digit() || character == '_'
            }) {
                return Err(
                    "API key environment variable must contain only A-Z, 0-9 and '_'".to_string(),
                );
            }
            Ok(ProviderCredentials::Environment(env_key.to_string()))
        }
        "direct" => {
            let api_key = object
                .get("apiKey")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "API key is required".to_string())?;
            Ok(ProviderCredentials::Direct(api_key.to_string()))
        }
        "none" => Ok(ProviderCredentials::None),
        other => Err(format!("unsupported credential mode '{other}'")),
    }
}

pub(crate) async fn experimental_feature_list_core(
    sessions: &Mutex<HashMap<String, Arc<WorkspaceSession>>>,
    workspace_id: String,
    cursor: Option<String>,
    limit: Option<u32>,
) -> Result<Value, String> {
    let session = get_session_clone(sessions, &workspace_id).await?;
    let params = json!({ "cursor": cursor, "limit": limit });
    session
        .send_request_for_workspace(&workspace_id, "experimentalFeature/list", params)
        .await
}

pub(crate) async fn account_rate_limits_core(
    sessions: &Mutex<HashMap<String, Arc<WorkspaceSession>>>,
    workspace_id: String,
) -> Result<Value, String> {
    let session = get_session_clone(sessions, &workspace_id).await?;
    session
        .send_request_for_workspace(&workspace_id, "account/rateLimits/read", Value::Null)
        .await
}

pub(crate) async fn account_read_core(
    sessions: &Mutex<HashMap<String, Arc<WorkspaceSession>>>,
    workspaces: &Mutex<HashMap<String, WorkspaceEntry>>,
    workspace_id: String,
) -> Result<Value, String> {
    let session = {
        let sessions = sessions.lock().await;
        sessions.get(&workspace_id).cloned()
    };
    let response = if let Some(session) = session {
        session
            .send_request_for_workspace(&workspace_id, "account/read", Value::Null)
            .await
            .ok()
    } else {
        None
    };

    let (entry, parent_entry) = resolve_workspace_and_parent(workspaces, &workspace_id).await?;
    let codex_home = resolve_workspace_codex_home(&entry, parent_entry.as_ref())
        .or_else(resolve_default_codex_home);
    let fallback = read_auth_account(codex_home);

    Ok(build_account_response(response, fallback))
}

pub(crate) async fn codex_login_core(
    sessions: &Mutex<HashMap<String, Arc<WorkspaceSession>>>,
    codex_login_cancels: &Mutex<HashMap<String, CodexLoginCancelState>>,
    workspace_id: String,
) -> Result<Value, String> {
    let session = get_session_clone(sessions, &workspace_id).await?;
    let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
    {
        let mut cancels = codex_login_cancels.lock().await;
        if let Some(existing) = cancels.remove(&workspace_id) {
            match existing {
                CodexLoginCancelState::PendingStart(tx) => {
                    let _ = tx.send(());
                }
                CodexLoginCancelState::LoginId(_) => {}
            }
        }
        cancels.insert(
            workspace_id.clone(),
            CodexLoginCancelState::PendingStart(cancel_tx),
        );
    }

    let start = Instant::now();
    let mut cancel_rx = cancel_rx;
    let workspace_for_request = workspace_id.clone();
    let mut login_request: Pin<Box<_>> = Box::pin(session.send_request_for_workspace(
        &workspace_for_request,
        "account/login/start",
        json!({ "type": "chatgpt" }),
    ));

    let response = loop {
        match cancel_rx.try_recv() {
            Ok(_) => {
                let mut cancels = codex_login_cancels.lock().await;
                cancels.remove(&workspace_id);
                return Err("Codex login canceled.".to_string());
            }
            Err(TryRecvError::Closed) => {
                let mut cancels = codex_login_cancels.lock().await;
                cancels.remove(&workspace_id);
                return Err("Codex login canceled.".to_string());
            }
            Err(TryRecvError::Empty) => {}
        }

        let elapsed = start.elapsed();
        if elapsed >= LOGIN_START_TIMEOUT {
            let mut cancels = codex_login_cancels.lock().await;
            cancels.remove(&workspace_id);
            return Err("Codex login start timed out.".to_string());
        }

        let tick = Duration::from_millis(150);
        let remaining = LOGIN_START_TIMEOUT.saturating_sub(elapsed);
        let wait_for = remaining.min(tick);

        match timeout(wait_for, &mut login_request).await {
            Ok(result) => break result?,
            Err(_elapsed) => continue,
        }
    };

    let payload = response.get("result").unwrap_or(&response);
    let login_id = payload
        .get("loginId")
        .or_else(|| payload.get("login_id"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| "missing login id in account/login/start response".to_string())?;
    let auth_url = payload
        .get("authUrl")
        .or_else(|| payload.get("auth_url"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| "missing auth url in account/login/start response".to_string())?;

    {
        let mut cancels = codex_login_cancels.lock().await;
        cancels.insert(
            workspace_id,
            CodexLoginCancelState::LoginId(login_id.clone()),
        );
    }

    Ok(json!({
        "loginId": login_id,
        "authUrl": auth_url,
        "raw": response,
    }))
}

pub(crate) async fn codex_login_cancel_core(
    sessions: &Mutex<HashMap<String, Arc<WorkspaceSession>>>,
    codex_login_cancels: &Mutex<HashMap<String, CodexLoginCancelState>>,
    workspace_id: String,
) -> Result<Value, String> {
    let cancel_state = {
        let mut cancels = codex_login_cancels.lock().await;
        cancels.remove(&workspace_id)
    };

    let Some(cancel_state) = cancel_state else {
        return Ok(json!({ "canceled": false }));
    };

    match cancel_state {
        CodexLoginCancelState::PendingStart(cancel_tx) => {
            let _ = cancel_tx.send(());
            return Ok(json!({
                "canceled": true,
                "status": "canceled",
            }));
        }
        CodexLoginCancelState::LoginId(login_id) => {
            let session = get_session_clone(sessions, &workspace_id).await?;
            let response = session
                .send_request_for_workspace(
                    &workspace_id,
                    "account/login/cancel",
                    json!({
                        "loginId": login_id,
                    }),
                )
                .await?;

            let payload = response.get("result").unwrap_or(&response);
            let status = payload
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let canceled = status.eq_ignore_ascii_case("canceled");

            Ok(json!({
                "canceled": canceled,
                "status": status,
                "raw": response,
            }))
        }
    }
}

pub(crate) async fn skills_list_core(
    sessions: &Mutex<HashMap<String, Arc<WorkspaceSession>>>,
    workspaces: &Mutex<HashMap<String, WorkspaceEntry>>,
    workspace_id: String,
) -> Result<Value, String> {
    let session = get_session_clone(sessions, &workspace_id).await?;
    let workspace_path = resolve_workspace_path_core(workspaces, &workspace_id).await?;

    // Codex can discover project-scoped skills from `<workspace>/.agents/skills`.
    // Some environments don't surface those reliably in CodexMonitor unless we
    // pass the default project skills path explicitly.
    let mut source_paths: Vec<String> = vec![];
    let project_skills_dir = Path::new(&workspace_path).join(".agents").join("skills");
    if project_skills_dir.is_dir() {
        if let Some(p) = project_skills_dir.to_str() {
            source_paths.push(p.to_string());
        }
    }

    let params = if source_paths.is_empty() {
        json!({ "cwd": workspace_path })
    } else {
        json!({ "cwd": workspace_path, "skillsPaths": source_paths })
    };

    let mut response = session
        .send_request_for_workspace(&workspace_id, "skills/list", params)
        .await?;

    // Attach diagnostics for the UI (non-breaking: keep original response fields).
    if let Value::Object(ref mut obj) = response {
        obj.insert("sourcePaths".to_string(), json!(source_paths));
        obj.insert("sourceErrors".to_string(), json!([]));
    }

    Ok(response)
}

pub(crate) async fn apps_list_core(
    sessions: &Mutex<HashMap<String, Arc<WorkspaceSession>>>,
    workspace_id: String,
    cursor: Option<String>,
    limit: Option<u32>,
    thread_id: Option<String>,
) -> Result<Value, String> {
    let session = get_session_clone(sessions, &workspace_id).await?;
    let params = json!({ "cursor": cursor, "limit": limit, "threadId": thread_id });
    session
        .send_request_for_workspace(&workspace_id, "app/list", params)
        .await
}

pub(crate) async fn respond_to_server_request_core(
    sessions: &Mutex<HashMap<String, Arc<WorkspaceSession>>>,
    workspace_id: String,
    request_id: Value,
    result: Value,
) -> Result<(), String> {
    let session = get_session_clone(sessions, &workspace_id).await?;
    session.send_response(request_id, result).await
}

pub(crate) async fn remember_approval_rule_core(
    workspaces: &Mutex<HashMap<String, WorkspaceEntry>>,
    workspace_id: String,
    command: Vec<String>,
) -> Result<Value, String> {
    let command = command
        .into_iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>();
    if command.is_empty() {
        return Err("empty command".to_string());
    }

    let codex_home = resolve_codex_home_for_workspace_core(workspaces, &workspace_id).await?;
    let rules_path = rules::default_rules_path(&codex_home);
    rules::append_prefix_rule(&rules_path, &command)?;

    Ok(json!({
        "ok": true,
        "rulesPath": rules_path,
    }))
}

pub(crate) async fn get_config_model_core(
    workspaces: &Mutex<HashMap<String, WorkspaceEntry>>,
    workspace_id: String,
) -> Result<Value, String> {
    let codex_home = resolve_codex_home_for_workspace_core(workspaces, &workspace_id).await?;
    let model = codex_config::read_config_model(Some(codex_home))?;
    Ok(json!({ "model": model }))
}

pub(crate) async fn resolve_approval_core(
    sessions: &Mutex<HashMap<String, Arc<WorkspaceSession>>>,
    workspace_id: String,
    thread_id: String,
    decision: String,
) -> Result<Value, String> {
    let session = get_session_clone(sessions, &workspace_id).await?;
    let mut params = Map::new();
    params.insert("threadId".to_string(), json!(thread_id));
    params.insert("decision".to_string(), json!(decision));
    session
        .send_request_for_workspace(&workspace_id, "resolveApproval", Value::Object(params))
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn normalize_strips_file_uri_prefix() {
        assert_eq!(
            normalize_file_path("file:///var/mobile/Containers/Data/photo.jpg"),
            "/var/mobile/Containers/Data/photo.jpg"
        );
    }

    #[test]
    fn normalize_strips_file_localhost_prefix() {
        assert_eq!(
            normalize_file_path("file://localhost/Users/test/image.png"),
            "/Users/test/image.png"
        );
    }

    #[test]
    fn normalize_decodes_percent_encoding() {
        assert_eq!(
            normalize_file_path("file:///var/mobile/path%20with%20spaces/img.jpg"),
            "/var/mobile/path with spaces/img.jpg"
        );
    }

    #[test]
    fn normalize_plain_path_unchanged() {
        assert_eq!(
            normalize_file_path("/var/mobile/Containers/Data/photo.jpg"),
            "/var/mobile/Containers/Data/photo.jpg"
        );
    }

    #[test]
    fn normalize_plain_path_percent_sequences_unchanged() {
        assert_eq!(
            normalize_file_path("/tmp/report%20final.png"),
            "/tmp/report%20final.png"
        );
    }

    #[test]
    fn normalize_trims_whitespace() {
        assert_eq!(normalize_file_path("  /tmp/image.png  "), "/tmp/image.png");
    }

    #[test]
    fn read_image_data_url_core_rejects_file_uri_that_does_not_exist() {
        let result = read_image_as_data_url_core("file:///nonexistent/photo.png");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            !err.contains("file://"),
            "error should reference normalized path, got: {err}"
        );
        assert!(err.contains("/nonexistent/photo.png"));
    }

    #[test]
    fn read_image_data_url_core_succeeds_with_file_uri_for_real_file() {
        let dir = std::env::temp_dir().join("codex_monitor_test");
        std::fs::create_dir_all(&dir).unwrap();
        let img_path = dir.join("test_photo.png");
        let png_bytes: &[u8] = &[
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00,
            0x00, 0x90, 0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08,
            0xD7, 0x63, 0xF8, 0xCF, 0xC0, 0x00, 0x00, 0x00, 0x02, 0x00, 0x01, 0xE2, 0x21, 0xBC,
            0x33, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ];
        std::fs::write(&img_path, png_bytes).unwrap();

        let file_uri = format!("file://{}", img_path.display());
        let result = read_image_as_data_url_core(&file_uri);
        assert!(
            result.is_ok(),
            "file:// URI for real file should succeed, got: {:?}",
            result.err()
        );
        let data_url = result.unwrap();
        assert!(data_url.starts_with("data:image/png;base64,"));

        let space_dir = dir.join("path with spaces");
        std::fs::create_dir_all(&space_dir).unwrap();
        let space_img = space_dir.join("photo.png");
        std::fs::write(&space_img, png_bytes).unwrap();
        let encoded_uri = format!(
            "file://{}",
            space_img.display().to_string().replace(' ', "%20")
        );
        let result2 = read_image_as_data_url_core(&encoded_uri);
        assert!(
            result2.is_ok(),
            "percent-encoded file:// URI should succeed, got: {:?}",
            result2.err()
        );

        let percent_img = dir.join("report%20final.png");
        std::fs::write(&percent_img, png_bytes).unwrap();
        let plain_percent_path = percent_img.display().to_string();
        let result3 = read_image_as_data_url_core(&plain_percent_path);
        assert!(
            result3.is_ok(),
            "plain filesystem paths with percent sequences should not be decoded, got: {:?}",
            result3.err()
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn heif_paths_are_inlined_for_codex() {
        assert!(should_inline_image_path_for_codex("/tmp/photo.heic"));
        assert!(should_inline_image_path_for_codex("/tmp/photo.HEIF"));
        assert!(!should_inline_image_path_for_codex("/tmp/photo.png"));
    }

    #[test]
    fn insert_optional_nullable_string_omits_missing_and_preserves_null() {
        let mut params = Map::new();

        insert_optional_nullable_string(&mut params, "serviceTier", None);
        assert!(!params.contains_key("serviceTier"));

        insert_optional_nullable_string(&mut params, "serviceTier", Some(None));
        assert_eq!(params.get("serviceTier"), Some(&Value::Null));

        insert_optional_nullable_string(&mut params, "serviceTier", Some(Some("fast".to_string())));
        assert_eq!(params.get("serviceTier"), Some(&json!("fast")));
    }

    #[test]
    fn thread_list_source_kinds_exclude_generic_subagent_and_keep_explicit_variants() {
        assert!(!THREAD_LIST_SOURCE_KINDS.contains(&"subAgent"));
        assert!(THREAD_LIST_SOURCE_KINDS.contains(&"subAgentReview"));
        assert!(THREAD_LIST_SOURCE_KINDS.contains(&"subAgentCompact"));
        assert!(THREAD_LIST_SOURCE_KINDS.contains(&"subAgentThreadSpawn"));
    }

    #[test]
    fn provider_credentials_support_environment_variable_and_direct_key_modes() {
        let environment = json!({
            "credentialMode": "environment",
            "envKey": "DEEPSEEK_API_KEY",
        });
        assert_eq!(
            parse_provider_credentials(environment.as_object().unwrap(), false),
            Ok(ProviderCredentials::Environment(
                "DEEPSEEK_API_KEY".to_string()
            ))
        );

        let direct = json!({
            "credentialMode": "direct",
            "apiKey": "test-direct-key",
        });
        let credentials = parse_provider_credentials(direct.as_object().unwrap(), false).unwrap();
        assert_eq!(
            credentials,
            ProviderCredentials::Direct("test-direct-key".to_string())
        );
        let mut provider = Map::new();
        credentials.apply_to_new_provider(&mut provider);
        assert_eq!(
            provider.get("experimental_bearer_token"),
            Some(&json!("test-direct-key"))
        );
        assert!(!provider.contains_key("env_key"));
    }

    #[test]
    fn direct_provider_credentials_clear_the_environment_key() {
        let edit = |key_path: String, value: Value| json!({ "keyPath": key_path, "value": value });
        let mut edits = Vec::new();
        ProviderCredentials::Direct("test-direct-key".to_string())
            .append_existing_provider_edits("model_providers.deepseek", &edit, &mut edits);

        assert_eq!(
            edits,
            vec![
                json!({ "keyPath": "model_providers.deepseek.env_key", "value": null }),
                json!({ "keyPath": "model_providers.deepseek.experimental_bearer_token", "value": "test-direct-key" }),
            ]
        );
    }

    #[test]
    fn provider_model_context_persists_a_newly_discovered_model() {
        let models = upsert_provider_model_context(None, "deepseek-v4-flash", 128_000);

        assert_eq!(
            models,
            vec![json!({
                "model_id": "deepseek-v4-flash",
                "model_name": "deepseek-v4-flash",
                "show_in_picker": true,
                "context_window": 128_000,
            })]
        );
        assert!(!models[0].as_object().unwrap().values().any(Value::is_null));
    }

    #[test]
    fn provider_model_context_updates_an_existing_model() {
        let persisted = vec![json!({
            "modelId": "deepseek-v4-flash",
            "modelName": "DeepSeek V4 Flash",
            "maxTokenLen": 64_000,
            "maxOutputTokens": 8_192,
            "showInPicker": true,
            "contextWindow": 64_000,
        })];

        let models =
            upsert_provider_model_context(Some(&persisted), "deepseek-v4-flash", 128_000);

        assert_eq!(models.len(), 1);
        assert_eq!(models[0]["model_id"], json!("deepseek-v4-flash"));
        assert_eq!(models[0]["model_name"], json!("DeepSeek V4 Flash"));
        assert_eq!(models[0]["context_window"], json!(128_000));
    }

    #[test]
    fn model_list_data_reads_json_rpc_and_unwrapped_responses() {
        let models = json!([{"model": "deepseek-v4-flash"}]);

        assert_eq!(
            model_list_data(&json!({"result": {"data": models.clone()}})),
            models.as_array().unwrap().clone()
        );
        assert_eq!(
            model_list_data(&json!({"data": models.clone()})),
            models.as_array().unwrap().clone()
        );
    }

    #[test]
    fn provider_model_catalog_omits_null_toml_values() {
        let model = provider_model_config_from_catalog(&json!({
            "model": "deepseek-v4-flash",
            "displayName": null,
            "contextWindow": null,
        }))
        .unwrap();

        assert_eq!(model["model_name"], json!("deepseek-v4-flash"));
        assert!(!model.as_object().unwrap().contains_key("context_window"));
        assert!(!model.as_object().unwrap().values().any(Value::is_null));
    }

    #[test]
    fn model_provider_id_reads_json_rpc_and_unwrapped_catalogs() {
        assert_eq!(
            model_provider_id_from_catalog(&json!({
                "result": {"currentProviderId": "deepseek"}
            })),
            Some("deepseek".to_string())
        );
        assert_eq!(
            model_provider_id_from_catalog(&json!({
                "currentProviderId": "deepseek"
            })),
            Some("deepseek".to_string())
        );
    }

}
