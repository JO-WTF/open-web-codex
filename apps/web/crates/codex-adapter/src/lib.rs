pub mod fake;
pub mod real;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use thiserror::Error;
use uuid::Uuid;

/// Errors from the Codex adapter layer.
#[derive(Debug, Error)]
pub enum AdapterError {
    #[error("Codex runtime unreachable: {0}")]
    Unreachable(String),

    #[error("RPC error: {0}")]
    Rpc(String),

    #[error("Method not implemented: {0}")]
    NotImplemented(String),

    #[error("Required Runtime capabilities unavailable: {0}")]
    CapabilityUnavailable(String),

    #[error("Profile Host error: {0}")]
    ProfileHost(#[from] open_web_codex_profile_host::ProfileHostError),

    #[error("Internal error: {0}")]
    Internal(String),
}

/// Health status returned by the adapter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub ok: bool,
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

/// Bounded Runtime-owned identity metadata attached to internal event frames.
///
/// This is consumed by the Platform event projection to associate an event
/// with the authoritative child Thread before projecting durable provenance.
/// It is never exposed as a browser DTO or sent back through the Runtime API.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeThreadIdentity {
    pub thread_id: String,
    pub parent_thread_id: String,
    pub source_kind: String,
    pub agent_path: Option<String>,
    pub agent_nickname: Option<String>,
    pub agent_role: Option<String>,
}

/// Internal event-envelope result for Runtime child identity hydration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RuntimeThreadIdentitySidecar {
    Resolved(RuntimeThreadIdentity),
    Unavailable { thread_id: String },
}

/// Stable result of starting a Codex Thread in an authorized workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartedThread {
    pub thread_id: String,
}

/// Browser-login handoff returned by the official app-server. The platform
/// exposes only the opaque login id and the URL the browser must open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartedProfileLogin {
    pub login_id: String,
    pub auth_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanceledProfileLogin {
    pub canceled: bool,
    pub status: String,
}

/// Typed, platform-safe projection of the official login completion event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileLoginStatus {
    pub completed: bool,
    pub success: Option<bool>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct TurnOptions {
    /// Browser-generated stable identity for a normal user message. The
    /// Platform validates it before this typed adapter boundary.
    pub client_user_message_id: Option<String>,
    pub effort: Option<String>,
    pub service_tier: Option<String>,
    pub access_mode: Option<String>,
    pub images: Vec<String>,
    pub collaboration_mode: Option<Value>,
    /// Server-resolved package persisted by the owning Task. This is never
    /// accepted as a Runtime path or arbitrary configuration value.
    pub copilot_package_id: Option<String>,
}

/// Exact Runtime-owned model settings for one materialized Thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadModelSettings {
    pub model_provider: String,
    pub model: String,
}

#[derive(Debug, Clone)]
pub enum ProfileQuery {
    Account,
    RateLimits,
    CollaborationModes,
    Apps {
        cursor: Option<String>,
        limit: Option<u32>,
        thread_id: Option<String>,
    },
    McpServers {
        cursor: Option<String>,
        limit: Option<u32>,
        thread_id: Option<String>,
    },
    ExperimentalFeatures {
        cursor: Option<String>,
        limit: Option<u32>,
        thread_id: Option<String>,
    },
    Skills {
        workspace: AuthorizedWorkspace,
        force_reload: bool,
    },
}

#[derive(Debug, Clone)]
pub enum ProfileMutation {
    SetExperimentalFeature { name: String, enabled: bool },
}

#[derive(Debug, Clone)]
pub enum ReviewTarget {
    UncommittedChanges,
    BaseBranch { branch: String },
    Commit { sha: String, title: Option<String> },
    Custom { instructions: String },
}

/// Abstract interface to the Codex runtime.
///
/// Platform server routes call through this trait instead of talking
/// directly to an app-server transport or the Codex process.
#[async_trait]
pub trait CodexAdapter: Send + Sync {
    /// Quick health probe.
    async fn health(&self) -> Result<HealthStatus, AdapterError>;

    /// Unique identity of the currently running app-server process. Runtime
    /// Server Request ids are valid only inside this instance.
    async fn runtime_instance_id(&self) -> Uuid;

    /// Start a Thread in a server-owned, authorization-checked workspace.
    async fn start_thread(
        &self,
        workspace: &AuthorizedWorkspace,
        copilot_package_id: Option<&str>,
    ) -> Result<StartedThread, AdapterError>;

    async fn fork_thread(
        &self,
        source_workspace: &AuthorizedWorkspace,
        target_workspace: &AuthorizedWorkspace,
        thread_id: &str,
    ) -> Result<StartedThread, AdapterError>;

