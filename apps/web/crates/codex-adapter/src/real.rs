use async_trait::async_trait;
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use open_web_codex_profile_host::{ProfileHost, ProfileHostConfig, ProfileHostState};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::RwLock;

use crate::{
    AdapterError, AuthorizedWorkspace, CanceledProfileLogin, CodexAdapter, HealthStatus,
    ProfileLoginStatus, ProfileMutation, ProfileQuery, ReviewTarget, StartedProfileLogin,
    StartedThread, TurnOptions,
};

/// Adapter backed directly by a native Profile Host and Codex app-server
/// JSONL connection without an intermediate local gateway.
pub struct RealCodexAdapter {
    host: ProfileHost,
    workspace_id: String,
    workspace_root: PathBuf,
    thread_workspaces: Arc<RwLock<HashMap<String, AuthorizedWorkspace>>>,
    suppressed_threads: Arc<RwLock<HashSet<String>>>,
    active_login_id: Arc<RwLock<Option<String>>>,
    login_statuses: Arc<RwLock<HashMap<String, ProfileLoginStatus>>>,
    terminal_workspaces: Arc<RwLock<HashMap<String, AuthorizedWorkspace>>>,
    local_events: broadcast::Sender<Value>,
}

impl RealCodexAdapter {
    pub async fn spawn(
        config: ProfileHostConfig,
        workspace_id: impl Into<String>,
    ) -> Result<Self, AdapterError> {
        let workspace_root = config.workspace_root.clone();
        let host = ProfileHost::spawn(config).await?;
        Self::from_host(host, workspace_id, workspace_root)
    }

    pub fn from_host(
        host: ProfileHost,
        workspace_id: impl Into<String>,
        workspace_root: PathBuf,
    ) -> Result<Self, AdapterError> {
        let workspace_root = workspace_root.canonicalize().map_err(|error| {
            AdapterError::Internal(format!("failed to resolve workspace root: {error}"))
        })?;
        let (local_events, _) = broadcast::channel(256);
        Ok(Self {
            host,
            workspace_id: workspace_id.into(),
            workspace_root,
            thread_workspaces: Arc::new(RwLock::new(HashMap::new())),
            suppressed_threads: Arc::new(RwLock::new(HashSet::new())),
            active_login_id: Arc::new(RwLock::new(None)),
            login_statuses: Arc::new(RwLock::new(HashMap::new())),
            terminal_workspaces: Arc::new(RwLock::new(HashMap::new())),
            local_events,
        })
    }

    /// Clone the native Profile connection for server-owned typed services.
    /// The browser must never receive this transport directly.
    pub fn profile_host(&self) -> ProfileHost {
        self.host.clone()
    }

    fn require_workspace(&self, params: &Value) -> Result<(), AdapterError> {
        let requested = params
            .get("workspaceId")
            .and_then(Value::as_str)
            .ok_or_else(|| AdapterError::Internal("missing workspaceId".to_string()))?;
        if requested != self.workspace_id {
            return Err(AdapterError::Rpc(format!(
                "workspace '{requested}' is not registered with this Profile Host"
            )));
        }
        Ok(())
    }

    fn authorized_root(&self, workspace: &AuthorizedWorkspace) -> Result<String, AdapterError> {
        let root = workspace.root.canonicalize().map_err(|error| {
            AdapterError::Internal(format!("failed to resolve authorized workspace: {error}"))
        })?;
        if root != self.workspace_root && root.parent() != Some(self.workspace_root.as_path()) {
            return Err(AdapterError::Rpc(
                "workspace is outside the Profile Host Runner root".to_string(),
            ));
        }
        Ok(root.to_string_lossy().to_string())
    }

