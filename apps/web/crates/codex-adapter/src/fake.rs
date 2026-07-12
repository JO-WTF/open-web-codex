use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;
use chrono::Utc;
use serde_json::{json, Value};
use tokio::sync::Mutex;
use tokio::sync::mpsc::UnboundedSender;
use uuid::Uuid;

use crate::{AdapterError, CodexAdapter, HealthStatus};

struct FakeState {
    workspaces: Vec<Value>,
    threads: Vec<Value>,
    next_thread: u64,
}

impl FakeState {
    fn new() -> Self {
        Self {
            workspaces: vec![],
            threads: vec![],
            next_thread: 0,
        }
    }
}

/// An in-memory Codex adapter that simulates workspace and thread operations.
pub struct FakeCodexAdapter {
    state: Arc<Mutex<FakeState>>,
    counter: Arc<AtomicU64>,
    name: &'static str,
    version: &'static str,
}

impl FakeCodexAdapter {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(FakeState::new())),
            counter: Arc::new(AtomicU64::new(1)),
            name: "open-web-codex-mock",
            version: "0.1.0-mock",
        }
    }

    /// Pre-populate with a sample workspace.
    pub async fn with_demo_workspace(self) -> Self {
        {
            let mut state = self.state.lock().await;
            state.workspaces.push(json!({
                "id": Uuid::now_v7().to_string(),
                "name": "demo-project",
                "path": "/tmp/demo-workspace",
                "connected": false,
                "kind": "main",
                "settings": {
                    "sidebarCollapsed": false,
                },
            }));
        }
        self
    }
}

#[async_trait]
impl CodexAdapter for FakeCodexAdapter {
    async fn health(&self) -> Result<HealthStatus, AdapterError> {
        Ok(HealthStatus {
            ok: true,
            version: self.version.to_string(),
            name: self.name.to_string(),
        })
    }

    async fn rpc(&self, method: &str, params: Value) -> Result<Value, AdapterError> {
        match method {
            "list_workspaces" => {
                let state = self.state.lock().await;
                Ok(Value::Array(state.workspaces.clone()))
            }

            "add_workspace" => {
                let path = params["path"]
                    .as_str()
                    .ok_or_else(|| AdapterError::Internal("missing path".into()))?
                    .to_string();
                let mut state = self.state.lock().await;
                let id = Uuid::now_v7().to_string();
                let name = path
                    .rsplit('/')
                    .next()
                    .unwrap_or("workspace")
                    .to_string();
                let ws = json!({
                    "id": id,
                    "name": name,
                    "path": path,
                    "connected": false,
                    "kind": "main",
                    "settings": { "sidebarCollapsed": false },
                });
                state.workspaces.push(ws.clone());
                Ok(ws)
            }

            "connect_workspace" => {
                let id = params["id"]
                    .as_str()
                    .ok_or_else(|| AdapterError::Internal("missing id".into()))?;
                let mut state = self.state.lock().await;
                for ws in &mut state.workspaces {
                    if ws["id"] == id {
                        ws["connected"] = json!(true);
                        return Ok(json!({}));
                    }
                }
                Err(AdapterError::Rpc(format!("workspace not found: {id}")))
            }

            "start_thread" => {
                let ws_id = params["workspaceId"]
                    .as_str()
                    .ok_or_else(|| AdapterError::Internal("missing workspaceId".into()))?;
                let state = self.state.lock().await;
                if !state.workspaces.iter().any(|w| w["id"] == ws_id) {
                    return Err(AdapterError::Rpc(format!("workspace not found: {ws_id}")));
                }
                let n = self.counter.fetch_add(1, Ordering::SeqCst);
                let thread_id = format!("mock-thread-{n:04x}");
                let created = Utc::now().to_rfc3339();
                Ok(json!({
                    "threadId": thread_id,
                    "createdAt": created,
                }))
            }

            "list_threads" => {
                Ok(json!({
                    "threads": [],
                    "totalCount": 0,
                }))
            }

            "send_user_message" => {
                let text = params["text"].as_str().unwrap_or("");
                tracing::info!(text = %text, "fake: received user message");
                Ok(json!({}))
            }

            other => Err(AdapterError::NotImplemented(format!(
                "fake adapter: method '{other}' not implemented"
            ))),
        }
    }

    async fn subscribe_events(
        &self,
        sender: UnboundedSender<Vec<u8>>,
    ) -> Result<(), AdapterError> {
        let state = self.state.clone();
        let counter = self.counter.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5));
            loop {
                interval.tick().await;
                let n = counter.fetch_add(1, Ordering::SeqCst);
                let event = json!({
                    "method": "app-server-event",
                    "params": {
                        "workspace_id": null,
                        "message": {
                            "method": "heartbeat",
                            "params": { "seq": n },
                        },
                    },
                });
                let frame = format!("data: {}\n\n", event.to_string());
                if sender.send(frame.into_bytes()).is_err() {
                    break;
                }

                let ws_count = state.try_lock().map(|s| s.workspaces.len()).unwrap_or(0);
                if ws_count > 0 && n % 3 == 0 {
                    let thread_id = format!("mock-thread-auto-{n:04x}");
                    let started = json!({
                        "method": "app-server-event",
                        "params": {
                            "workspace_id": null,
                            "message": {
                                "method": "thread/started",
                                "params": { "threadId": thread_id },
                            },
                        },
                    });
                    let frame = format!("data: {}\n\n", started.to_string());
                    if sender.send(frame.into_bytes()).is_err() {
                        break;
                    }
                }
            }
        });

        Ok(())
    }
}