    /// Read the authoritative persisted Thread from Codex.
    async fn read_thread(
        &self,
        workspace: &AuthorizedWorkspace,
        copilot_package_id: Option<&str>,
        thread_id: &str,
    ) -> Result<Value, AdapterError>;

    /// Read the actual Thread model settings through the official resume
    /// lifecycle without applying any configuration overrides.
    async fn read_thread_model_settings(
        &self,
        workspace: &AuthorizedWorkspace,
        thread_id: &str,
    ) -> Result<ThreadModelSettings, AdapterError>;

    /// Persist a model-only update for an authorized Thread. The caller must
    /// first establish that the requested Provider equals the Runtime-owned
    /// Provider, then read the settings again to confirm the update.
    async fn update_thread_model(
        &self,
        workspace: &AuthorizedWorkspace,
        thread_id: &str,
        model: &str,
    ) -> Result<(), AdapterError>;

    /// Read all persisted Turns with full items from Codex pagination.
    async fn list_thread_turns(
        &self,
        workspace: &AuthorizedWorkspace,
        copilot_package_id: Option<&str>,
        thread_id: &str,
    ) -> Result<Vec<Value>, AdapterError>;

    /// Read an MCP Resource through the official app-server API after the
    /// platform has authorized the owning Thread and hidden the source URI
    /// from the browser.
    async fn read_mcp_resource(
        &self,
        workspace: &AuthorizedWorkspace,
        copilot_package_id: Option<&str>,
        thread_id: &str,
        server: &str,
        uri: &str,
    ) -> Result<Value, AdapterError>;

    /// Start a user turn in the authorized workspace bound to the Thread.
    async fn send_user_message(
        &self,
        workspace: &AuthorizedWorkspace,
        thread_id: &str,
        text: &str,
        options: &TurnOptions,
    ) -> Result<Value, AdapterError>;

    /// Interrupt the active Turn identified by the durable platform
    /// projection. The caller must authorize both the Thread and workspace.
    async fn interrupt_turn(
        &self,
        workspace: &AuthorizedWorkspace,
        thread_id: &str,
        turn_id: &str,
    ) -> Result<(), AdapterError>;

    /// Steer the active Turn while preserving the Run and Thread.
    async fn steer_turn(
        &self,
        workspace: &AuthorizedWorkspace,
        thread_id: &str,
        turn_id: &str,
        text: &str,
        images: &[String],
    ) -> Result<Value, AdapterError>;

    /// Internal response path for app-server initiated requests. Public routes
    /// must resolve a platform-owned durable request before calling this.
    async fn respond_to_server_request(
        &self,
        runtime_instance_id: Uuid,
        request_id: Value,
        result: Value,
    ) -> Result<(), AdapterError>;

    /// Read a fixed, typed Profile capability. The enum owns the app-server
    /// method selection so browser input can never choose a raw method.
    async fn query_profile(&self, query: ProfileQuery) -> Result<Value, AdapterError>;

    /// Apply a fixed Profile mutation selected by the platform. Browser input
    /// never supplies an app-server method or raw configuration key path.
    async fn mutate_profile(&self, mutation: ProfileMutation) -> Result<Value, AdapterError>;

    /// Start the official ChatGPT browser login flow for this Profile.
    async fn start_profile_login(&self) -> Result<StartedProfileLogin, AdapterError>;

    /// Cancel the active browser login for this Profile, if any.
    async fn cancel_profile_login(&self) -> Result<CanceledProfileLogin, AdapterError>;

    /// Read the completion state captured from the official app-server event.
    async fn profile_login_status(
        &self,
        login_id: &str,
    ) -> Result<ProfileLoginStatus, AdapterError>;

    async fn archive_thread(
        &self,
        workspace: &AuthorizedWorkspace,
        thread_id: &str,
    ) -> Result<(), AdapterError>;

    async fn set_thread_name(
        &self,
        workspace: &AuthorizedWorkspace,
        thread_id: &str,
        name: &str,
    ) -> Result<(), AdapterError>;

    async fn compact_thread(
        &self,
        workspace: &AuthorizedWorkspace,
        thread_id: &str,
    ) -> Result<Value, AdapterError>;

    async fn start_review(
        &self,
        workspace: &AuthorizedWorkspace,
        thread_id: &str,
        target: ReviewTarget,
    ) -> Result<Value, AdapterError>;

    /// Subscribe to the internal app-server event stream. The implementor sends
    /// frames through `sender` and returns when the subscription ends.
    async fn subscribe_events(
        &self,
        sender: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    ) -> Result<(), AdapterError>;
}