    async fn start_thread_in_workspace(
        &self,
        workspace: &AuthorizedWorkspace,
    ) -> Result<StartedThread, AdapterError> {
        let workspace_root = self.authorized_root(workspace)?;
        let result = self
            .host
            .request(
                "thread/start",
                json!({
                    "cwd": &workspace_root,
                    "approvalPolicy": "on-request",
                }),
            )
            .await?;
        let thread_id = result
            .pointer("/thread/id")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                AdapterError::Rpc("thread/start response omitted thread.id".to_string())
            })?;
        self.thread_workspaces
            .write()
            .await
            .insert(thread_id.to_string(), workspace.clone());
        Ok(StartedThread {
            thread_id: thread_id.to_string(),
        })
    }

    async fn ensure_thread_bound(
        &self,
        workspace: &AuthorizedWorkspace,
        thread_id: &str,
    ) -> Result<String, AdapterError> {
        if thread_id.trim().is_empty() {
            return Err(AdapterError::Internal("Thread id is required".to_string()));
        }
        let workspace_root = self.authorized_root(workspace)?;
        if let Some(bound) = self.thread_workspaces.read().await.get(thread_id).cloned() {
            if bound == *workspace {
                return Ok(workspace_root);
            }
            return Err(AdapterError::Rpc(
                "Thread is not bound to the authorized workspace".to_string(),
            ));
        }

        self.host
            .request(
                "thread/resume",
                json!({
                    "threadId": thread_id,
                    "cwd": &workspace_root,
                    "approvalPolicy": "on-request",
                }),
            )
            .await?;
        self.thread_workspaces
            .write()
            .await
            .insert(thread_id.to_string(), workspace.clone());
        Ok(workspace_root)
    }

    async fn send_user_message_in_workspace(
        &self,
        workspace: &AuthorizedWorkspace,
        thread_id: &str,
        text: &str,
        options: &TurnOptions,
    ) -> Result<Value, AdapterError> {
        if thread_id.trim().is_empty() || text.trim().is_empty() {
            return Err(AdapterError::Internal(
                "Thread id and message text are required".to_string(),
            ));
        }
        let workspace_root = self.ensure_thread_bound(workspace, thread_id).await?;

        let mut input = vec![json!({ "type": "text", "text": text.trim() })];
        for image in &options.images {
            if !(image.starts_with("data:")
                || image.starts_with("https://")
                || image.starts_with("http://"))
            {
                return Err(AdapterError::Internal(
                    "image input must be an embedded or remote URL".to_string(),
                ));
            }
            input.push(json!({ "type": "image", "url": image }));
        }
        let read_only = options.access_mode.as_deref() == Some("read-only");
        let mut params = json!({
            "threadId": thread_id,
            "input": input,
            "cwd": &workspace_root,
            "approvalPolicy": "on-request",
            "sandboxPolicy": if read_only {
                json!({ "type": "readOnly" })
            } else {
                json!({
                    "type": "workspaceWrite",
                    "writableRoots": [&workspace_root],
                    "networkAccess": true,
                })
            },
        });
        let object = params
            .as_object_mut()
            .expect("turn/start params are an object");
        if let Some(model) = options
            .model
            .as_ref()
            .filter(|value| !value.trim().is_empty())
        {
            object.insert("model".to_string(), json!(model));
        }
        if let Some(effort) = options
            .effort
            .as_ref()
            .filter(|value| !value.trim().is_empty())
        {
            object.insert("effort".to_string(), json!(effort));
        }
        if let Some(service_tier) = options
            .service_tier
            .as_ref()
            .filter(|value| !value.trim().is_empty())
        {
            object.insert("serviceTier".to_string(), json!(service_tier));
        }
        if let Some(collaboration_mode) = &options.collaboration_mode {
            object.insert("collaborationMode".to_string(), collaboration_mode.clone());
        }
        let result = self.host.request("turn/start", params).await?;
        Ok(json!({
            "status": "sent",
            "turnId": result.pointer("/turn/id").cloned().unwrap_or(Value::Null),
        }))
    }

    async fn steer_turn_in_workspace(
        &self,
        workspace: &AuthorizedWorkspace,
        thread_id: &str,
        turn_id: &str,
        text: &str,
        images: &[String],
    ) -> Result<Value, AdapterError> {
        if thread_id.trim().is_empty() || turn_id.trim().is_empty() || text.trim().is_empty() {
            return Err(AdapterError::Internal(
                "Thread id, Turn id and message text are required".to_string(),
            ));
        }
        self.ensure_thread_bound(workspace, thread_id).await?;
        let mut input = vec![json!({ "type": "text", "text": text.trim() })];
        for image in images {
            if !(image.starts_with("data:")
                || image.starts_with("https://")
                || image.starts_with("http://"))
            {
                return Err(AdapterError::Internal(
                    "image input must be an embedded or remote URL".to_string(),
                ));
            }
            input.push(json!({ "type": "image", "url": image }));
        }
        self.host
            .request(
                "turn/steer",
                json!({
                    "threadId": thread_id,
                    "expectedTurnId": turn_id,
                    "input": input,
                }),
            )
            .await
            .map_err(Into::into)
    }

    async fn interrupt_turn_in_workspace(
        &self,
        workspace: &AuthorizedWorkspace,
        thread_id: &str,
        turn_id: &str,
    ) -> Result<(), AdapterError> {
        if thread_id.trim().is_empty() || turn_id.trim().is_empty() {
            return Err(AdapterError::Internal(
                "Thread id and Turn id are required".to_string(),
            ));
        }
        self.ensure_thread_bound(workspace, thread_id).await?;
        self.host
            .request(
                "turn/interrupt",
                json!({ "threadId": thread_id, "turnId": turn_id }),
            )
            .await?;
        Ok(())
    }
}

