pub mod fake;
pub mod real;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
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
    /// One exact published Agent executes as the root Thread. The platform
    /// supplies only its sealed instructions and typed MCP requirements.
    GovernedAgent {
        developer_instructions: String,
        required_mcp_servers: Vec<RequiredMcpServer>,
    },
    /// Immutable policy material required for a governed Supervisor start.
    ///
    /// Only the Platform server constructs this mode from a published Policy
    /// and Agent Definition set. The adapter verifies every referenced
    /// Profile Host file again immediately before each start or fork.
    GovernedSupervisor {
        developer_instructions: String,
        roles: Vec<PlatformRuntimeRole>,
        role_spawn_limits: BTreeMap<String, u32>,
        required_mcp_servers: Vec<RequiredMcpServer>,
        coordination_mcp_servers: Vec<RequiredMcpServer>,
        max_threads: u32,
    },
}

pub(crate) fn governed_agent_config_overrides(
    required_mcp_servers: &[RequiredMcpServer],
) -> Result<Value, AdapterError> {
    validate_required_mcp_servers(required_mcp_servers)?;
    let capability_roots = merge_capability_root_mcp_inventories(required_mcp_servers)?;
    let mut overrides = serde_json::Map::new();
    overrides.insert("features.apps".to_string(), Value::Bool(false));
    overrides.insert("features.multi_agent_v2".to_string(), Value::Bool(false));
    overrides.insert("features.plugins".to_string(), Value::Bool(false));
    overrides.insert("features.shell_tool".to_string(), Value::Bool(false));
    overrides.insert(
        "skills.include_instructions".to_string(),
        Value::Bool(false),
    );
    for (capability_root_id, server_names) in &capability_roots {
        overrides.insert(
            format!("plugins.{capability_root_id}.enabled"),
            Value::Bool(true),
        );
        for server_name in server_names {
            overrides.insert(
                format!("plugins.{capability_root_id}.mcp_servers.{server_name}.enabled"),
                Value::Bool(false),
            );
        }
    }
    for server in required_mcp_servers {
        for capability_root in &server.capability_roots {
            overrides.insert(
                format!(
                    "plugins.{}.mcp_servers.{}.enabled",
                    capability_root.capability_root_id, server.name
                ),
                Value::Bool(true),
            );
            overrides.insert(
                format!(
                    "plugins.{}.mcp_servers.{}.enabled_tools",
                    capability_root.capability_root_id, server.name
                ),
                Value::Array(server.tools.iter().cloned().map(Value::String).collect()),
            );
        }
    }
    Ok(Value::Object(overrides))
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

/// Complete MCP Server inventory for one selected Runtime capability root.
///
/// The platform derives this from the reviewed capability package declaration.
/// It is carried with the allowlist so Runtime configuration can explicitly
/// disable sibling Servers that Codex would otherwise enable by default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityRootMcpInventory {
    pub capability_root_id: String,
    pub mcp_server_names: Vec<String>,
}

/// Exact MCP inventory required by an immutable governed policy.
///
/// These identifiers are derived from code-published Agent Definitions. They
/// are never accepted from the browser or inferred from tool display text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequiredMcpServer {
    pub name: String,
    pub tools: Vec<String>,
    pub capability_roots: Vec<CapabilityRootMcpInventory>,
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
    /// Materialize immutable platform-authored Runtime Role files.
    ///
    /// These files are referenced only by governed Thread request config; this
    /// mutation must not register the roles in persistent Profile config.
    MaterializePlatformRuntimeRoleFiles {
        roles: Vec<PlatformRuntimeRole>,
    },
}

const MAX_PLATFORM_RUNTIME_ROLE_ID_BYTES: usize = 128;
const MAX_PLATFORM_RUNTIME_ROLE_NAME_BYTES: usize = 64;
const MAX_PLATFORM_RUNTIME_ROLE_DESCRIPTION_BYTES: usize = 512;
const MAX_PLATFORM_RUNTIME_ROLE_INSTRUCTIONS_BYTES: usize = 16 * 1024;
const MAX_PLATFORM_RUNTIME_ROLE_CONFIG_BYTES: usize = 32 * 1024;
const MAX_REQUIRED_MCP_SERVER_NAME_BYTES: usize = 128;
const MAX_REQUIRED_MCP_TOOL_NAME_BYTES: usize = 128;
const MAX_REQUIRED_MCP_SERVERS: usize = 16;
const MAX_REQUIRED_MCP_TOOLS_PER_SERVER: usize = 64;
const MAX_REQUIRED_MCP_CAPABILITY_ROOTS_PER_SERVER: usize = 16;
const MIN_PLATFORM_RUNTIME_MAX_THREADS: u32 = 1;
const MAX_PLATFORM_RUNTIME_MAX_THREADS: u32 = 12;

