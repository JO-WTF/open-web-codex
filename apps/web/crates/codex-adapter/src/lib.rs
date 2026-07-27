pub mod fake;
pub mod real;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use thiserror::Error;
use toml_edit::DocumentMut;
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

/// Model-visible instructions applied when the Runtime creates a root Thread.
///
/// The platform resolves enterprise policy content before calling the
/// adapter. Browser payloads and mutable Profile configuration must never be
/// forwarded through this type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThreadStartMode {
    Standard,
    /// Immutable policy material required for a governed Supervisor start.
    ///
    /// Only the Platform server constructs this mode from a published Policy
    /// and Agent Definition set. The adapter verifies every referenced
    /// Profile Host file again immediately before each start or fork.
    GovernedSupervisor {
        developer_instructions: String,
        roles: Vec<PlatformRuntimeRole>,
        min_threads: u32,
        min_depth: u32,
    },
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
    pub model: Option<String>,
    pub model_provider: Option<String>,
    pub effort: Option<String>,
    pub service_tier: Option<String>,
    pub access_mode: Option<String>,
    pub images: Vec<String>,
    pub collaboration_mode: Option<Value>,
}

/// One platform-governed Runtime Role that may be projected into a Profile.
///
/// This is deliberately an internal, typed input. It is not a browser DTO and
/// it cannot be used to select an arbitrary Profile path: the adapter verifies
/// that every role maps to its immutable managed path before asking Profile
/// Host to write the role file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformRuntimeRole {
    pub definition_id: String,
    pub version: String,
    pub name: String,
    pub description: String,
    pub config_file: String,
    pub config_toml: String,
    pub content_sha256: String,
}

#[derive(Debug, Clone)]
pub enum ProfileQuery {
    Account,
    RateLimits,
    Usage,
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
    Config,
}

#[derive(Debug, Clone)]
pub enum ProfileMutation {
    SetExperimentalFeature {
        name: String,
        enabled: bool,
    },
    SetAgentCore {
        multi_agent_enabled: bool,
        max_threads: u32,
        max_depth: u32,
    },
    SetAgentDefinition {
        original_name: Option<String>,
        name: String,
        description: Option<String>,
        config_file: String,
    },
    RemoveAgentDefinition {
        name: String,
    },
    /// Materialize platform-authored Runtime Role files and make the exact
    /// roles discoverable by Codex. The platform, not a browser caller,
    /// constructs these static role definitions.
    EnsurePlatformRuntimeRoles {
        roles: Vec<PlatformRuntimeRole>,
        max_threads: u32,
        max_depth: u32,
    },
}

const MAX_PLATFORM_RUNTIME_ROLE_ID_BYTES: usize = 128;
const MAX_PLATFORM_RUNTIME_ROLE_NAME_BYTES: usize = 64;
const MAX_PLATFORM_RUNTIME_ROLE_DESCRIPTION_BYTES: usize = 512;
const MAX_PLATFORM_RUNTIME_ROLE_INSTRUCTIONS_BYTES: usize = 16 * 1024;
const MIN_PLATFORM_RUNTIME_MAX_THREADS: u32 = 2;
const MAX_PLATFORM_RUNTIME_MAX_THREADS: u32 = 12;
const MIN_PLATFORM_RUNTIME_MAX_DEPTH: u32 = 1;
const MAX_PLATFORM_RUNTIME_MAX_DEPTH: u32 = 4;