fn login_completion(message: &Value) -> Option<(String, bool, Option<String>)> {
    if message.get("method").and_then(Value::as_str) != Some("account/login/completed") {
        return None;
    }
    let params = message.get("params")?;
    let login_id = params
        .get("loginId")
        .or_else(|| params.get("login_id"))?
        .as_str()?
        .trim();
    if login_id.is_empty() {
        return None;
    }
    let success = params
        .get("success")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let error = params
        .get("error")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    Some((login_id.to_string(), success, error))
}

#[async_trait]
impl CodexAdapter for RealCodexAdapter {
    async fn health(&self) -> Result<HealthStatus, AdapterError> {
        let snapshot = self.host.snapshot().await;
        Ok(HealthStatus {
            ok: snapshot.state == ProfileHostState::Ready,
            version: snapshot
                .server_build
                .unwrap_or_else(|| "unknown".to_string()),
            name: "codex-app-server".to_string(),
        })
    }

    async fn rpc(&self, method: &str, params: Value) -> Result<Value, AdapterError> {
        match method {
            "list_workspaces" => Ok(json!([{
                "id": self.workspace_id,
                "name": self.workspace_id,
                "path": self.workspace_root,
                "connected": true,
                "kind": "profile",
            }])),
            "start_thread" => {
                self.require_workspace(&params)?;
                let workspace = AuthorizedWorkspace {
                    id: self.workspace_id.clone(),
                    root: self.workspace_root.clone(),
                };
                let started = self.start_thread_in_workspace(&workspace).await?;
                Ok(json!({ "threadId": started.thread_id }))
            }
            "send_user_message" => {
                self.require_workspace(&params)?;
                let workspace = AuthorizedWorkspace {
                    id: self.workspace_id.clone(),
                    root: self.workspace_root.clone(),
                };
                let thread_id = params.get("threadId").and_then(Value::as_str).unwrap_or_default();
                let text = params.get("text").and_then(Value::as_str).unwrap_or_default();
                self.send_user_message_in_workspace(
                    &workspace,
                    thread_id,
                    text,
                    &TurnOptions::default(),
                )
                .await
            }
            other => Err(AdapterError::NotImplemented(format!(
                "native Profile Host adapter method '{other}' is not available through the transitional RPC interface"
            ))),
        }
    }

    async fn start_thread(
        &self,
        workspace: &AuthorizedWorkspace,
    ) -> Result<StartedThread, AdapterError> {
        self.start_thread_in_workspace(workspace).await
    }

    async fn fork_thread(
        &self,
        source_workspace: &AuthorizedWorkspace,
        target_workspace: &AuthorizedWorkspace,
        thread_id: &str,
    ) -> Result<StartedThread, AdapterError> {
        self.ensure_thread_bound(source_workspace, thread_id)
            .await?;
        let target_root = self.authorized_root(target_workspace)?;
        let result = self
            .host
            .request(
                "thread/fork",
                json!({
                    "threadId": thread_id,
                    "cwd": target_root,
                    "approvalPolicy": "on-request",
                }),
            )
            .await?;
        let forked_thread_id = result
            .pointer("/thread/id")
            .and_then(Value::as_str)
            .ok_or_else(|| AdapterError::Rpc("thread/fork response omitted thread.id".to_string()))?
            .to_string();
        self.thread_workspaces
            .write()
            .await
            .insert(forked_thread_id.clone(), target_workspace.clone());
        Ok(StartedThread {
            thread_id: forked_thread_id,
        })
    }

    async fn send_user_message(
        &self,
        workspace: &AuthorizedWorkspace,
        thread_id: &str,
        text: &str,
        options: &TurnOptions,
    ) -> Result<Value, AdapterError> {
        self.send_user_message_in_workspace(workspace, thread_id, text, options)
            .await
    }

