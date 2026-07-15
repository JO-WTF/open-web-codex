use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::Utc;
use serde_json::{json, Value};
use tokio::sync::Mutex;
use tokio::sync::mpsc::UnboundedSender;
use uuid::Uuid;

use crate::{AdapterError, CodexAdapter, HealthStatus};

const MANIFEST_FIXTURE: &str =
    include_str!("../../../contracts/codex/fixtures/capability-manifest.v1.json");

/// A tracked mock thread for list/show responses.
#[derive(Clone)] struct MockThread {
    id: String, ws_id: String, created_at: String, status: String,
    msg_count: u64, updated_at: i64,
}

/// In-memory state shared between RPC handlers and event generator.
struct FakeState {
    workspaces: Vec<Value>,
    threads: Vec<MockThread>,
    /// Events queued by RPC handlers (e.g. thread/started).
    pending_events: Vec<Value>,
    current_provider_id: String,
    providers: Vec<Value>,
    models: Vec<Value>,
    thread_settings: HashMap<String, Value>,
}

/// In-memory Codex adapter that simulates workspace, thread and event flows.
#[derive(Clone)]
pub struct FakeCodexAdapter {
    state: Arc<Mutex<FakeState>>,
    notify: Arc<tokio::sync::Notify>,
    counter: Arc<AtomicU64>,
}

impl Default for FakeCodexAdapter {
    fn default() -> Self { Self::new() }
}

impl FakeCodexAdapter {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(FakeState {
                workspaces: vec![],
                threads: vec![],
                pending_events: vec![],
                current_provider_id: "openai".to_string(),
                providers: default_providers(),
                models: default_models(),
                thread_settings: HashMap::new(),
            })),
            notify: Arc::new(tokio::sync::Notify::new()),
            counter: Arc::new(AtomicU64::new(1)),
        }
    }

    fn provider_catalog(state: &FakeState) -> Value {
        let current = &state.current_provider_id;
        let data = state
            .providers
            .iter()
            .map(|provider| {
                let mut entry = provider.clone();
                if let Some(object) = entry.as_object_mut() {
                    let id = object
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    object.insert("isCurrent".to_string(), json!(id == current));
                    if let Some(models) = object.get("models").and_then(Value::as_array) {
                        object.insert("modelCount".to_string(), json!(models.len()));
                    }
                }
                entry
            })
            .collect::<Vec<_>>();
        json!({
            "data": data,
            "currentProviderId": current,
        })
    }

    fn model_list_response(state: &FakeState) -> Value {
        json!({
            "data": state.models.clone(),
            "nextCursor": Value::Null,
        })
    }
}

fn default_providers() -> Vec<Value> {
    vec![
        json!({
            "id": "openai",
            "name": "OpenAI",
            "baseUrl": null,
            "envKey": "OPENAI_API_KEY",
            "wireApi": "responses",
            "kind": "builtIn",
            "isCurrent": true,
            "modelCount": 0,
            "canEdit": false,
            "canDelete": false,
            "canFetchModels": false,
            "models": [],
        }),
        json!({
            "id": "deepseek",
            "name": "DeepSeek",
            "baseUrl": "https://api.deepseek.com",
            "envKey": "DEEPSEEK_API_KEY",
            "wireApi": "responses",
            "kind": "custom",
            "isCurrent": false,
            "modelCount": 1,
            "canEdit": true,
            "canDelete": true,
            "canFetchModels": true,
            "models": [{
                "modelId": "deepseek-chat",
                "modelName": "DeepSeek Chat",
                "contextWindow": 65536,
            }],
        }),
    ]
}

fn default_models() -> Vec<Value> {
    vec![
        json!({
            "id": "gpt-5.4",
            "model": "gpt-5.4",
            "displayName": "GPT-5.4",
            "isDefault": true,
        }),
        json!({
            "id": "deepseek-chat",
            "model": "deepseek-chat",
            "displayName": "DeepSeek Chat",
            "isDefault": false,
        }),
    ]
}

impl FakeCodexAdapter {

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

    fn approval_request_event(ws_id: &str, th_id: &str, request_id: u64) -> Value {
        json!({
            "method": "app-server-event",
            "params": {
                "workspace_id": ws_id,
                "message": {
                    "id": request_id,
                    "method": "item/commandExecution/requestApproval",
                    "params": {
                        "threadId": th_id,
                        "command": ["echo", "hello"],
                        "reason": "mock approval for smoke tests",
                    },
                },
            },
        })
    }
}