/// Validate the only Role documents the platform is allowed to project into a
/// Profile. Keeping this next to the public mutation type lets the real and
/// fake adapters enforce the same contract.
pub(crate) fn validate_platform_runtime_roles(
    roles: &[PlatformRuntimeRole],
    max_threads: u32,
    max_depth: u32,
) -> Result<(), AdapterError> {
    if roles.is_empty() {
        return Err(AdapterError::Internal(
            "Platform Runtime Role projection requires at least one role".to_string(),
        ));
    }
    if !(MIN_PLATFORM_RUNTIME_MAX_THREADS..=MAX_PLATFORM_RUNTIME_MAX_THREADS).contains(&max_threads)
        || !(MIN_PLATFORM_RUNTIME_MAX_DEPTH..=MAX_PLATFORM_RUNTIME_MAX_DEPTH).contains(&max_depth)
    {
        return Err(AdapterError::Internal(
            "Platform Runtime Role limits are outside supported bounds".to_string(),
        ));
    }

    let mut role_names = HashSet::new();
    let mut role_files = HashSet::new();
    for role in roles {
        validate_platform_runtime_role(role)?;
        if !role_names.insert(role.name.as_str()) || !role_files.insert(role.config_file.as_str()) {
            return Err(AdapterError::Internal(
                "Platform Runtime Role projection contains duplicate roles".to_string(),
            ));
        }
    }
    Ok(())
}

/// Resolve the batch-write limits against the existing effective Profile
/// config. A governed projection can raise the limits needed for collaboration,
/// but it must never lower a user/Profile value that is already higher.
pub(crate) fn resolve_platform_runtime_role_limits(
    profile_config: &Value,
    roles: &[PlatformRuntimeRole],
    max_threads: u32,
    max_depth: u32,
) -> Result<(u32, u32), AdapterError> {
    resolve_platform_runtime_role_limits_with_host_paths(
        profile_config,
        roles,
        max_threads,
        max_depth,
        &HashMap::new(),
    )
}

/// Same limit resolution with exact Profile Host paths for Roles that the
/// caller has already verified. This is the only way a pre-existing absolute
/// Runtime serialization is accepted; suffixes and unverified paths remain
/// conflicts.
pub(crate) fn resolve_platform_runtime_role_limits_with_host_paths(
    profile_config: &Value,
    roles: &[PlatformRuntimeRole],
    max_threads: u32,
    max_depth: u32,
    verified_host_paths: &HashMap<String, PathBuf>,
) -> Result<(u32, u32), AdapterError> {
    validate_platform_runtime_roles(roles, max_threads, max_depth)?;
    let config = profile_config
        .get("config")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            AdapterError::Internal(
                "Codex Profile configuration is unavailable for Runtime Role projection"
                    .to_string(),
            )
        })?;
    let agents = match config.get("agents") {
        None | Some(Value::Null) => None,
        Some(Value::Object(agents)) => Some(agents),
        Some(_) => {
            return Err(AdapterError::Internal(
                "Codex Runtime Role configuration is invalid".to_string(),
            ));
        }
    };

    if let Some(agents) = agents {
        for role in roles {
            if let Some(existing) = agents.get(&role.name) {
                let expected_host_path = verified_host_paths.get(&role.name).map(PathBuf::as_path);
                if !is_matching_platform_runtime_role_at_path(existing, role, expected_host_path) {
                    return Err(AdapterError::Internal(
                        "Platform Runtime Role conflicts with an existing Profile role".to_string(),
                    ));
                }
            }
        }
    }

    let current_threads = agents
        .and_then(|agents| agents.get("max_concurrent_threads_per_session"))
        .map(|value| positive_u32(value, "Codex Runtime Role concurrency setting is invalid"))
        .transpose()?
        .unwrap_or_default();
    let current_depth = agents
        .and_then(|agents| agents.get("max_depth"))
        .map(|value| positive_u32(value, "Codex Runtime Role depth setting is invalid"))
        .transpose()?
        .unwrap_or_default();

    Ok((
        max_threads.max(current_threads),
        max_depth.max(current_depth),
    ))
}