    async fn steer_turn(
        &self,
        workspace: &AuthorizedWorkspace,
        thread_id: &str,
        turn_id: &str,
        text: &str,
        images: &[String],
    ) -> Result<Value, AdapterError> {
        self.steer_turn_in_workspace(workspace, thread_id, turn_id, text, images)
            .await
    }

    async fn interrupt_turn(
        &self,
        workspace: &AuthorizedWorkspace,
        thread_id: &str,
        turn_id: &str,
    ) -> Result<(), AdapterError> {
        self.interrupt_turn_in_workspace(workspace, thread_id, turn_id)
            .await
    }

    async fn respond_to_server_request(
        &self,
        request_id: Value,
        result: Value,
    ) -> Result<(), AdapterError> {
        self.host.respond(request_id, Ok(result)).await?;
        Ok(())
    }

    async fn query_profile(&self, query: ProfileQuery) -> Result<Value, AdapterError> {
        let (method, params) = match query {
            ProfileQuery::Account => ("account/read", json!({ "refreshToken": false })),
            ProfileQuery::RateLimits => ("account/rateLimits/read", json!({})),
            ProfileQuery::Usage => ("account/usage/read", json!({})),
            ProfileQuery::CollaborationModes => ("collaborationMode/list", json!({})),
            ProfileQuery::Apps {
                cursor,
                limit,
                thread_id,
            } => (
                "app/list",
                json!({ "cursor": cursor, "limit": limit, "threadId": thread_id }),
            ),
            ProfileQuery::McpServers {
                cursor,
                limit,
                thread_id,
            } => (
                "mcpServerStatus/list",
                json!({ "cursor": cursor, "limit": limit, "threadId": thread_id }),
            ),
            ProfileQuery::ExperimentalFeatures {
                cursor,
                limit,
                thread_id,
            } => (
                "experimentalFeature/list",
                json!({ "cursor": cursor, "limit": limit, "threadId": thread_id }),
            ),
            ProfileQuery::Skills {
                workspace,
                force_reload,
            } => {
                let root = self.authorized_root(&workspace)?;
                (
                    "skills/list",
                    json!({ "cwds": [root], "forceReload": force_reload }),
                )
            }
            ProfileQuery::Config => (
                "config/read",
                json!({ "includeLayers": false, "cwd": null }),
            ),
        };
        self.host.request(method, params).await.map_err(Into::into)
    }

    async fn mutate_profile(&self, mutation: ProfileMutation) -> Result<Value, AdapterError> {
        match mutation {
            ProfileMutation::SetExperimentalFeature { name, enabled } => {
                let mut enablement = serde_json::Map::new();
                enablement.insert(name, json!(enabled));
                self.host
                    .request(
                        "experimentalFeature/enablement/set",
                        json!({ "enablement": enablement }),
                    )
                    .await
                    .map_err(Into::into)
            }
            ProfileMutation::SetAgentCore {
                multi_agent_enabled,
                max_threads,
                max_depth,
            } => {
                self.host
                    .request(
                        "config/batchWrite",
                        json!({
                            "edits": [
                                { "keyPath": "features.multi_agent", "value": multi_agent_enabled, "mergeStrategy": "replace" },
                                { "keyPath": "agents.max_threads", "value": max_threads, "mergeStrategy": "replace" },
                                { "keyPath": "agents.max_depth", "value": max_depth, "mergeStrategy": "replace" }
                            ],
                            "filePath": null,
                            "expectedVersion": null,
                            "reloadUserConfig": true
                        }),
                    )
                    .await
                    .map_err(Into::into)
            }
            ProfileMutation::SetAgentDefinition {
                original_name,
                name,
                description,
                config_file,
            } => {
                let mut definition = serde_json::Map::new();
                if let Some(description) = description {
                    definition.insert("description".to_string(), json!(description));
                }
                definition.insert("config_file".to_string(), json!(config_file));
                let definition = Value::Object(definition);
                if let Some(original_name) = original_name.filter(|value| value != &name) {
                    self.host
                        .request(
                            "config/batchWrite",
                            json!({
                                "edits": [
                                    { "keyPath": format!("agents.{original_name}"), "value": Value::Null, "mergeStrategy": "replace" },
                                    { "keyPath": format!("agents.{name}"), "value": definition, "mergeStrategy": "replace" }
                                ],
                                "filePath": null,
                                "expectedVersion": null,
                                "reloadUserConfig": true
                            }),
                        )
                        .await
                        .map_err(Into::into)
                } else {
                    self.host
                        .request(
                            "config/batchWrite",
                            json!({
                                "edits": [
                                    { "keyPath": format!("agents.{name}"), "value": definition, "mergeStrategy": "replace" }
                                ],
                                "filePath": null,
                                "expectedVersion": null,
                                "reloadUserConfig": true
                            }),
                        )
                        .await
                        .map_err(Into::into)
                }
            }
            ProfileMutation::RemoveAgentDefinition { name } => {
                self.host
                    .request(
                        "config/batchWrite",
                        json!({
                            "edits": [
                                { "keyPath": format!("agents.{name}"), "value": Value::Null, "mergeStrategy": "replace" }
                            ],
                            "filePath": null,
                            "expectedVersion": null,
                            "reloadUserConfig": true
                        }),
                    )
                    .await
                    .map_err(Into::into)
            }
        }
    }