#[async_trait]
impl CodexAdapter for FakeCodexAdapter {
    async fn health(&self) -> Result<HealthStatus, AdapterError> {
        Ok(HealthStatus { ok: true, version: "0.1.0-mock".into(), name: "open-web-codex-mock".into() })
    }

    async fn rpc(&self, method: &str, params: Value) -> Result<Value, AdapterError> {
        match method {
            "initialize" => {
                let manifest: Value = serde_json::from_str(MANIFEST_FIXTURE).map_err(|error| {
                    AdapterError::Internal(format!("manifest fixture invalid: {error}"))
                })?;
                Ok(json!({ "manifest": manifest }))
            }

            "respond_to_server_request" => Ok(json!({ "ok": true })),

            "list_workspaces" => {
                let s = self.state.lock().await;
                Ok(Value::Array(s.workspaces.clone()))
            }

            "add_workspace" => {
                let path = params["path"].as_str()
                    .ok_or_else(|| AdapterError::Internal("missing path".into()))?.to_string();
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
                let id = params["id"].as_str()
                    .ok_or_else(|| AdapterError::Internal("missing id".into()))?;
                let mut s = self.state.lock().await;
                for ws in &mut s.workspaces {
                    if ws["id"] == id { ws["connected"] = json!(true); return Ok(json!({})); }
                }
                Err(AdapterError::Rpc(format!("workspace not found: {id}")))
            }

            "start_thread" => {
                let ws_id = params["workspaceId"].as_str()
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
                        id: thread_id.clone(), ws_id: ws_id.to_string(),
                        created_at: created.clone(), status: "active".into(),
                        msg_count: 0, updated_at: updated_ts,
                    });
                }
                // Emit thread/started immediately; defer approval until after start_thread returns
                // so event_projection can resolve codex_thread_id on the run row.
                self.emit(Self::started_event(ws_id, &thread_id)).await;
                let adapter = self.clone();
                let ws_id = ws_id.to_string();
                let thread_id_for_approval = thread_id.clone();
                tokio::spawn(async move {
                    tokio::task::yield_now().await;
                    let request_id = adapter.counter.fetch_add(1, Ordering::SeqCst);
                    adapter
                        .emit(Self::approval_request_event(
                            &ws_id,
                            &thread_id_for_approval,
                            request_id,
                        ))
                        .await;
                });