fn validate_platform_runtime_role(role: &PlatformRuntimeRole) -> Result<(), AdapterError> {
    if !is_safe_platform_path_segment(&role.definition_id) {
        return Err(AdapterError::Internal(
            "Platform Runtime Role definition id is invalid".to_string(),
        ));
    }
    if !is_safe_platform_path_segment(&role.version) {
        return Err(AdapterError::Internal(
            "Platform Runtime Role version is invalid".to_string(),
        ));
    }
    if !is_platform_runtime_role_name(&role.name) {
        return Err(AdapterError::Internal(
            "Platform Runtime Role name is invalid".to_string(),
        ));
    }
    if role.description.is_empty()
        || role.description != role.description.trim()
        || role.description.len() > MAX_PLATFORM_RUNTIME_ROLE_DESCRIPTION_BYTES
    {
        return Err(AdapterError::Internal(
            "Platform Runtime Role description is invalid".to_string(),
        ));
    }

    let expected_config_file =
        platform_runtime_role_config_file(&role.definition_id, &role.version);
    if role.config_file != expected_config_file {
        return Err(AdapterError::Internal(
            "Platform Runtime Role config file is not platform-managed".to_string(),
        ));
    }

    validate_platform_runtime_role_toml(&role.config_toml)?;
    let expected_digest = hex::encode(Sha256::digest(role.config_toml.as_bytes()));
    if role.content_sha256 != expected_digest {
        return Err(AdapterError::Internal(
            "Platform Runtime Role SHA-256 does not match configuration content".to_string(),
        ));
    }
    Ok(())
}

fn validate_platform_runtime_role_toml(contents: &str) -> Result<(), AdapterError> {
    if contents.is_empty() || contents.len() > MAX_PLATFORM_RUNTIME_ROLE_INSTRUCTIONS_BYTES {
        return Err(AdapterError::Internal(
            "Platform Runtime Role configuration is invalid".to_string(),
        ));
    }
    let document = contents.parse::<DocumentMut>().map_err(|_| {
        AdapterError::Internal("Platform Runtime Role configuration is invalid".to_string())
    })?;
    let table = document.as_table();
    if table.len() != 1 {
        return Err(AdapterError::Internal(
            "Platform Runtime Role configuration may only define developer instructions"
                .to_string(),
        ));
    }
    let Some(item) = table.get("developer_instructions") else {
        return Err(AdapterError::Internal(
            "Platform Runtime Role configuration may only define developer instructions"
                .to_string(),
        ));
    };
    let Some(instructions) = item.as_value().and_then(|value| value.as_str()) else {
        return Err(AdapterError::Internal(
            "Platform Runtime Role developer instructions are invalid".to_string(),
        ));
    };
    if instructions.trim().is_empty()
        || instructions.len() > MAX_PLATFORM_RUNTIME_ROLE_INSTRUCTIONS_BYTES
    {
        return Err(AdapterError::Internal(
            "Platform Runtime Role developer instructions are invalid".to_string(),
        ));
    }
    Ok(())
}

pub(crate) fn platform_runtime_role_config_file(definition_id: &str, version: &str) -> String {
    format!("platform-agents/{definition_id}/{version}.toml")
}

/// Match a Role only when its relative platform path is exact, or when the
/// Runtime reports the exact canonical absolute path that Profile Host just
/// verified. A suffix match would let an unrelated Profile file impersonate
/// a governed Role and is intentionally never accepted.
pub(crate) fn is_matching_platform_runtime_role_at_path(
    existing: &Value,
    role: &PlatformRuntimeRole,
    expected_host_path: Option<&Path>,
) -> bool {
    let Some(existing) = existing.as_object() else {
        return false;
    };
    if existing
        .keys()
        .any(|key| key != "description" && key != "config_file" && key != "nickname_candidates")
        || existing
            .get("nickname_candidates")
            .is_some_and(|value| !value.is_null())
        || existing.get("description").and_then(Value::as_str) != Some(role.description.as_str())
    {
        return false;
    }
    let Some(config_file) = existing.get("config_file").and_then(Value::as_str) else {
        return false;
    };
    config_file == role.config_file
        || expected_host_path.is_some_and(|path| {
            Path::new(config_file).is_absolute() && Path::new(config_file) == path
        })
}

