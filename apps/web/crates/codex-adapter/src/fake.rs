use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::Utc;
use serde_json::{json, Value};
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::{AdapterError, AuthorizedWorkspace, CodexAdapter, HealthStatus, StartedThread};

/// A tracked mock thread for list/show responses.
#[derive(Clone)]
struct MockThread {
    id: String,
    ws_id: String,
    created_at: String,
    status: String,
    msg_count: u64,
    updated_at: i64,
}

/// In-memory state shared between RPC handlers and event generator.
struct FakeState {
    workspaces: Vec<Value>,
    threads: Vec<MockThread>,
    /// Events queued by RPC handlers (e.g. thread/started).
    pending_events: Vec<Value>,
}

/// In-memory Codex adapter that simulates workspace, thread and event flows.
pub struct FakeCodexAdapter {
    state: Arc<Mutex<FakeState>>,
    notify: Arc<tokio::sync::Notify>,
    counter: Arc<AtomicU64>,
}

impl Default for FakeCodexAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeCodexAdapter {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(FakeState {
                workspaces: vec![],
                threads: vec![],
                pending_events: vec![],
            })),
            notify: Arc::new(tokio::sync::Notify::new()),
            counter: Arc::new(AtomicU64::new(1)),
        }
    }

    /// Pre-populate with a sample workspace.
    pub async fn with_demo_workspace(self) -> Self {
        let mut state = self.state.lock().await;
        state.workspaces.push(json!({
            "id": Uuid::now_v7().to_string(),
            "name": "open-web-codex",
            "path": &std::env::current_dir()
                .unwrap_or_else(|_| "/tmp/demo".into())
                .to_string_lossy()
                .to_string(),
            "connected": false,
            "kind": "main",
            "settings": { "sidebarCollapsed": false },
        }));
        drop(state);
        self
    }

    /// Helper: push an SSE frame event and notify the event loop.
    async fn emit(&self, evt: Value) {
        let mut state = self.state.lock().await;
        state.pending_events.push(evt);
        self.notify.notify_one();
    }

    /// Build a thread/started SSE frame.
    fn started_event(ws_id: &str, th_id: &str) -> Value {
        json!({
            "method": "app-server-event",
            "params": {
                "workspace_id": ws_id,
                "message": {
                    "method": "thread/started",
                    "params": { "threadId": th_id },
                },
            },
        })
    }

    fn completed_event(ws_id: &str, th_id: &str) -> Value {
        json!({
            "method": "app-server-event",
            "params": {
                "workspace_id": ws_id,
                "message": {
                    "method": "thread/completed",
                    "params": { "threadId": th_id },
                },
            },
        })
    }
}

#[async_trait]
impl CodexAdapter for FakeCodexAdapter {
    async fn health(&self) -> Result<HealthStatus, AdapterError> {
        Ok(HealthStatus {
            ok: true,
            version: "0.1.0-mock".into(),
            name: "open-web-codex-mock".into(),
        })
    }