    async fn start_profile_login(&self) -> Result<StartedProfileLogin, AdapterError> {
        let response = self
            .host
            .request(
                "account/login/start",
                json!({
                    "type": "chatgpt",
                    "codexStreamlinedLogin": false,
                    "useHostedLoginSuccessPage": true,
                    "appBrand": "codex"
                }),
            )
            .await?;
        if response.get("type").and_then(Value::as_str) != Some("chatgpt") {
            return Err(AdapterError::Rpc(
                "account/login/start returned an unexpected login type".to_string(),
            ));
        }
        let login_id = response
            .get("loginId")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| AdapterError::Rpc("account/login/start omitted loginId".to_string()))?
            .to_string();
        let auth_url = response
            .get("authUrl")
            .and_then(Value::as_str)
            .filter(|value| value.starts_with("https://") || value.starts_with("http://"))
            .ok_or_else(|| {
                AdapterError::Rpc("account/login/start returned an invalid authUrl".to_string())
            })?
            .to_string();
        *self.active_login_id.write().await = Some(login_id.clone());
        self.login_statuses
            .write()
            .await
            .entry(login_id.clone())
            .or_insert(ProfileLoginStatus {
                completed: false,
                success: None,
                error: None,
            });
        Ok(StartedProfileLogin { login_id, auth_url })
    }

    async fn cancel_profile_login(&self) -> Result<CanceledProfileLogin, AdapterError> {
        let Some(login_id) = self.active_login_id.read().await.clone() else {
            return Ok(CanceledProfileLogin {
                canceled: false,
                status: "notFound".to_string(),
            });
        };
        let response = self
            .host
            .request("account/login/cancel", json!({ "loginId": &login_id }))
            .await?;
        let status = response
            .get("status")
            .and_then(Value::as_str)
            .ok_or_else(|| AdapterError::Rpc("account/login/cancel omitted status".to_string()))?;
        if !matches!(status, "canceled" | "notFound") {
            return Err(AdapterError::Rpc(
                "account/login/cancel returned an unexpected status".to_string(),
            ));
        }
        let mut active = self.active_login_id.write().await;
        if active.as_deref() == Some(login_id.as_str()) {
            *active = None;
        }
        self.login_statuses.write().await.remove(&login_id);
        Ok(CanceledProfileLogin {
            canceled: status == "canceled",
            status: status.to_string(),
        })
    }

    async fn profile_login_status(
        &self,
        login_id: &str,
    ) -> Result<ProfileLoginStatus, AdapterError> {
        let mut statuses = self.login_statuses.write().await;
        let status = statuses
            .get(login_id)
            .cloned()
            .ok_or_else(|| AdapterError::Rpc("Profile login was not found".to_string()))?;
        if status.completed {
            statuses.remove(login_id);
        }
        Ok(status)
    }

    async fn archive_thread(
        &self,
        workspace: &AuthorizedWorkspace,
        thread_id: &str,
    ) -> Result<(), AdapterError> {
        self.ensure_thread_bound(workspace, thread_id).await?;
        self.host
            .request("thread/archive", json!({ "threadId": thread_id }))
            .await?;
        self.thread_workspaces.write().await.remove(thread_id);
        Ok(())
    }

    async fn set_thread_name(
        &self,
        workspace: &AuthorizedWorkspace,
        thread_id: &str,
        name: &str,
    ) -> Result<(), AdapterError> {
        self.ensure_thread_bound(workspace, thread_id).await?;
        self.host
            .request(
                "thread/name/set",
                json!({ "threadId": thread_id, "name": name }),
            )
            .await?;
        Ok(())
    }

    async fn generate_text(
        &self,
        workspace: &AuthorizedWorkspace,
        prompt: &str,
        model: Option<&str>,
    ) -> Result<String, AdapterError> {
        if prompt.trim().is_empty() {
            return Err(AdapterError::Internal(
                "generation prompt is required".to_string(),
            ));
        }
        let mut events = self.host.subscribe();
        let started = self.start_thread_in_workspace(workspace).await?;
        self.suppressed_threads
            .write()
            .await
            .insert(started.thread_id.clone());
        let workspace_root = self.authorized_root(workspace)?;
        let mut params = json!({
            "threadId": &started.thread_id,
            "input": [{ "type": "text", "text": prompt.trim() }],
            "cwd": workspace_root,
            "approvalPolicy": "never",
            "sandboxPolicy": { "type": "readOnly" }
        });
        if let Some(model) = model.filter(|value| !value.trim().is_empty()) {
            params
                .as_object_mut()
                .expect("generation params are an object")
                .insert("model".to_string(), json!(model));
        }
        let thread_id = started.thread_id.clone();
        let collected = async {
            let turn = self.host.request("turn/start", params).await?;
            let turn_id = turn
                .pointer("/turn/id")
                .and_then(Value::as_str)
                .map(str::to_string);
            tokio::time::timeout(std::time::Duration::from_secs(60), async {
                let mut output = String::new();
                loop {
                    let event = events.recv().await.map_err(|error| {
                        AdapterError::Unreachable(format!(
                            "background generation stream closed: {error}"
                        ))
                    })?;
                    if message_thread_id(&event) != Some(thread_id.as_str()) {
                        continue;
                    }
                    let method = event
                        .get("method")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    if method == "item/agentMessage/delta" {
                        if let Some(delta) = event.pointer("/params/delta").and_then(Value::as_str)
                        {
                            output.push_str(delta);
                        }
                    } else if method == "turn/completed" && turn_matches(&event, turn_id.as_deref())
                    {
                        return Ok(output);
                    } else if method == "turn/error" && turn_matches(&event, turn_id.as_deref()) {
                        return Err(AdapterError::Rpc(
                            "background generation failed".to_string(),
                        ));
                    }
                }
            })
            .await
            .map_err(|_| AdapterError::Unreachable("background generation timed out".to_string()))?
        }
        .await;
        let _ = self
            .host
            .request("thread/archive", json!({ "threadId": &started.thread_id }))
            .await;
        self.thread_workspaces
            .write()
            .await
            .remove(&started.thread_id);
        let output = collected?.trim().to_string();
        if output.is_empty() {
            return Err(AdapterError::Rpc(
                "background generation returned no text".to_string(),
            ));
        }
        Ok(output)
    }

    async fn compact_thread(
        &self,
        workspace: &AuthorizedWorkspace,
        thread_id: &str,
    ) -> Result<Value, AdapterError> {
        self.ensure_thread_bound(workspace, thread_id).await?;
        self.host
            .request("thread/compact/start", json!({ "threadId": thread_id }))
            .await
            .map_err(Into::into)
    }

    async fn start_review(
        &self,
        workspace: &AuthorizedWorkspace,
        thread_id: &str,
        target: ReviewTarget,
    ) -> Result<Value, AdapterError> {
        self.ensure_thread_bound(workspace, thread_id).await?;
        let target = match target {
            ReviewTarget::UncommittedChanges => json!({ "type": "uncommittedChanges" }),
            ReviewTarget::BaseBranch { branch } => {
                json!({ "type": "baseBranch", "branch": branch })
            }
            ReviewTarget::Commit { sha, title } => {
                json!({ "type": "commit", "sha": sha, "title": title })
            }
            ReviewTarget::Custom { instructions } => {
                json!({ "type": "custom", "instructions": instructions })
            }
        };
        self.host
            .request(
                "review/start",
                json!({ "threadId": thread_id, "target": target, "delivery": "inline" }),
            )
            .await
            .map_err(Into::into)
    }

    async fn open_terminal(
        &self,
        workspace: &AuthorizedWorkspace,
        process_id: &str,
        cols: u16,
        rows: u16,
    ) -> Result<(), AdapterError> {
        let root = self.authorized_root(workspace)?;
        if process_id.trim().is_empty() || cols == 0 || rows == 0 {
            return Err(AdapterError::Internal(
                "terminal process id and non-zero size are required".to_string(),
            ));
        }
        self.terminal_workspaces
            .write()
            .await
            .insert(process_id.to_string(), workspace.clone());
        let host = self.host.clone();
        let terminal_workspaces = self.terminal_workspaces.clone();
        let local_events = self.local_events.clone();
        let process_id = process_id.to_string();
        let workspace_id = workspace.id.clone();
        tokio::spawn(async move {
            let result = host
                .request_long_running(
                    "command/exec",
                    json!({
                        "command": terminal_command(),
                        "processId": process_id,
                        "tty": true,
                        "streamStdin": true,
                        "streamStdoutStderr": true,
                        "disableOutputCap": true,
                        "disableTimeout": true,
                        "cwd": root,
                        "size": { "cols": cols, "rows": rows },
                        "permissionProfile": ":workspace",
                    }),
                )
                .await;
            terminal_workspaces.write().await.remove(&process_id);
            let (exit_code, error) = match result {
                Ok(value) => (value.get("exitCode").and_then(Value::as_i64), None),
                Err(error) => (None, Some(error.to_string())),
            };
            let _ = local_events.send(json!({
                "method": "platform/terminalExited",
                "params": {
                    "processId": process_id,
                    "workspaceId": workspace_id,
                    "exitCode": exit_code,
                    "failed": error.is_some(),
                }
            }));
        });
        Ok(())
    }

    async fn write_terminal(
        &self,
        workspace: &AuthorizedWorkspace,
        process_id: &str,
        data: &str,
    ) -> Result<(), AdapterError> {
        self.require_terminal(workspace, process_id).await?;
        self.host
            .request(
                "command/exec/write",
                json!({
                    "processId": process_id,
                    "deltaBase64": BASE64.encode(data.as_bytes()),
                }),
            )
            .await?;
        Ok(())
    }

    async fn resize_terminal(
        &self,
        workspace: &AuthorizedWorkspace,
        process_id: &str,
        cols: u16,
        rows: u16,
    ) -> Result<(), AdapterError> {
        self.require_terminal(workspace, process_id).await?;
        if cols == 0 || rows == 0 {
            return Err(AdapterError::Internal(
                "terminal size must be non-zero".to_string(),
            ));
        }
        self.host
            .request(
                "command/exec/resize",
                json!({ "processId": process_id, "size": { "cols": cols, "rows": rows } }),
            )
            .await?;
        Ok(())
    }

    async fn close_terminal(
        &self,
        workspace: &AuthorizedWorkspace,
        process_id: &str,
    ) -> Result<(), AdapterError> {
        self.require_terminal(workspace, process_id).await?;
        self.host
            .request("command/exec/terminate", json!({ "processId": process_id }))
            .await?;
        Ok(())
    }

    async fn subscribe_events(&self, sender: UnboundedSender<Vec<u8>>) -> Result<(), AdapterError> {
        let mut receiver = self.host.subscribe();
        let mut local_receiver = self.local_events.subscribe();
        loop {
            let received = tokio::select! {
                message = receiver.recv() => message,
                message = local_receiver.recv() => message,
            };
            match received {
                Ok(message) => {
                    if let Some((login_id, success, error)) = login_completion(&message) {
                        self.login_statuses.write().await.insert(
                            login_id.clone(),
                            ProfileLoginStatus {
                                completed: true,
                                success: Some(success),
                                error,
                            },
                        );
                        let mut active = self.active_login_id.write().await;
                        if active.as_deref() == Some(login_id.as_str()) {
                            *active = None;
                        }
                    }
                    if let Some(thread_id) = message_thread_id(&message) {
                        if self.suppressed_threads.read().await.contains(thread_id) {
                            continue;
                        }
                    }
                    let workspace_id = if let Some(workspace_id) = message_workspace_id(&message) {
                        workspace_id.to_string()
                    } else if let Some(process_id) = message_process_id(&message) {
                        self.terminal_workspaces
                            .read()
                            .await
                            .get(process_id)
                            .map(|workspace| workspace.id.as_str())
                            .unwrap_or(&self.workspace_id)
                            .to_string()
                    } else {
                        match message_thread_id(&message) {
                            Some(thread_id) => self
                                .thread_workspaces
                                .read()
                                .await
                                .get(thread_id)
                                .map(|workspace| workspace.id.as_str())
                                .unwrap_or(&self.workspace_id)
                                .to_string(),
                            None => self.workspace_id.clone(),
                        }
                    };
                    let frame = app_server_event_frame(&workspace_id, message)?;
                    if sender.send(frame).is_err() {
                        return Ok(());
                    }
                }
                Err(broadcast::error::RecvError::Lagged(count)) => {
                    let frame = app_server_event_frame(
                        &self.workspace_id,
                        json!({
                            "method": "codex/eventLagged",
                            "params": { "dropped": count },
                        }),
                    )?;
                    if sender.send(frame).is_err() {
                        return Ok(());
                    }
                }
                Err(broadcast::error::RecvError::Closed) => {
                    return Err(AdapterError::Unreachable(
                        "Profile Host event stream closed".to_string(),
                    ));
                }
            }
        }
    }
}