/// Validate the project-aware effective Runtime configuration after Profile
/// Host has verified every managed Role file. This deliberately treats a
/// Project layer that attempts to set any protected key as a failure even if
/// a higher-precedence layer happens to mask it today.
pub(crate) fn validate_effective_platform_runtime_roles(
    config_read: &Value,
    roles: &[PlatformRuntimeRole],
    min_threads: u32,
    min_depth: u32,
    verified_host_paths: &HashMap<String, PathBuf>,
) -> Result<(), AdapterError> {
    validate_platform_runtime_roles(roles, min_threads, min_depth)?;
    reject_project_runtime_role_configuration(config_read, roles)?;

    let config = config_read
        .get("config")
        .and_then(Value::as_object)
        .ok_or_else(runtime_role_verification_error)?;
    if config
        .get("features")
        .and_then(Value::as_object)
        .and_then(|features| features.get("multi_agent"))
        .and_then(Value::as_bool)
        != Some(true)
    {
        return Err(runtime_role_verification_error());
    }
    let agents = config
        .get("agents")
        .and_then(Value::as_object)
        .ok_or_else(runtime_role_verification_error)?;
    if agents.get("enabled").and_then(Value::as_bool) != Some(true) {
        return Err(runtime_role_verification_error());
    }
    let threads = positive_u32(
        agents
            .get("max_concurrent_threads_per_session")
            .ok_or_else(runtime_role_verification_error)?,
        "Platform Runtime Role concurrency setting is invalid",
    )?;
    let depth = positive_u32(
        agents
            .get("max_depth")
            .ok_or_else(runtime_role_verification_error)?,
        "Platform Runtime Role depth setting is invalid",
    )?;
    if threads < min_threads || depth < min_depth {
        return Err(runtime_role_verification_error());
    }
    for role in roles {
        let configured = agents
            .get(&role.name)
            .ok_or_else(runtime_role_verification_error)?;
        let expected_host_path = verified_host_paths.get(&role.name).map(PathBuf::as_path);
        if !is_matching_platform_runtime_role_at_path(configured, role, expected_host_path) {
            return Err(runtime_role_verification_error());
        }
    }
    Ok(())
}

/// Build the only per-thread config overrides permitted for a governed
/// Supervisor. Every Role path comes from the just-verified Profile Host
/// file, never from a policy string, workspace config, or browser payload.
///
/// The Multi-Agent backend is intentionally not represented here. Governed
/// starts and forks select V1 through the typed Runtime `multiAgentBackend`
/// request field, which has higher precedence than model metadata and
/// inherited Thread history.
pub(crate) fn governed_runtime_role_config_overrides(
    roles: &[PlatformRuntimeRole],
    min_threads: u32,
    min_depth: u32,
    verified_host_paths: &HashMap<String, PathBuf>,
) -> Result<Value, AdapterError> {
    validate_platform_runtime_roles(roles, min_threads, min_depth)?;
    let mut overrides = serde_json::Map::new();
    overrides.insert("features.multi_agent".to_string(), Value::Bool(true));
    overrides.insert("agents.enabled".to_string(), Value::Bool(true));
    overrides.insert(
        "agents.max_concurrent_threads_per_session".to_string(),
        Value::from(min_threads),
    );
    overrides.insert("agents.max_depth".to_string(), Value::from(min_depth));
    for role in roles {
        let path = verified_host_paths
            .get(&role.name)
            .filter(|path| path.is_absolute())
            .and_then(|path| path.to_str())
            .ok_or_else(runtime_role_verification_error)?;
        overrides.insert(
            format!("agents.{}", role.name),
            serde_json::json!({
                "description": role.description,
                "config_file": path,
                // Request config is recursively merged with Project config.
                // Seal every accepted AgentRoleToml field so a Project layer
                // cannot contribute nickname candidates after verification.
                "nickname_candidates": [role.name.clone()],
            }),
        );
    }
    Ok(Value::Object(overrides))
}

