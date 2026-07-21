use async_trait::async_trait;
use open_web_codex_profile_host::{ProfileHost, ProfileHostConfig, ProfileHostState};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::RwLock;

use crate::{AdapterError, AuthorizedWorkspace, CodexAdapter, HealthStatus, StartedThread};

/// Adapter backed directly by a native Profile Host and Codex app-server
/// JSONL connection. No Tauri daemon or loopback RPC/SSE hop is involved.
pub struct RealCodexAdapter {
    host: ProfileHost,
    workspace_id: String,
    workspace_root: PathBuf,
    thread_workspaces: Arc<RwLock<HashMap<String, AuthorizedWorkspace>>>,
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
        Ok(Self {
            host,
            workspace_id: workspace_id.into(),
            workspace_root,
            thread_workspaces: Arc::new(RwLock::new(HashMap::new())),
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

    async fn send_user_message_in_workspace(
        &self,
        workspace: &AuthorizedWorkspace,
        thread_id: &str,
        text: &str,
    ) -> Result<Value, AdapterError> {
        if thread_id.trim().is_empty() || text.trim().is_empty() {
            return Err(AdapterError::Internal(
                "Thread id and message text are required".to_string(),
            ));
        }
        let workspace_root = self.authorized_root(workspace)?;
        let bound = self.thread_workspaces.read().await.get(thread_id).cloned();
        if bound.as_ref() != Some(workspace) {
            return Err(AdapterError::Rpc(
                "Thread is not bound to the authorized workspace".to_string(),
            ));
        }

        let result = self
            .host
            .request(
                "turn/start",
                json!({
                    "threadId": thread_id,
                    "input": [{ "type": "text", "text": text.trim() }],
                    "cwd": &workspace_root,
                    "approvalPolicy": "on-request",
                    "sandboxPolicy": {
                        "type": "workspaceWrite",
                        "writableRoots": [&workspace_root],
                        "networkAccess": true,
                    },
                }),
            )
            .await?;
        Ok(json!({
            "status": "sent",
            "turnId": result.pointer("/turn/id").cloned().unwrap_or(Value::Null),
        }))
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
        self.authorized_root(workspace)?;
        let bound = self.thread_workspaces.read().await.get(thread_id).cloned();
        if bound.as_ref() != Some(workspace) {
            return Err(AdapterError::Rpc(
                "Thread is not bound to the authorized workspace".to_string(),
            ));
        }
        self.host
            .request(
                "turn/interrupt",
                json!({ "threadId": thread_id, "turnId": turn_id }),
            )
            .await?;
        Ok(())
    }
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
                self.send_user_message_in_workspace(&workspace, thread_id, text).await
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

    async fn send_user_message(
        &self,
        workspace: &AuthorizedWorkspace,
        thread_id: &str,
        text: &str,
    ) -> Result<Value, AdapterError> {
        self.send_user_message_in_workspace(workspace, thread_id, text)
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

    async fn subscribe_events(&self, sender: UnboundedSender<Vec<u8>>) -> Result<(), AdapterError> {
        let mut receiver = self.host.subscribe();
        loop {
            match receiver.recv().await {
                Ok(message) => {
                    let workspace_id = match message_thread_id(&message) {
                        Some(thread_id) => self
                            .thread_workspaces
                            .read()
                            .await
                            .get(thread_id)
                            .map(|workspace| workspace.id.as_str())
                            .unwrap_or(&self.workspace_id)
                            .to_string(),
                        None => self.workspace_id.clone(),
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

fn message_thread_id(message: &Value) -> Option<&str> {
    message
        .pointer("/params/threadId")
        .or_else(|| message.pointer("/params/thread_id"))
        .or_else(|| message.pointer("/params/thread/id"))
        .and_then(Value::as_str)
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
    use super::{app_server_event_frame, message_thread_id};
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
}
