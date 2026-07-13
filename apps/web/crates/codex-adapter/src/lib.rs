pub mod fake;
pub mod real;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

/// Errors from the Codex adapter layer.
#[derive(Debug, Error)]
pub enum AdapterError {
    #[error("Codex runtime unreachable: {0}")]
    Unreachable(String),

    #[error("RPC error: {0}")]
    Rpc(String),

    #[error("Method not implemented: {0}")]
    NotImplemented(String),

    #[error("Transport error: {0}")]
    Transport(#[from] reqwest::Error),

    #[error("Internal error: {0}")]
    Internal(String),
}

/// Health status returned by the adapter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub ok: bool,
    pub version: String,
    pub name: String,
}

/// Events emitted by the adapter for SSE streaming.
#[derive(Debug, Clone)]
pub struct AdapterEvent {
    pub data: Vec<u8>,
}

/// Mode for the Codex adapter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AdapterMode {
    /// Use a real (or proxied) Codex runtime.
    Real,
    /// Use an in-memory fake that simulates responses.
    Fake,
}

impl std::fmt::Display for AdapterMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Real => write!(f, "real"),
            Self::Fake => write!(f, "fake"),
        }
    }
}

/// Abstract interface to the Codex runtime.
///
/// Platform server routes call through this trait instead of talking
/// directly to the Tauri daemon or the Codex process.
#[async_trait]
pub trait CodexAdapter: Send + Sync {
    /// Quick health probe.
    async fn health(&self) -> Result<HealthStatus, AdapterError>;

    /// JSON-RPC call. `method` is e.g. "list_workspaces".
    async fn rpc(&self, method: &str, params: Value) -> Result<Value, AdapterError>;

    /// Subscribe to the SSE event stream. The implementor sends SSE frames
    /// through `sender` and returns when the subscription ends (sender dropped).
    async fn subscribe_events(
        &self,
        sender: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    ) -> Result<(), AdapterError>;
}