/// Validate the only Role documents the platform is allowed to project into a
/// Profile. Keeping this next to the public mutation type lets the real and
/// fake adapters enforce the same contract.
pub(crate) fn validate_platform_runtime_roles(
    roles: &[PlatformRuntimeRole],
    max_threads: u32,
) -> Result<(), AdapterError> {
    validate_platform_runtime_role_files(roles)?;
    if !(MIN_PLATFORM_RUNTIME_MAX_THREADS..=MAX_PLATFORM_RUNTIME_MAX_THREADS).contains(&max_threads)
    {
        return Err(AdapterError::Internal(
            "Platform Runtime Role limits are outside supported bounds".to_string(),
        ));
    }
    Ok(())
}

/// Validate the exact MCP inventory sealed in a platform-owned capability
/// package or governed Release.
///
/// This validates only the typed declaration. It deliberately does not inspect
/// workspace files or discover Runtime state. The real adapter performs the
/// authoritative process and Tool inventory check after the Thread starts.
pub fn validate_required_mcp_servers(servers: &[RequiredMcpServer]) -> Result<(), AdapterError> {
    if servers.is_empty() || servers.len() > MAX_REQUIRED_MCP_SERVERS {
        return Err(AdapterError::Internal(
            "governed Runtime MCP requirements are invalid".to_string(),
        ));
    }
    let mut server_names = HashSet::new();
    for server in servers {
        if !is_safe_runtime_capability_name(&server.name, MAX_REQUIRED_MCP_SERVER_NAME_BYTES)
            || server.tools.len() > MAX_REQUIRED_MCP_TOOLS_PER_SERVER
            || server.capability_roots.is_empty()
            || server.capability_roots.len() > MAX_REQUIRED_MCP_CAPABILITY_ROOTS_PER_SERVER
            || !server_names.insert(server.name.as_str())
        {
            return Err(AdapterError::Internal(
                "governed Runtime MCP requirements are invalid".to_string(),
            ));
        }
        let mut tool_names = HashSet::new();
        if server.tools.iter().any(|tool| {
            !is_safe_runtime_capability_name(tool, MAX_REQUIRED_MCP_TOOL_NAME_BYTES)
                || !tool_names.insert(tool.as_str())
        }) {
            return Err(AdapterError::Internal(
                "governed Runtime MCP requirements are invalid".to_string(),
            ));
        }
    }
    merge_capability_root_mcp_inventories(servers)?;
    Ok(())
}

pub(crate) fn merge_capability_root_mcp_inventories(
    servers: &[RequiredMcpServer],
) -> Result<BTreeMap<String, Vec<String>>, AdapterError> {
    let mut inventories = BTreeMap::<String, Vec<String>>::new();
    for required in servers {
        let mut root_ids = HashSet::new();
        for root in &required.capability_roots {
            let server_names = root
                .mcp_server_names
                .iter()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>();
            if !is_safe_runtime_capability_name(
                &root.capability_root_id,
                MAX_REQUIRED_MCP_SERVER_NAME_BYTES,
            ) || !root_ids.insert(root.capability_root_id.as_str())
                || server_names.is_empty()
                || server_names.len() > MAX_REQUIRED_MCP_SERVERS
                || server_names.len() != root.mcp_server_names.len()
                || server_names.iter().any(|server_name| {
                    !is_safe_runtime_capability_name(
                        server_name,
                        MAX_REQUIRED_MCP_SERVER_NAME_BYTES,
                    )
                })
                || !server_names.contains(&required.name)
            {
                return Err(AdapterError::Internal(
                    "governed Runtime MCP capability root inventory is invalid".to_string(),
                ));
            }
            let server_names = server_names.into_iter().collect::<Vec<_>>();
            if inventories
                .insert(root.capability_root_id.clone(), server_names.clone())
                .is_some_and(|existing| existing != server_names)
            {
                return Err(AdapterError::Internal(
                    "governed Runtime MCP capability root inventory is inconsistent".to_string(),
                ));
            }
        }
    }
    Ok(inventories)
}