                Ok(json!({ "threadId": thread_id, "createdAt": created }))
            }

            "list_threads" => {
                let ws_id = params["workspaceId"].as_str().unwrap_or("");
                let s = self.state.lock().await;
                let threads: Vec<Value> = s.threads.iter()
                    .filter(|t| t.ws_id == ws_id)
                    .map(|t| json!({
                        "id": t.id, "name": format!("Fake Thread ({})", &t.id[13..]),
                        "createdAt": t.created_at, "updatedAt": t.updated_at,
                        "messageCount": t.msg_count, "status": t.status,
                    }))
                    .collect();
                Ok(json!({ "threads": threads, "totalCount": threads.len() }))
            }

            "send_user_message" => {
                let th_id = params["threadId"]
                    .as_str()
                    .or_else(|| params["thread_id"].as_str())
                    .unwrap_or("");
                let text = params["text"].as_str().unwrap_or("");
                let model = params.get("model").and_then(Value::as_str);
                tracing::info!(thread = %th_id, text = %text, model = ?model, "fake: user message");

                {
                    let mut s = self.state.lock().await;
                    if let Some(th) = s.threads.iter_mut().find(|t| t.id == th_id) {
                        th.msg_count += 1;
                        th.updated_at = Utc::now().timestamp_millis();
                    }
                }

                Ok(json!({ "status": "sent" }))
            }

            "model_provider_list" => Ok({
                let s = self.state.lock().await;
                Self::provider_catalog(&s)
            }),

            "model_list" => Ok({
                let s = self.state.lock().await;
                Self::model_list_response(&s)
            }),

            "model_provider_write" => {
                let input = params
                    .get("input")
                    .cloned()
                    .unwrap_or_else(|| params.clone());
                let object = input
                    .as_object()
                    .ok_or_else(|| AdapterError::Rpc("provider mutation must be an object".into()))?;
                let action = object
                    .get("action")
                    .and_then(Value::as_str)
                    .ok_or_else(|| AdapterError::Rpc("missing provider action".into()))?;
                let id = object
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| AdapterError::Rpc("provider id is required".into()))?;

                let mut s = self.state.lock().await;
                match action {
                    "select" => {
                        if !s.providers.iter().any(|provider| provider["id"] == id) {
                            return Err(AdapterError::Rpc(format!("provider '{id}' does not exist")));
                        }
                        s.current_provider_id = id.to_string();
                    }
                    "context" => {
                        let provider = s
                            .providers
                            .iter_mut()
                            .find(|provider| provider["id"] == id)
                            .ok_or_else(|| AdapterError::Rpc(format!("provider '{id}' does not exist")))?;
                        if provider["kind"] == "builtIn" {
                            return Err(AdapterError::Rpc(
                                "built-in model metadata cannot be edited".into(),
                            ));
                        }
                        let model_id = object
                            .get("modelId")
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .ok_or_else(|| AdapterError::Rpc("model id is required".into()))?;
                        let context_window = object
                            .get("contextWindow")
                            .and_then(Value::as_i64)
                            .filter(|value| *value >= 1024)
                            .ok_or_else(|| {
                                AdapterError::Rpc(
                                    "context window must be at least 1024 tokens".into(),
                                )
                            })?;
                        if let Some(models) = provider
                            .get_mut("models")
                            .and_then(Value::as_array_mut)
                        {
                            if let Some(entry) = models
                                .iter_mut()
                                .find(|entry| entry["modelId"] == model_id)
                            {
                                entry["contextWindow"] = json!(context_window);
                            } else {
                                models.push(json!({
                                    "modelId": model_id,
                                    "modelName": model_id,
                                    "contextWindow": context_window,
                                }));
                            }
                            let count = models.len();
                            provider["modelCount"] = json!(count);
                        }
                    }
                    "upsert" => {
                        if s.providers.iter().any(|provider| provider["id"] == id) {
                            return Err(AdapterError::Rpc(format!(
                                "provider '{id}' already exists; use edit flow"
                            )));
                        }
                        let name = object
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or(id);
                        let base_url = object
                            .get("baseUrl")
                            .and_then(Value::as_str)
                            .unwrap_or("");
                        s.providers.push(json!({
                            "id": id,
                            "name": name,
                            "baseUrl": base_url,
                            "envKey": object.get("envKey").cloned().unwrap_or(Value::Null),
                            "wireApi": object.get("wireApi").and_then(Value::as_str).unwrap_or("responses"),
                            "kind": "custom",
                            "isCurrent": false,
                            "modelCount": 0,
                            "canEdit": true,
                            "canDelete": true,
                            "canFetchModels": true,
                            "models": [],
                        }));
                        if object.get("select").and_then(Value::as_bool) == Some(true) {
                            s.current_provider_id = id.to_string();
                        }
                    }
                    "delete" => {
                        if id == "openai" {
                            return Err(AdapterError::Rpc(
                                "built-in provider 'openai' cannot be deleted".into(),
                            ));
                        }
                        if s.current_provider_id == id {
                            return Err(AdapterError::Rpc(
                                "select another provider before deleting the current provider"
                                    .into(),
                            ));
                        }
                        s.providers.retain(|provider| provider["id"] != id);
                    }
                    "fetch" => {
                        if !s.providers.iter().any(|provider| provider["id"] == id) {
                            return Err(AdapterError::Rpc(format!("provider '{id}' does not exist")));
                        }
                        s.current_provider_id = id.to_string();
                        return Ok(Self::model_list_response(&s));
                    }
                    other => {
                        return Err(AdapterError::Rpc(format!(
                            "unsupported provider action '{other}'"
                        )));
                    }
                }
                Ok(Self::provider_catalog(&s))
            }

            "thread_settings_update" => {
                let thread_id = params["threadId"]
                    .as_str()
                    .ok_or_else(|| AdapterError::Rpc("missing threadId".into()))?;
                let settings = params
                    .get("settings")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                let mut s = self.state.lock().await;
                s.thread_settings.insert(thread_id.to_string(), settings);
                Ok(json!({ "status": "updated" }))
            }

            "turn_interrupt" | "turn_steer" => Ok(json!({ "status": "ok" })),

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
