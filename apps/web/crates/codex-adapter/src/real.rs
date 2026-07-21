use async_trait::async_trait;
use open_web_codex_profile_host::{ProfileHost, ProfileHostConfig, ProfileHostState};
use serde_json::{json, Value};
use std::path::PathBuf;
use tokio::sync::broadcast;
use tokio::sync::mpsc::UnboundedSender;

use crate::{AdapterError, CodexAdapter, HealthStatus};

/// Adapter backed directly by a native Profile Host and Codex app-server
/// JSONL connection. No Tauri daemon or loopback RPC/SSE hop is involved.
pub struct RealCodexAdapter {
    host: ProfileHost,
    workspace_id: String,
    workspace_root: String,
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
        let workspace_root = workspace_root.to_string_lossy().to_string();
        Ok(Self {
            host,
            workspace_id: workspace_id.into(),
            workspace_root,
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

    async fn start_thread(&self, params: Value) -> Result<Value, AdapterError> {
        self.require_workspace(&params)?;
        let result = self
            .host
            .request(
                "thread/start",
                json!({
                    "cwd": self.workspace_root,
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
        Ok(json!({
            "threadId": thread_id,
            "thread": result.get("thread").cloned().unwrap_or(Value::Null),
        }))
    }

    async fn send_user_message(&self, params: Value) -> Result<Value, AdapterError> {
        let thread_id = params
            .get("threadId")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| AdapterError::Internal("missing threadId".to_string()))?;
        let text = params
            .get("text")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| AdapterError::Internal("missing message text".to_string()))?;
        if let Some(workspace_id) = params.get("workspaceId") {
            self.require_workspace(&json!({ "workspaceId": workspace_id }))?;
        }

        let result = self
            .host
            .request(
                "turn/start",
                json!({
                    "threadId": thread_id,
                    "input": [{ "type": "text", "text": text }],
                    "cwd": self.workspace_root,
                    "approvalPolicy": "on-request",
                    "sandboxPolicy": {
                        "type": "workspaceWrite",
                        "writableRoots": [self.workspace_root],
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
            "start_thread" => self.start_thread(params).await,
            "send_user_message" => self.send_user_message(params).await,
            other => Err(AdapterError::NotImplemented(format!(
                "native Profile Host adapter method '{other}' is not available through the transitional RPC interface"
            ))),
        }
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
                    let frame = app_server_event_frame(&self.workspace_id, message)?;
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
    use super::app_server_event_frame;
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
}