pub(crate) fn validate_role_spawn_limits(
    roles: &[PlatformRuntimeRole],
    limits: &BTreeMap<String, u32>,
) -> Result<(), AdapterError> {
    if limits.is_empty()
        || limits.len() > roles.len()
        || limits.iter().any(|(role, limit)| {
            *limit == 0
                || *limit > MAX_PLATFORM_RUNTIME_MAX_THREADS
                || !roles.iter().any(|candidate| candidate.name == *role)
        })
    {
        return Err(AdapterError::Internal(
            "governed Runtime Role instance limits are invalid".to_string(),
        ));
    }
    Ok(())
}

pub(crate) fn validate_platform_runtime_role_files(
    roles: &[PlatformRuntimeRole],
) -> Result<(), AdapterError> {
    if roles.is_empty() {
        return Err(AdapterError::Internal(
            "governed Runtime Role set requires at least one role".to_string(),
        ));
    }

    let mut role_names = HashSet::new();
    let mut role_files = HashSet::new();
    for role in roles {
        validate_platform_runtime_role(role)?;
        if !role_names.insert(role.name.as_str()) || !role_files.insert(role.config_file.as_str()) {
            return Err(AdapterError::Internal(
                "governed Runtime Role set contains duplicate roles".to_string(),
            ));
        }
    }
    Ok(())
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
    if contents.is_empty() || contents.len() > MAX_PLATFORM_RUNTIME_ROLE_CONFIG_BYTES {
        return Err(AdapterError::Internal(
            "Platform Runtime Role configuration is invalid".to_string(),
        ));
    }
    let document = contents.parse::<DocumentMut>().map_err(|_| {
        AdapterError::Internal("Platform Runtime Role configuration is invalid".to_string())
    })?;
    let table = document.as_table();
    if table.len() != 5
        || [
            "developer_instructions",
            "agents",
            "features",
            "skills",
            "plugins",
        ]
        .iter()
        .any(|field| !table.contains_key(field))
    {
        return Err(AdapterError::Internal(
            "Platform Runtime Role configuration contains unsupported fields".to_string(),
        ));
    }
    let Some(item) = table.get("developer_instructions") else {
        return Err(AdapterError::Internal(
            "Platform Runtime Role configuration must define developer instructions".to_string(),
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
    let agents = table
        .get("agents")
        .and_then(|item| item.as_table())
        .filter(|agents| agents.len() == 1)
        .ok_or_else(|| {
            AdapterError::Internal(
                "Platform Runtime Role Agent restrictions are invalid".to_string(),
            )
        })?;
    if agents
        .get("enabled")
        .and_then(|item| item.as_value())
        .and_then(|value| value.as_bool())
        != Some(false)
    {
        return Err(AdapterError::Internal(
            "Platform Runtime Role Agent restrictions are invalid".to_string(),
        ));
    }

    let skills = table
        .get("skills")
        .and_then(|item| item.as_table())
        .filter(|skills| skills.len() == 1)
        .ok_or_else(|| {
            AdapterError::Internal(
                "Platform Runtime Role Skill restrictions are invalid".to_string(),
            )
        })?;
    if skills
        .get("include_instructions")
        .and_then(|item| item.as_value())
        .and_then(|value| value.as_bool())
        != Some(false)
    {
        return Err(AdapterError::Internal(
            "Platform Runtime Role Skill restrictions are invalid".to_string(),
        ));
    }

    let features = table
        .get("features")
        .and_then(|item| item.as_table())
        .ok_or_else(|| {
            AdapterError::Internal(
                "Platform Runtime Role feature restrictions are invalid".to_string(),
            )
        })?;
    if features.len() != 4
        || ["apps", "multi_agent_v2", "plugins", "shell_tool"]
            .iter()
            .any(|feature| {
                features
                    .get(feature)
                    .and_then(|item| item.as_value())
                    .and_then(|value| value.as_bool())
                    != Some(false)
            })
    {
        return Err(AdapterError::Internal(
            "Platform Runtime Role feature restrictions are invalid".to_string(),
        ));
    }

    let plugins = table
        .get("plugins")
        .and_then(|item| item.as_table())
        .filter(|plugins| {
            !plugins.is_empty() && plugins.len() <= MAX_REQUIRED_MCP_CAPABILITY_ROOTS_PER_SERVER
        })
        .ok_or_else(|| {
            AdapterError::Internal("Platform Runtime Role MCP restrictions are invalid".to_string())
        })?;
    let mut enabled_server_count = 0_usize;
    for (root_id, item) in plugins {
        if !is_safe_runtime_capability_name(root_id, MAX_REQUIRED_MCP_SERVER_NAME_BYTES) {
            return Err(AdapterError::Internal(
                "Platform Runtime Role MCP restrictions are invalid".to_string(),
            ));
        }
        let plugin = item
            .as_table()
            .filter(|server| server.len() == 2)
            .ok_or_else(|| {
                AdapterError::Internal(
                    "Platform Runtime Role MCP restrictions are invalid".to_string(),
                )
            })?;
        if plugin
            .get("enabled")
            .and_then(|item| item.as_value())
            .and_then(|value| value.as_bool())
            != Some(true)
        {
            return Err(AdapterError::Internal(
                "Platform Runtime Role MCP server must be enabled".to_string(),
            ));
        }
        let mcp_servers = plugin
            .get("mcp_servers")
            .and_then(|item| item.as_table())
            .filter(|servers| !servers.is_empty() && servers.len() <= MAX_REQUIRED_MCP_SERVERS)
            .ok_or_else(|| {
                AdapterError::Internal(
                    "Platform Runtime Role MCP restrictions are invalid".to_string(),
                )
            })?;
        for (server_name, item) in mcp_servers {
            if !is_safe_runtime_capability_name(server_name, MAX_REQUIRED_MCP_SERVER_NAME_BYTES) {
                return Err(AdapterError::Internal(
                    "Platform Runtime Role MCP restrictions are invalid".to_string(),
                ));
            }
            let server = item.as_table().ok_or_else(|| {
                AdapterError::Internal(
                    "Platform Runtime Role MCP restrictions are invalid".to_string(),
                )
            })?;
            let enabled = server
                .get("enabled")
                .and_then(|item| item.as_value())
                .and_then(|value| value.as_bool())
                .ok_or_else(|| {
                    AdapterError::Internal(
                        "Platform Runtime Role MCP restrictions are invalid".to_string(),
                    )
                })?;
            if !enabled {
                if server.len() != 1 {
                    return Err(AdapterError::Internal(
                        "disabled Platform Runtime Role MCP Server has unsupported fields"
                            .to_string(),
                    ));
                }
                continue;
            }
            enabled_server_count += 1;
            if server.len() != 2 {
                return Err(AdapterError::Internal(
                    "enabled Platform Runtime Role MCP Server restrictions are invalid".to_string(),
                ));
            }
            let enabled_tools = server
                .get("enabled_tools")
                .and_then(|item| item.as_value())
                .and_then(|value| value.as_array())
                .filter(|tools| tools.len() <= MAX_REQUIRED_MCP_TOOLS_PER_SERVER)
                .ok_or_else(|| {
                    AdapterError::Internal(
                        "Platform Runtime Role MCP tool restrictions are invalid".to_string(),
                    )
                })?;
            let mut tool_names = HashSet::new();
            for tool in enabled_tools {
                let tool = tool.as_str().filter(|tool| {
                    is_safe_runtime_capability_name(tool, MAX_REQUIRED_MCP_TOOL_NAME_BYTES)
                });
                if tool.is_none() || !tool_names.insert(tool.unwrap()) {
                    return Err(AdapterError::Internal(
                        "Platform Runtime Role MCP tool restrictions are invalid".to_string(),
                    ));
                }
            }
        }
    }
    if enabled_server_count == 0 {
        return Err(AdapterError::Internal(
            "Platform Runtime Role must enable an MCP Server".to_string(),
        ));
    }
    Ok(())
}

pub(crate) fn platform_runtime_role_config_file(definition_id: &str, version: &str) -> String {
    format!("platform-agents/{definition_id}/{version}.toml")
}

/// Build the only per-thread config overrides permitted for a governed
/// Supervisor. Every Role path comes from the just-verified Profile Host
/// file, never from a policy string, workspace config, or browser payload.
///
/// Governed starts and forks explicitly select the current V2 collaboration
/// engine through a request-scoped config override. This does not mutate
/// Profile or Project configuration.
pub(crate) fn governed_runtime_role_config_overrides(
    roles: &[PlatformRuntimeRole],
    role_spawn_limits: &BTreeMap<String, u32>,
    required_mcp_servers: &[RequiredMcpServer],
    coordination_mcp_servers: &[RequiredMcpServer],
    max_threads: u32,
    verified_host_paths: &HashMap<String, PathBuf>,
) -> Result<Value, AdapterError> {
    validate_platform_runtime_roles(roles, max_threads)?;
    validate_role_spawn_limits(roles, role_spawn_limits)?;
    validate_required_mcp_servers(required_mcp_servers)?;
    if !coordination_mcp_servers.is_empty() {
        validate_required_mcp_servers(coordination_mcp_servers)?;
    }
    // Root coordination is platform-owned and may be installed separately
    // from the domain Agent packages.  Merge both inventories for config
    // isolation, but do not require the root read-only capability to be part
    // of a child Agent's tool inventory.
    let mut all_servers = required_mcp_servers.to_vec();
    all_servers.extend_from_slice(coordination_mcp_servers);
    let capability_roots = merge_capability_root_mcp_inventories(&all_servers)?;
    let mut overrides = serde_json::Map::new();
    overrides.insert("features.apps".to_string(), Value::Bool(false));
    overrides.insert("features.multi_agent_v2".to_string(), Value::Bool(true));
    // The official input tool is root-thread owned. Governed Supervisors use
    // the Runtime's default-mode feature so the Platform can persist and
    // project the same request without inventing a parallel input protocol.
    overrides.insert(
        "features.default_mode_request_user_input".to_string(),
        Value::Bool(true),
    );
    overrides.insert("features.plugins".to_string(), Value::Bool(false));
    overrides.insert("features.shell_tool".to_string(), Value::Bool(false));
    overrides.insert(
        "skills.include_instructions".to_string(),
        Value::Bool(false),
    );
    overrides.insert(
        "agents.allowed_roles".to_string(),
        Value::Array(
            roles
                .iter()
                .map(|role| Value::String(role.name.clone()))
                .collect(),
        ),
    );
    for (role, limit) in role_spawn_limits {
        overrides.insert(
            format!("agents.role_spawn_limits.{role}"),
            Value::from(*limit),
        );
    }
    overrides.insert(
        "agents.max_concurrent_threads_per_session".to_string(),
        Value::from(max_threads),
    );
    for (capability_root_id, server_names) in capability_roots {
        overrides.insert(
            format!("plugins.{capability_root_id}.enabled"),
            Value::Bool(true),
        );
        for server_name in server_names {
            overrides.insert(
                format!("plugins.{capability_root_id}.mcp_servers.{server_name}.enabled"),
                Value::Bool(false),
            );
        }
    }
    for server in coordination_mcp_servers {
        for capability_root in &server.capability_roots {
            overrides.insert(
                format!(
                    "plugins.{}.mcp_servers.{}.enabled",
                    capability_root.capability_root_id, server.name
                ),
                Value::Bool(true),
            );
            overrides.insert(
                format!(
                    "plugins.{}.mcp_servers.{}.enabled_tools",
                    capability_root.capability_root_id, server.name
                ),
                Value::Array(server.tools.iter().cloned().map(Value::String).collect()),
            );
        }
    }
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

fn runtime_role_verification_error() -> AdapterError {
    AdapterError::Internal("Platform Runtime Role verification failed".to_string())
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
                | "allowed_roles"
                | "enabled"
                | "max_concurrent_threads_per_session"
                | "max_threads"
                | "role_spawn_limits"
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

fn is_safe_runtime_capability_name(value: &str, maximum_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum_bytes
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
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