    async fn rpc(&self, method: &str, params: Value) -> Result<Value, AdapterError> {
        match method {
            "list_workspaces" => {
                let s = self.state.lock().await;
                Ok(Value::Array(s.workspaces.clone()))
            }

            "add_workspace" => {
                let path = params["path"]
                    .as_str()
                    .ok_or_else(|| AdapterError::Internal("missing path".into()))?
                    .to_string();
                let mut s = self.state.lock().await;
                let id = Uuid::now_v7().to_string();
                let name = path.rsplit('/').next().unwrap_or("workspace").to_string();
                let ws = json!({
                    "id": id, "name": name, "path": path,
                    "connected": false, "kind": "main",
                    "settings": { "sidebarCollapsed": false },
                });
                s.workspaces.push(ws.clone());
                Ok(ws)
            }

            "connect_workspace" => {
                let id = params["id"]
                    .as_str()
                    .ok_or_else(|| AdapterError::Internal("missing id".into()))?;
                let mut s = self.state.lock().await;
                for ws in &mut s.workspaces {
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
                let now = Utc::now();
                {
                    let s = self.state.lock().await;
                    if !s.workspaces.iter().any(|w| w["id"] == ws_id) {
                        return Err(AdapterError::Rpc(format!("workspace not found: {ws_id}")));
                    }
                }
                let n = self.counter.fetch_add(1, Ordering::SeqCst);
                let thread_id = format!("mock-thread-{:04x}", n);
                let created = now.to_rfc3339();
                let updated_ts = now.timestamp_millis();

                {
                    let mut s = self.state.lock().await;
                    s.threads.push(MockThread {
                        id: thread_id.clone(),
                        ws_id: ws_id.to_string(),
                        created_at: created.clone(),
                        status: "active".into(),
                        msg_count: 0,
                        updated_at: updated_ts,
                    });
                }
                // Emit thread/started event immediately
                self.emit(Self::started_event(ws_id, &thread_id)).await;

                Ok(json!({ "threadId": thread_id, "createdAt": created }))
            }

            "list_threads" => {
                let ws_id = params["workspaceId"].as_str().unwrap_or("");
                let s = self.state.lock().await;
                let threads: Vec<Value> = s
                    .threads
                    .iter()
                    .filter(|t| t.ws_id == ws_id)
                    .map(|t| {
                        json!({
                            "id": t.id, "name": format!("Fake Thread ({})", &t.id[13..]),
                            "createdAt": t.created_at, "updatedAt": t.updated_at,
                            "messageCount": t.msg_count, "status": t.status,
                        })
                    })
                    .collect();
                Ok(json!({ "threads": threads, "totalCount": threads.len() }))
            }

            "send_user_message" => {
                let ws_id = params["workspaceId"].as_str().unwrap_or("");
                let th_id = params["threadId"].as_str().unwrap_or("");
                let text = params["text"].as_str().unwrap_or("");
                tracing::info!(ws = %ws_id, thread = %th_id, text = %text, "fake: user message");

                // Increment message count
                {
                    let mut s = self.state.lock().await;
                    if let Some(th) = s.threads.iter_mut().find(|t| t.id == th_id) {
                        th.msg_count += 1;
                        th.updated_at = Utc::now().timestamp_millis();
                    }
                }

                // Schedule a mock turn after a brief delay (handled by subscribe_events)
                // For now just acknowledge
                Ok(json!({ "status": "sent" }))
            }

            other => Err(AdapterError::NotImplemented(format!(
                "fake adapter: method '{other}' not implemented"
            ))),
        }
    }

    async fn start_thread(
        &self,
        workspace: &AuthorizedWorkspace,
    ) -> Result<StartedThread, AdapterError> {
        {
            let mut state = self.state.lock().await;
            if !state
                .workspaces
                .iter()
                .any(|value| value["id"] == workspace.id)
            {
                state.workspaces.push(json!({
                    "id": workspace.id,
                    "name": workspace.id,
                    "path": workspace.root,
                    "connected": true,
                    "kind": "run",
                }));
            }
        }
        let result = self
            .rpc("start_thread", json!({ "workspaceId": workspace.id }))
            .await?;
        let thread_id = result
            .get("threadId")
            .and_then(Value::as_str)
            .ok_or_else(|| AdapterError::Rpc("fake Thread omitted id".to_string()))?;
        Ok(StartedThread {
            thread_id: thread_id.to_string(),
        })
    }

    async fn send_user_message(
        &self,
        workspace: &AuthorizedWorkspace,
        thread_id: &str,
        text: &str,
    ) -> Result<Value, AdapterError> {
        self.rpc(
            "send_user_message",
            json!({
                "workspaceId": workspace.id,
                "threadId": thread_id,
                "text": text,
            }),
        )
        .await
    }

    async fn interrupt_turn(
        &self,
        workspace: &AuthorizedWorkspace,
        thread_id: &str,
        turn_id: &str,
    ) -> Result<(), AdapterError> {
        if workspace.id.trim().is_empty()
            || thread_id.trim().is_empty()
            || turn_id.trim().is_empty()
        {
            return Err(AdapterError::Internal(
                "workspace, Thread, and Turn ids are required".to_string(),
            ));
        }
        let mut state = self.state.lock().await;
        let thread = state
            .threads
            .iter_mut()
            .find(|thread| thread.id == thread_id && thread.ws_id == workspace.id)
            .ok_or_else(|| AdapterError::Rpc("fake Thread was not found".to_string()))?;
        thread.status = "interrupted".to_string();
        Ok(())
    }

    async fn respond_to_server_request(
        &self,
        _request_id: Value,
        _result: Value,
    ) -> Result<(), AdapterError> {
        Ok(())
    }

    async fn subscribe_events(&self, sender: UnboundedSender<Vec<u8>>) -> Result<(), AdapterError> {
        let state = self.state.clone();
        let notify = self.notify.clone();
        let counter = self.counter.clone();

        tokio::spawn(async move {
            let mut heartbeat = tokio::time::interval(tokio::time::Duration::from_secs(10));
            let mut item_timer = tokio::time::interval(tokio::time::Duration::from_secs(8));
            let mut item_seq: u64 = 0;
            let mut thread_ticks: HashMap<String, u32> = HashMap::new();

            // Helper: send an SSE frame
            let send = |data: Value| {
                let frame = format!("data: {}\n\n", data.to_string());
                let _ = sender.send(frame.into_bytes());
            };

            loop {
                tokio::select! {
                    _ = notify.notified() => {
                        // Drain pending events
                        let events = {
                            let mut s = state.lock().await;
                            std::mem::take(&mut s.pending_events)
                        };
                        for evt in events { send(evt); }
                    }
                    _ = heartbeat.tick() => {
                        let n = counter.fetch_add(1, Ordering::SeqCst);
                        send(json!({
                            "method": "app-server-event",
                            "params": {
                                "workspace_id": null,
                                "message": {
                                    "method": "heartbeat",
                                    "params": { "seq": n },
                                },
                            },
                        }));
                    }
                    _ = item_timer.tick() => {
                        // For each active thread, emit mock item activity
                        let threads = {
                            let s = state.lock().await;
                            s.threads.iter()
                                .filter(|t| t.status == "active")
                                .map(|t| (t.id.clone(), t.ws_id.clone()))
                                .collect::<Vec<_>>()
                        };
                        for (th_id, ws_id) in &threads {
                            let ticks = thread_ticks.entry(th_id.clone()).or_insert(0);
                            if *ticks >= 3 {
                                // Emit thread/completed
                                send(FakeCodexAdapter::completed_event(ws_id, th_id));
                                // Mark thread completed in state
                                {
                                    let mut s = state.lock().await;
                                    if let Some(th) = s.threads.iter_mut().find(|t| t.id == *th_id) {
                                        th.status = "completed".to_string();
                                    }
                                }
                                tracing::info!(thread = %th_id, "mock thread completed after {ticks} ticks");
                                continue;
                            }
                            *ticks += 1;

                            item_seq += 1;
                            let item_id = format!("mock-item-{item_seq:04x}");

                            // Cycle through item types
                            let item_kind = match item_seq % 4 {
                                0 => "plan",
                                1 => "explore",
                                2 => "tool",
                                _ => "message",
                            };
                            let desc = match item_kind {
                                "plan" => "Analyzing project structure and dependencies",
                                "explore" => "Reading source files for context",
                                "tool" => "Running analysis tools",
                                _ => "Processing request",
                            };

                            send(json!({
                                "method": "app-server-event",
                                "params": {
                                    "workspace_id": ws_id,
                                    "message": {
                                        "method": "item/started",
                                        "params": {
                                            "threadId": th_id,
                                            "item": {
                                                "id": item_id,
                                                "kind": item_kind,
                                                "description": desc,
                                                "status": "in_progress",
                                            },
                                        },
                                    },
                                },
                            }));

                            // Mark completed shortly after (simulated by next tick's item)
                            send(json!({
                                "method": "app-server-event",
                                "params": {
                                    "workspace_id": ws_id,
                                    "message": {
                                        "method": "item/completed",
                                        "params": {
                                            "threadId": th_id,
                                            "item": {
                                                "id": item_id,
                                                "kind": item_kind,
                                                "status": "completed",
                                            },
                                        },
                                    },
                                },
                            }));
                        }
                    }
                }

                if sender.is_closed() {
                    break;
                }
            }
        });

        Ok(())
    }
}