impl RealCodexAdapter {
    async fn require_terminal(
        &self,
        workspace: &AuthorizedWorkspace,
        process_id: &str,
    ) -> Result<(), AdapterError> {
        self.authorized_root(workspace)?;
        let bound = self
            .terminal_workspaces
            .read()
            .await
            .get(process_id)
            .cloned();
        if bound.as_ref() != Some(workspace) {
            return Err(AdapterError::Rpc(
                "terminal is not bound to the authorized workspace".to_string(),
            ));
        }
        Ok(())
    }
}

fn terminal_command() -> Vec<String> {
    #[cfg(windows)]
    {
        vec!["powershell.exe".to_string(), "-NoLogo".to_string()]
    }
    #[cfg(not(windows))]
    {
        vec![
            std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string()),
            "-l".to_string(),
        ]
    }
}

fn message_thread_id(message: &Value) -> Option<&str> {
    message
        .pointer("/params/threadId")
        .or_else(|| message.pointer("/params/thread_id"))
        .or_else(|| message.pointer("/params/thread/id"))
        .and_then(Value::as_str)
}

fn message_process_id(message: &Value) -> Option<&str> {
    message.pointer("/params/processId").and_then(Value::as_str)
}

fn message_workspace_id(message: &Value) -> Option<&str> {
    message
        .pointer("/params/workspaceId")
        .and_then(Value::as_str)
}

