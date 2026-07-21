pub mod fake;
pub mod real;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
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

    #[error("Profile Host error: {0}")]
    ProfileHost(#[from] open_web_codex_profile_host::ProfileHostError),

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

/// Events emitted by the adapter for durable platform projection.
#[derive(Debug, Clone)]
pub struct AdapterEvent {
    pub data: Vec<u8>,
}

/// Server-authorized workspace context for typed Runtime calls. The root is an
/// internal Runner path and must never be constructed from a browser payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedWorkspace {
    pub id: String,
    pub root: PathBuf,
}

/// Stable result of starting a Codex Thread in an authorized workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartedThread {
    pub thread_id: String,
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
/// directly to an app-server transport or the Codex process.
#[async_trait]
pub trait CodexAdapter: Send + Sync {
    /// Quick health probe.
    async fn health(&self) -> Result<HealthStatus, AdapterError>;

    /// Transitional internal RPC call. Product routes should replace this
    /// with typed Thread/Turn operations; it is never a public browser
    /// contract.
    async fn rpc(&self, method: &str, params: Value) -> Result<Value, AdapterError>;

    /// Start a Thread in a server-owned, authorization-checked workspace.
    async fn start_thread(
        &self,
        workspace: &AuthorizedWorkspace,
    ) -> Result<StartedThread, AdapterError>;

    /// Start a user turn in the authorized workspace bound to the Thread.
    async fn send_user_message(
        &self,
        workspace: &AuthorizedWorkspace,
        thread_id: &str,
        text: &str,
    ) -> Result<Value, AdapterError>;

    /// Interrupt the active Turn identified by the durable platform
    /// projection. The caller must authorize both the Thread and workspace.
    async fn interrupt_turn(
        &self,
        workspace: &AuthorizedWorkspace,
        thread_id: &str,
        turn_id: &str,
    ) -> Result<(), AdapterError>;

    /// Internal response path for app-server initiated requests. Public routes
    /// must resolve a platform-owned durable request before calling this.
    async fn respond_to_server_request(
        &self,
        request_id: Value,
        result: Value,
    ) -> Result<(), AdapterError>;

    /// Subscribe to the internal app-server event stream. The implementor sends
    /// frames through `sender` and returns when the subscription ends.
    async fn subscribe_events(
        &self,
        sender: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    ) -> Result<(), AdapterError>;
}