fn reject_project_runtime_role_configuration(
    config_read: &Value,
    roles: &[PlatformRuntimeRole],
) -> Result<(), AdapterError> {
    let origins = config_read
        .get("origins")
        .and_then(Value::as_object)
        .ok_or_else(runtime_role_verification_error)?;
    for (key, origin) in origins {
        if is_protected_runtime_role_key(key, roles)
            && config_layer_source_type(origin)? == "project"
        {
            return Err(runtime_role_verification_error());
        }
    }

    let layers = config_read
        .get("layers")
        .and_then(Value::as_array)
        .ok_or_else(runtime_role_verification_error)?;
    for layer in layers {
        if config_layer_source_type(layer)? == "project" {
            let config = layer
                .get("config")
                .ok_or_else(runtime_role_verification_error)?;
            if project_layer_contains_protected_runtime_role_key(config, roles)? {
                return Err(runtime_role_verification_error());
            }
        }
    }
    Ok(())
}

fn config_layer_source_type(value: &Value) -> Result<&str, AdapterError> {
    value
        .pointer("/name/type")
        .and_then(Value::as_str)
        .ok_or_else(runtime_role_verification_error)
}

fn is_protected_runtime_role_key(key: &str, roles: &[PlatformRuntimeRole]) -> bool {
    matches!(
        key,
        "features.multi_agent"
            | "features.collab"
            | "agents.enabled"
            | "agents.max_concurrent_threads_per_session"
            | "agents.max_threads"
            | "agents.max_depth"
    ) || key == "features.multi_agent_v2"
        || key.starts_with("features.multi_agent_v2.")
        || roles.iter().any(|role| {
            let role_key = format!("agents.{}", role.name);
            key == role_key || key.starts_with(&format!("{role_key}."))
        })
}

fn project_layer_contains_protected_runtime_role_key(
    value: &Value,
    roles: &[PlatformRuntimeRole],
) -> Result<bool, AdapterError> {
    let object = value
        .as_object()
        .ok_or_else(runtime_role_verification_error)?;
    if object
        .keys()
        .any(|key| is_protected_runtime_role_key(key, roles))
    {
        return Ok(true);
    }
    if let Some(features) = object.get("features") {
        let features = features
            .as_object()
            .ok_or_else(runtime_role_verification_error)?;
        if features.contains_key("multi_agent")
            || features.contains_key("collab")
            || features.contains_key("multi_agent_v2")
        {
            return Ok(true);
        }
    }
    if let Some(agents) = object.get("agents") {
        let agents = agents
            .as_object()
            .ok_or_else(runtime_role_verification_error)?;
        if agents.contains_key("enabled")
            || agents.contains_key("max_concurrent_threads_per_session")
            || agents.contains_key("max_depth")
            || agents.contains_key("max_threads")
            || roles.iter().any(|role| agents.contains_key(&role.name))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn runtime_role_verification_error() -> AdapterError {
    AdapterError::Internal("Platform Runtime Role verification failed".to_string())
}

fn positive_u32(value: &Value, error: &'static str) -> Result<u32, AdapterError> {
    value
        .as_u64()
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| *value > 0)
        .ok_or_else(|| AdapterError::Internal(error.to_string()))
}

fn is_safe_platform_path_segment(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_PLATFORM_RUNTIME_ROLE_ID_BYTES
        && value != "."
        && value != ".."
        && !value.contains("..")
        && !value.starts_with('.')
        && !value.ends_with('.')
        && value
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        && value
            .as_bytes()
            .last()
            .is_some_and(u8::is_ascii_alphanumeric)
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_' | b'.')
        })
}