fn turn_matches(message: &Value, expected: Option<&str>) -> bool {
    expected.is_none_or(|expected| {
        message
            .pointer("/params/turnId")
            .or_else(|| message.pointer("/params/turn/id"))
            .and_then(Value::as_str)
            == Some(expected)
    })
}

fn app_server_event_frame(workspace_id: &str, message: Value) -> Result<Vec<u8>, AdapterError> {
    let envelope = json!({
        "method": "app-server-event",
        "params": {
            "workspace_id": workspace_id,
            "message": message,
        },
    });
    let mut frame = b"data: ".to_vec();
    serde_json::to_writer(&mut frame, &envelope)
        .map_err(|error| AdapterError::Internal(format!("failed to encode event: {error}")))?;
    frame.extend_from_slice(b"\n\n");
    Ok(frame)
}

#[cfg(test)]
mod tests {
    use super::{app_server_event_frame, login_completion, message_thread_id};
    use serde_json::{json, Value};

    #[test]
    fn wraps_native_notifications_in_the_existing_internal_event_envelope() {
        let frame = app_server_event_frame(
            "workspace-1",
            json!({
                "method": "thread/started",
                "params": { "thread": { "id": "thread-1" } },
            }),
        )
        .expect("event frame");
        let payload = frame
            .strip_prefix(b"data: ")
            .and_then(|value| value.strip_suffix(b"\n\n"))
            .expect("SSE data frame");
        let value: Value = serde_json::from_slice(payload).expect("valid event JSON");

        assert_eq!(value["params"]["workspace_id"], "workspace-1");
        assert_eq!(value["params"]["message"]["method"], "thread/started");
    }

    #[test]
    fn finds_thread_ids_in_notification_variants() {
        assert_eq!(
            message_thread_id(&json!({"params": {"threadId": "thread-1"}})),
            Some("thread-1")
        );
        assert_eq!(
            message_thread_id(&json!({"params": {"thread": {"id": "thread-2"}}})),
            Some("thread-2")
        );
    }

    #[test]
    fn captures_only_typed_login_completion_fields() {
        assert_eq!(
            login_completion(&json!({
                "method": "account/login/completed",
                "params": {
                    "loginId": "login-1",
                    "success": false,
                    "error": " authorization failed ",
                    "authorization": "must-not-be-projected"
                }
            })),
            Some((
                "login-1".to_string(),
                false,
                Some("authorization failed".to_string())
            ))
        );
        assert_eq!(
            login_completion(&json!({"method": "account/updated", "params": {}})),
            None
        );
    }
}