fn is_platform_runtime_role_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_PLATFORM_RUNTIME_ROLE_NAME_BYTES
        && !matches!(
            value,
            "default"
                | "enabled"
                | "max_concurrent_threads_per_session"
                | "max_threads"
                | "max_depth"
                | "default_subagent_model"
                | "default_subagent_reasoning_effort"
                | "interrupt_message"
                | "job_max_runtime_seconds"
        )
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

#[derive(Debug, Clone)]
pub enum ReviewTarget {
    UncommittedChanges,
    BaseBranch { branch: String },
    Commit { sha: String, title: Option<String> },
    Custom { instructions: String },
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

    /// Unique identity of the currently running app-server process. Runtime
    /// Server Request ids are valid only inside this instance.
    async fn runtime_instance_id(&self) -> Uuid;

    /// Transitional internal RPC call. Product routes should replace this
    /// with typed Thread/Turn operations; it is never a public browser
    /// contract.
    async fn rpc(&self, method: &str, params: Value) -> Result<Value, AdapterError>;

    /// Start a Thread in a server-owned, authorization-checked workspace.
    async fn start_thread(
        &self,
        workspace: &AuthorizedWorkspace,
        mode: &ThreadStartMode,
    ) -> Result<StartedThread, AdapterError>;

    async fn fork_thread(
        &self,
        source_workspace: &AuthorizedWorkspace,
        target_workspace: &AuthorizedWorkspace,
        thread_id: &str,
        mode: &ThreadStartMode,
    ) -> Result<StartedThread, AdapterError>;

    /// Read the authoritative persisted Thread from Codex.
    async fn read_thread(
        &self,
        workspace: &AuthorizedWorkspace,
        thread_id: &str,
    ) -> Result<Value, AdapterError>;

    /// Read all persisted Turns with full items from Codex pagination.
    async fn list_thread_turns(
        &self,
        workspace: &AuthorizedWorkspace,
        thread_id: &str,
    ) -> Result<Vec<Value>, AdapterError>;

    /// Read an MCP Resource through the official app-server API after the
    /// platform has authorized the owning Thread and hidden the source URI
    /// from the browser.
    async fn read_mcp_resource(
        &self,
        workspace: &AuthorizedWorkspace,
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

    /// Re-verify platform-authored Runtime Role files and their effective
    /// configuration for an authorized workspace. This is a server-internal
    /// guard: it reads the official project-aware config view and never
    /// exposes Runtime paths or raw configuration to the browser.
    async fn verify_platform_runtime_roles(
        &self,
        workspace: &AuthorizedWorkspace,
        roles: &[PlatformRuntimeRole],
        min_threads: u32,
        min_depth: u32,
    ) -> Result<(), AdapterError>;

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

    /// Run a short, platform-authored background prompt in an authorized
    /// workspace and return only the final assistant text.
    async fn generate_text(
        &self,
        workspace: &AuthorizedWorkspace,
        prompt: &str,
        model: Option<&str>,
    ) -> Result<String, AdapterError>;

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

    async fn open_terminal(
        &self,
        workspace: &AuthorizedWorkspace,
        process_id: &str,
        cols: u16,
        rows: u16,
    ) -> Result<(), AdapterError>;

    async fn write_terminal(
        &self,
        workspace: &AuthorizedWorkspace,
        process_id: &str,
        data: &str,
    ) -> Result<(), AdapterError>;

    async fn resize_terminal(
        &self,
        workspace: &AuthorizedWorkspace,
        process_id: &str,
        cols: u16,
        rows: u16,
    ) -> Result<(), AdapterError>;

    async fn close_terminal(
        &self,
        workspace: &AuthorizedWorkspace,
        process_id: &str,
    ) -> Result<(), AdapterError>;

    /// Subscribe to the internal app-server event stream. The implementor sends
    /// frames through `sender` and returns when the subscription ends.
    async fn subscribe_events(
        &self,
        sender: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    ) -> Result<(), AdapterError>;
}
