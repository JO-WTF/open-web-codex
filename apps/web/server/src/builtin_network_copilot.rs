use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use open_web_codex_adapter::real::ThreadSkillConfig;
use open_web_codex_profile_host::{CodexFeature, ProfileStartupFile};
use serde::Deserialize;
use thiserror::Error;
use toml_edit::{value, Array, DocumentMut, Item, Table};

const SUPERVISOR_SKILL: &str =
    include_str!("../../builtin/warehouse-network-copilot/skills/warehouse-supervisor/SKILL.md");
const DATA_SKILL: &str =
    include_str!("../../builtin/warehouse-network-copilot/skills/warehouse-data/SKILL.md");
const NETWORK_SKILL: &str =
    include_str!("../../builtin/warehouse-network-copilot/skills/warehouse-network/SKILL.md");
const DATA_ROLE: &str =
    include_str!("../../builtin/warehouse-network-copilot/agents/data_agent.toml");
const NETWORK_ROLE: &str =
    include_str!("../../builtin/warehouse-network-copilot/agents/network_agent.toml");
const ROLE_MCP_SERVER_POLICY_KEYS: [&str; 5] = [
    "enabled",
    "default_tools_approval_mode",
    "enabled_tools",
    "disabled_tools",
    "tools",
];
const ALLOWED_HOST_ENVIRONMENT_NAMES: [&str; 8] = [
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "NO_PROXY",
    "http_proxy",
    "https_proxy",
    "all_proxy",
    "no_proxy",
];

/// Keep the phase-one warehouse acceptance Profile focused on its native
/// Skills, Roles, and role-local MCP tools. Codex still owns discovery and
/// prompt construction; these are official feature switches, not a local
/// tool/plugin filter.
pub(crate) const fn disabled_codex_features() -> [CodexFeature; 4] {
    [
        CodexFeature::Plugins,
        CodexFeature::RemotePlugin,
        CodexFeature::Apps,
        CodexFeature::ToolSuggest,
    ]
}

/// Use Codex's native structured user-input lifecycle while the warehouse
/// supervisor is executing in Default mode. Platform and Web only persist,
/// authorize, and render the official request; they do not infer questions
/// from assistant text.
pub(crate) const fn enabled_codex_features() -> [CodexFeature; 1] {
    [CodexFeature::DefaultModeRequestUserInput]
}

pub(crate) fn root_skill_config() -> Vec<ThreadSkillConfig> {
    vec![
        ThreadSkillConfig {
            name: "warehouse-supervisor".to_string(),
            enabled: true,
        },
        ThreadSkillConfig {
            name: "warehouse-data".to_string(),
            enabled: false,
        },
        ThreadSkillConfig {
            name: "warehouse-network".to_string(),
            enabled: false,
        },
    ]
}

#[derive(Debug)]
pub(crate) struct BuiltinNetworkCopilotAssets {
    capability_roots: BTreeMap<String, PreparedCapabilityRoot>,
}

#[derive(Debug, Clone)]
struct McpTransport {
    command: PathBuf,
    args: Vec<String>,
    startup_timeout_sec: Option<i64>,
    tool_timeout_sec: Option<i64>,
    env_bindings: Vec<PreparedEnvironmentBinding>,
}

#[derive(Debug, Clone)]
struct PreparedCapabilityRoot {
    servers: BTreeMap<String, McpTransport>,
}

#[derive(Debug, Clone)]
enum PreparedEnvironmentBinding {
    ProfileHome { name: String },
    ToolStateRoot { name: String },
    DependencyRoot { name: String, root: PathBuf },
    Host { name: String },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PreparedDescriptorDocument {
    schema_version: u32,
    composition_descriptor_sha256: String,
    capability_roots: Vec<PreparedDescriptorCapabilityRoot>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PreparedDescriptorCapabilityRoot {
    id: String,
    servers: Vec<PreparedDescriptorServer>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PreparedDescriptorServer {
    id: String,
    transport: String,
    command: PathBuf,
    args: Vec<String>,
    env_bindings: Vec<PreparedDescriptorEnvironmentBinding>,
    startup_timeout_sec: Option<i64>,
    tool_timeout_sec: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PreparedDescriptorEnvironmentBinding {
    name: String,
    source: String,
    dependency: Option<String>,
    resolved_root: Option<PathBuf>,
}

impl BuiltinNetworkCopilotAssets {
    pub(crate) fn resolve(descriptor_path: &Path) -> Result<Self, BuiltinNetworkCopilotError> {
        let capability_roots = load_prepared_descriptor(descriptor_path)?;
        Ok(Self { capability_roots })
    }

    pub(crate) fn startup_files(
        &self,
        profile_home: &Path,
    ) -> Result<Vec<ProfileStartupFile>, BuiltinNetworkCopilotError> {
        let profile_home = require_absolute_path("Profile CODEX_HOME", profile_home)?;
        let data_role = self.render_data_role(&profile_home)?;
        let network_role = self.render_network_role(&profile_home)?;

        Ok(vec![
            ProfileStartupFile::managed_skill("warehouse-supervisor", SUPERVISOR_SKILL.as_bytes())?,
            ProfileStartupFile::managed_skill("warehouse-data", DATA_SKILL.as_bytes())?,
            ProfileStartupFile::managed_skill("warehouse-network", NETWORK_SKILL.as_bytes())?,
            ProfileStartupFile::managed_agent_role("data_agent", data_role.into_bytes())?,
            ProfileStartupFile::managed_agent_role("network_agent", network_role.into_bytes())?,
        ])
    }

    fn render_data_role(&self, profile_home: &Path) -> Result<String, BuiltinNetworkCopilotError> {
        let mut role = parse_role_template("data_agent", DATA_ROLE)?;
        project_role_mcp_servers(
            &mut role,
            "data_agent",
            &self.capability_roots,
            profile_home,
        )?;
        finish_role_template("data_agent", role)
    }

    fn render_network_role(
        &self,
        profile_home: &Path,
    ) -> Result<String, BuiltinNetworkCopilotError> {
        let mut role = parse_role_template("network_agent", NETWORK_ROLE)?;
        project_role_mcp_servers(
            &mut role,
            "network_agent",
            &self.capability_roots,
            profile_home,
        )?;
        finish_role_template("network_agent", role)
    }
}

#[derive(Debug, Error)]
pub(crate) enum BuiltinNetworkCopilotError {
    #[error("built-in warehouse-network capability is unavailable: {component}: {message}")]
    Unavailable {
        component: &'static str,
        message: String,
    },
    #[error("invalid built-in warehouse-network {role} Role template: {message}")]
    InvalidRoleTemplate { role: &'static str, message: String },
    #[error(transparent)]
    StartupFile(#[from] open_web_codex_profile_host::ProfileStartupFileError),
}

fn require_absolute_path(
    component: &'static str,
    path: &Path,
) -> Result<PathBuf, BuiltinNetworkCopilotError> {
    if !path.is_absolute() {
        return Err(unavailable_message(
            component,
            "configured path must be absolute".to_string(),
        ));
    }
    Ok(path.to_path_buf())
}

fn unavailable(
    component: &'static str,
    error: impl std::fmt::Display,
) -> BuiltinNetworkCopilotError {
    unavailable_message(component, error.to_string())
}

fn unavailable_message(component: &'static str, message: String) -> BuiltinNetworkCopilotError {
    BuiltinNetworkCopilotError::Unavailable { component, message }
}

fn parse_role_template(
    role: &'static str,
    template: &str,
) -> Result<DocumentMut, BuiltinNetworkCopilotError> {
    template.parse::<DocumentMut>().map_err(|error| {
        BuiltinNetworkCopilotError::InvalidRoleTemplate {
            role,
            message: error.to_string(),
        }
    })
}

fn load_prepared_descriptor(
    descriptor_path: &Path,
) -> Result<BTreeMap<String, PreparedCapabilityRoot>, BuiltinNetworkCopilotError> {
    const COMPONENT: &str = "prepared Copilot descriptor";
    let descriptor_path = canonical_regular_file(COMPONENT, descriptor_path, false)?;
    let raw =
        fs::read_to_string(&descriptor_path).map_err(|error| unavailable(COMPONENT, error))?;
    let document: PreparedDescriptorDocument =
        serde_json::from_str(&raw).map_err(|error| unavailable(COMPONENT, error))?;
    if document.schema_version != 1 {
        return Err(unavailable_message(
            COMPONENT,
            format!("unsupported schemaVersion {}", document.schema_version),
        ));
    }
    if document.composition_descriptor_sha256.len() != 64
        || !document
            .composition_descriptor_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(unavailable_message(
            COMPONENT,
            "compositionDescriptorSha256 must be a lowercase SHA-256 digest".to_string(),
        ));
    }
    if document.capability_roots.is_empty() {
        return Err(unavailable_message(
            COMPONENT,
            "capabilityRoots must not be empty".to_string(),
        ));
    }

    let mut capability_roots = BTreeMap::new();
    for root in document.capability_roots {
        require_stable_identifier(COMPONENT, "capability root", &root.id)?;
        if root.servers.is_empty() {
            return Err(unavailable_message(
                COMPONENT,
                format!("capability root {} declares no servers", root.id),
            ));
        }
        let mut servers = BTreeMap::new();
        for server in root.servers {
            require_stable_identifier(COMPONENT, "MCP server", &server.id)?;
            if server.transport != "stdio" {
                return Err(unavailable_message(
                    COMPONENT,
                    format!(
                        "capability root {} server {} transport must be stdio",
                        root.id, server.id
                    ),
                ));
            }
            let command = canonical_regular_file(COMPONENT, &server.command, true)?;
            let startup_timeout_sec = positive_timeout(
                COMPONENT,
                &root.id,
                &server.id,
                "startupTimeoutSec",
                server.startup_timeout_sec,
            )?;
            let tool_timeout_sec = positive_timeout(
                COMPONENT,
                &root.id,
                &server.id,
                "toolTimeoutSec",
                server.tool_timeout_sec,
            )?;
            let mut environment_names = BTreeMap::new();
            let mut env_bindings = Vec::new();
            for binding in server.env_bindings {
                require_environment_name(COMPONENT, &binding.name)?;
                if environment_names.insert(binding.name.clone(), ()).is_some() {
                    return Err(unavailable_message(
                        COMPONENT,
                        format!(
                            "capability root {} server {} duplicates environment binding {}",
                            root.id, server.id, binding.name
                        ),
                    ));
                }
                let binding = match binding.source.as_str() {
                    "profile_home" => {
                        reject_binding_metadata(COMPONENT, &root.id, &server.id, &binding)?;
                        PreparedEnvironmentBinding::ProfileHome { name: binding.name }
                    }
                    "tool_state_root" => {
                        reject_binding_metadata(COMPONENT, &root.id, &server.id, &binding)?;
                        PreparedEnvironmentBinding::ToolStateRoot { name: binding.name }
                    }
                    "dependency_root" => {
                        let dependency = binding.dependency.as_deref().ok_or_else(|| {
                            unavailable_message(
                                COMPONENT,
                                format!(
                                    "capability root {} server {} dependency_root binding {} omits dependency",
                                    root.id, server.id, binding.name
                                ),
                            )
                        })?;
                        require_stable_identifier(COMPONENT, "dependency", dependency)?;
                        let resolved_root = binding.resolved_root.as_deref().ok_or_else(|| {
                            unavailable_message(
                                COMPONENT,
                                format!(
                                    "capability root {} server {} dependency_root binding {} omits resolvedRoot",
                                    root.id, server.id, binding.name
                                ),
                            )
                        })?;
                        PreparedEnvironmentBinding::DependencyRoot {
                            name: binding.name,
                            root: canonical_directory(COMPONENT, resolved_root)?,
                        }
                    }
                    "host" => {
                        reject_binding_metadata(COMPONENT, &root.id, &server.id, &binding)?;
                        if !ALLOWED_HOST_ENVIRONMENT_NAMES.contains(&binding.name.as_str()) {
                            return Err(unavailable_message(
                                COMPONENT,
                                format!(
                                    "capability root {} server {} host binding {} is not an allowed proxy environment variable",
                                    root.id, server.id, binding.name
                                ),
                            ));
                        }
                        PreparedEnvironmentBinding::Host { name: binding.name }
                    }
                    source => {
                        return Err(unavailable_message(
                            COMPONENT,
                            format!(
                                "capability root {} server {} binding {} has unsupported source {source}",
                                root.id, server.id, binding.name
                            ),
                        ));
                    }
                };
                env_bindings.push(binding);
            }
            let server_id = server.id;
            if servers
                .insert(
                    server_id.clone(),
                    McpTransport {
                        command,
                        args: server.args,
                        startup_timeout_sec,
                        tool_timeout_sec,
                        env_bindings,
                    },
                )
                .is_some()
            {
                return Err(unavailable_message(
                    COMPONENT,
                    format!(
                        "capability root {} duplicates MCP server {server_id}",
                        root.id
                    ),
                ));
            }
        }
        let root_id = root.id;
        if capability_roots
            .insert(root_id.clone(), PreparedCapabilityRoot { servers })
            .is_some()
        {
            return Err(unavailable_message(
                COMPONENT,
                format!("duplicate capability root {root_id}"),
            ));
        }
    }
    Ok(capability_roots)
}

fn canonical_regular_file(
    component: &'static str,
    path: &Path,
    executable: bool,
) -> Result<PathBuf, BuiltinNetworkCopilotError> {
    let path = require_absolute_path(component, path)?;
    let path = path
        .canonicalize()
        .map_err(|error| unavailable(component, error))?;
    let metadata = fs::metadata(&path).map_err(|error| unavailable(component, error))?;
    if !metadata.is_file() || (executable && !is_executable(&metadata)) {
        let expected = if executable {
            "an executable regular file"
        } else {
            "a regular file"
        };
        return Err(unavailable_message(
            component,
            format!("expected {expected} at {}", path.display()),
        ));
    }
    Ok(path)
}

fn canonical_directory(
    component: &'static str,
    path: &Path,
) -> Result<PathBuf, BuiltinNetworkCopilotError> {
    let path = require_absolute_path(component, path)?;
    let path = path
        .canonicalize()
        .map_err(|error| unavailable(component, error))?;
    if !path.is_dir() {
        return Err(unavailable_message(
            component,
            format!("expected a directory at {}", path.display()),
        ));
    }
    Ok(path)
}

fn require_stable_identifier(
    component: &'static str,
    field: &str,
    value: &str,
) -> Result<(), BuiltinNetworkCopilotError> {
    let mut characters = value.chars();
    let valid = matches!(characters.next(), Some(first) if first.is_ascii_alphanumeric())
        && value.len() <= 128
        && characters.all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '.' | '-')
        });
    if !valid {
        return Err(unavailable_message(
            component,
            format!("{field} {value:?} is not a stable identifier"),
        ));
    }
    Ok(())
}

fn require_environment_name(
    component: &'static str,
    value: &str,
) -> Result<(), BuiltinNetworkCopilotError> {
    let mut characters = value.chars();
    let valid = matches!(characters.next(), Some(first) if first == '_' || first.is_ascii_alphabetic())
        && characters.all(|character| character == '_' || character.is_ascii_alphanumeric());
    if !valid {
        return Err(unavailable_message(
            component,
            format!("invalid environment variable name {value:?}"),
        ));
    }
    Ok(())
}

fn reject_binding_metadata(
    component: &'static str,
    root_id: &str,
    server_id: &str,
    binding: &PreparedDescriptorEnvironmentBinding,
) -> Result<(), BuiltinNetworkCopilotError> {
    if binding.dependency.is_some() || binding.resolved_root.is_some() {
        return Err(unavailable_message(
            component,
            format!(
                "capability root {root_id} server {server_id} binding {} permits dependency and resolvedRoot only for dependency_root",
                binding.name
            ),
        ));
    }
    Ok(())
}

fn positive_timeout(
    component: &'static str,
    root_id: &str,
    server_id: &str,
    field: &str,
    value: Option<i64>,
) -> Result<Option<i64>, BuiltinNetworkCopilotError> {
    if matches!(value, Some(value) if value <= 0) {
        return Err(unavailable_message(
            component,
            format!("capability root {root_id} server {server_id} {field} must be positive"),
        ));
    }
    Ok(value)
}

fn project_role_mcp_servers(
    role: &mut DocumentMut,
    role_name: &'static str,
    capability_roots: &BTreeMap<String, PreparedCapabilityRoot>,
    profile_home: &Path,
) -> Result<(), BuiltinNetworkCopilotError> {
    let plugins = role
        .get("plugins")
        .and_then(Item::as_table)
        .ok_or_else(|| invalid_role(role_name, "Role omits plugins policy"))?;
    let mut runtime_servers = Table::new();
    for (capability_root_id, plugin) in plugins {
        let plugin = plugin.as_table().ok_or_else(|| {
            invalid_role(
                role_name,
                format!("plugins.{capability_root_id} is not a table"),
            )
        })?;
        let policies = plugin
            .get("mcp_servers")
            .and_then(Item::as_table)
            .ok_or_else(|| {
                invalid_role(
                    role_name,
                    format!("plugins.{capability_root_id} omits mcp_servers"),
                )
            })?;
        let declared = capability_roots.get(capability_root_id).ok_or_else(|| {
            invalid_role(
                role_name,
                format!("plugins.{capability_root_id} has no prepared capability root"),
            )
        })?;
        for (server_name, policy) in policies {
            if runtime_servers.contains_key(server_name) {
                return Err(invalid_role(
                    role_name,
                    format!("MCP server {server_name} has multiple transports"),
                ));
            }
            let transport = declared.servers.get(server_name).ok_or_else(|| {
                invalid_role(
                    role_name,
                    format!(
                        "plugins.{capability_root_id} references unprepared server {server_name}"
                    ),
                )
            })?;
            let policy = policy.as_table().ok_or_else(|| {
                invalid_role(role_name, format!("server {server_name} policy is invalid"))
            })?;
            for (key, _) in policy {
                if !ROLE_MCP_SERVER_POLICY_KEYS.contains(&key) {
                    return Err(invalid_role(
                        role_name,
                        format!(
                            "plugins.{capability_root_id}.mcp_servers.{server_name}.{key} is not part of Role MCP server policy"
                        ),
                    ));
                }
            }
            let mut runtime = Table::new();
            for key in ROLE_MCP_SERVER_POLICY_KEYS {
                if let Some(item) = policy.get(key) {
                    runtime[key] = item.clone();
                }
            }
            runtime["command"] = value(path_text("prepared MCP command", &transport.command)?);
            let mut args = Array::new();
            for argument in &transport.args {
                args.push(argument.as_str());
            }
            runtime["args"] = value(args);
            if let Some(timeout) = transport.startup_timeout_sec {
                runtime["startup_timeout_sec"] = value(timeout);
            }
            if let Some(timeout) = transport.tool_timeout_sec {
                runtime["tool_timeout_sec"] = value(timeout);
            }
            let mut env = Table::new();
            let mut inherited_env_vars = Array::new();
            let tool_state_root = profile_home
                .join(".open-web-codex")
                .join("mcp-state")
                .join(capability_root_id);
            for binding in &transport.env_bindings {
                match binding {
                    PreparedEnvironmentBinding::ProfileHome { name } => {
                        env[name] = value(path_text("Profile home binding", profile_home)?);
                    }
                    PreparedEnvironmentBinding::ToolStateRoot { name } => {
                        env[name] = value(path_text("Tool state binding", &tool_state_root)?);
                    }
                    PreparedEnvironmentBinding::DependencyRoot { name, root } => {
                        env[name] = value(path_text("dependency root binding", root)?);
                    }
                    PreparedEnvironmentBinding::Host { name } => {
                        inherited_env_vars.push(name.as_str());
                    }
                }
            }
            runtime["env"] = Item::Table(env);
            runtime["env_vars"] = value(inherited_env_vars);
            runtime_servers[server_name] = Item::Table(runtime);
        }
    }
    role.remove("plugins");
    role["mcp_servers"] = Item::Table(runtime_servers);
    Ok(())
}

fn invalid_role(role: &'static str, message: impl Into<String>) -> BuiltinNetworkCopilotError {
    BuiltinNetworkCopilotError::InvalidRoleTemplate {
        role,
        message: message.into(),
    }
}

fn finish_role_template(
    _role_name: &'static str,
    role: DocumentMut,
) -> Result<String, BuiltinNetworkCopilotError> {
    Ok(role.to_string())
}

fn path_text<'a>(
    component: &'static str,
    path: &'a Path,
) -> Result<&'a str, BuiltinNetworkCopilotError> {
    path.to_str().ok_or_else(|| {
        unavailable_message(
            component,
            format!("path is not valid UTF-8: {}", path.display()),
        )
    })
}

#[cfg(unix)]
fn is_executable(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_metadata: &fs::Metadata) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_file(path: &Path, contents: &str, executable: bool) {
        fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
        fs::write(path, contents).expect("write fixture");
        #[cfg(unix)]
        if executable {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o700)).expect("make executable");
        }
        let _ = executable;
    }

    fn fixture() -> (
        tempfile::TempDir,
        BuiltinNetworkCopilotAssets,
        PathBuf,
        PathBuf,
    ) {
        let temp = tempfile::tempdir().expect("temp dir");
        let prepared = temp.path().join("prepared 运行态");
        let supply_python = prepared.join("dependencies/supply python/bin/python");
        let maps_python = prepared.join("dependencies/maps python/bin/python");
        let style_spec = prepared.join("dependencies/map style spec");
        let profile = temp.path().join("profile 用户");
        for directory in [&profile, &style_spec] {
            fs::create_dir_all(directory).expect("fixture directory");
        }
        write_file(&supply_python, "#!/bin/sh\n", true);
        write_file(&maps_python, "#!/bin/sh\n", true);
        let descriptor = prepared.join("prepared-tools.v1.json");
        write_file(
            &descriptor,
            &serde_json::to_string_pretty(&serde_json::json!({
                "schemaVersion": 1,
                "compositionDescriptorSha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "capabilityRoots": [
                    {
                        "id": "supply_chain",
                        "servers": [
                            {
                                "id": "supply_chain_data",
                                "transport": "stdio",
                                "command": supply_python,
                                "args": ["-m", "prepared.data"],
                                "envBindings": [
                                    {"name": "CODEX_HOME", "source": "profile_home"},
                                    {"name": "HTTP_PROXY", "source": "host"}
                                ]
                            },
                            {
                                "id": "supply_chain",
                                "transport": "stdio",
                                "command": supply_python,
                                "args": ["-m", "prepared.network"],
                                "envBindings": [
                                    {"name": "CODEX_HOME", "source": "profile_home"},
                                    {"name": "HTTP_PROXY", "source": "host"}
                                ]
                            }
                        ]
                    },
                    {
                        "id": "map_utils",
                        "servers": [{
                            "id": "map_utils",
                            "transport": "stdio",
                            "command": maps_python,
                            "args": ["-m", "prepared.maps"],
                            "envBindings": [
                                {"name": "TOOL_STATE", "source": "tool_state_root"},
                                {
                                    "name": "DEPENDENCY_ASSETS",
                                    "source": "dependency_root",
                                    "dependency": "style-spec",
                                    "resolvedRoot": style_spec
                                },
                                {"name": "HTTP_PROXY", "source": "host"}
                            ],
                            "startupTimeoutSec": 60,
                            "toolTimeoutSec": 90
                        }]
                    }
                ]
            }))
            .expect("serialize descriptor"),
            false,
        );

        let assets = BuiltinNetworkCopilotAssets::resolve(&descriptor).expect("resolve assets");
        (temp, assets, profile, descriptor)
    }

    #[test]
    fn renders_native_profile_seeds_with_role_local_mcp_only() {
        let (_temp, assets, profile, _descriptor) = fixture();
        let data = assets.render_data_role(&profile).expect("data role");
        let network = assets.render_network_role(&profile).expect("network role");

        let data = data.parse::<DocumentMut>().expect("parse data role");
        let network = network.parse::<DocumentMut>().expect("parse network role");
        assert_eq!(
            data["nickname_candidates"]
                .as_array()
                .expect("data nicknames")
                .iter()
                .filter_map(|item| item.as_str())
                .collect::<Vec<_>>(),
            vec!["Wanwan"]
        );
        assert!(data["mcp_servers"]["supply_chain_data"]
            .get("cwd")
            .is_none());
        assert_eq!(
            data["mcp_servers"]["supply_chain_data"]["args"]
                .as_array()
                .expect("data args")
                .iter()
                .filter_map(|item| item.as_str())
                .collect::<Vec<_>>(),
            vec!["-m", "prepared.data"]
        );
        assert_eq!(
            data["mcp_servers"]["supply_chain_data"]["default_tools_approval_mode"].as_str(),
            Some("approve")
        );
        assert_eq!(
            data["mcp_servers"]["supply_chain_data"]["enabled_tools"]
                .as_array()
                .expect("data tools")
                .iter()
                .filter_map(|item| item.as_str())
                .collect::<Vec<_>>(),
            vec![
                "discover_workspace_sources",
                "inspect_workspace_sources",
                "normalize_network_input",
                "prepare_network_geography",
            ]
        );
        assert_eq!(
            data["mcp_servers"]["supply_chain_data"]["env"]["CODEX_HOME"].as_str(),
            profile.to_str()
        );
        assert_eq!(
            data["mcp_servers"]["supply_chain_data"]["env_vars"]
                .as_array()
                .and_then(|values| values.get(0))
                .and_then(|value| value.as_str()),
            Some("HTTP_PROXY")
        );
        assert!(data["developer_instructions"]
            .as_str()
            .expect("data instructions")
            .contains("Do not use shell, Workspace command, Git, jq, or ad-hoc Python"));
        assert!(network["mcp_servers"]["supply_chain"].get("cwd").is_none());
        assert_eq!(
            network["mcp_servers"]["supply_chain"]["args"]
                .as_array()
                .expect("network args")
                .iter()
                .filter_map(|item| item.as_str())
                .collect::<Vec<_>>(),
            vec!["-m", "prepared.network"]
        );
        let supply_chain_tools = network["mcp_servers"]["supply_chain"]["enabled_tools"]
            .as_array()
            .expect("supply chain tools")
            .iter()
            .filter_map(|item| item.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            supply_chain_tools,
            vec![
                "plan_route_matrix",
                "build_haversine_route_matrix",
                "build_provided_route_matrix",
                "validate_route_matrix",
                "register_navigation_route_matrix",
                "plan_cost_matrix",
                "prepare_network_distribution_map",
                "prepare_network_comparison_map",
                "evaluate_network_baseline",
                "assess_facility_change",
                "evaluate_facility_scenario",
                "solve_p_median",
                "compare_network_scenarios",
                "render_network_comparison_map",
                "publish_network_planning_report",
            ]
        );
        assert_eq!(
            network["mcp_servers"]["supply_chain"]["default_tools_approval_mode"].as_str(),
            Some("prompt")
        );
        for tool in &supply_chain_tools[..13] {
            assert_eq!(
                network["mcp_servers"]["supply_chain"]["tools"][*tool]["approval_mode"].as_str(),
                Some("approve"),
                "safe Network Tool {tool} must be preapproved",
            );
        }
        for tool in &supply_chain_tools[13..] {
            assert!(
                network["mcp_servers"]["supply_chain"]["tools"]
                    .get(*tool)
                    .is_none(),
                "final Workspace Tool {tool} must inherit prompt",
            );
        }
        assert_eq!(
            network["mcp_servers"]["map_utils"]["command"].as_str(),
            Some(
                path_text(
                    "maps command",
                    &assets.capability_roots["map_utils"].servers["map_utils"].command
                )
                .expect("path")
            )
        );
        assert!(network["mcp_servers"]["map_utils"].get("cwd").is_none());
        assert_eq!(
            network["mcp_servers"]["map_utils"]["args"]
                .as_array()
                .expect("map args")
                .iter()
                .filter_map(|item| item.as_str())
                .collect::<Vec<_>>(),
            vec!["-m", "prepared.maps"]
        );
        assert_eq!(
            network["mcp_servers"]["map_utils"]["env"]["TOOL_STATE"].as_str(),
            profile.join(".open-web-codex/mcp-state/map_utils").to_str()
        );
        assert!(
            network["mcp_servers"]["map_utils"]["env"]["DEPENDENCY_ASSETS"]
                .as_str()
                .is_some()
        );
        assert_eq!(
            network["mcp_servers"]["map_utils"]["startup_timeout_sec"].as_integer(),
            Some(60)
        );
        assert_eq!(
            network["mcp_servers"]["map_utils"]["tool_timeout_sec"].as_integer(),
            Some(90)
        );
        let map_tools = network["mcp_servers"]["map_utils"]["enabled_tools"]
            .as_array()
            .expect("map tools")
            .iter()
            .filter_map(|item| item.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            map_tools,
            vec!["get_route", "distance_matrix", "create_map_card",]
        );
        assert_eq!(
            network["mcp_servers"]["map_utils"]["default_tools_approval_mode"].as_str(),
            Some("prompt")
        );
        assert_eq!(
            network["mcp_servers"]["map_utils"]["tools"]["create_map_card"]["approval_mode"]
                .as_str(),
            Some("approve")
        );
        for tool in ["get_route", "distance_matrix"] {
            assert!(
                network["mcp_servers"]["map_utils"]["tools"]
                    .get(tool)
                    .is_none(),
                "external Map Tool {tool} must inherit prompt",
            );
        }
        assert!(network["developer_instructions"]
            .as_str()
            .expect("network instructions")
            .contains("Do not use shell, Workspace command, Git, jq, or ad-hoc Python"));
        let network_instructions = network["instructions"]
            .as_str()
            .expect("network Role instructions");
        assert!(network_instructions.contains("focused warehouse-network analysis Agent"));
        assert!(!network_instructions.contains("You are a coding agent"));
        assert_eq!(
            network["include_permissions_instructions"].as_bool(),
            Some(false)
        );
        assert_eq!(network["include_apps_instructions"].as_bool(), Some(false));
        assert_eq!(
            network["include_collaboration_mode_instructions"].as_bool(),
            Some(false)
        );
        assert_eq!(
            network["skills"]["bundled"]["enabled"].as_bool(),
            Some(false)
        );
        assert!(network["skills"]["config"]
            .as_array_of_tables()
            .expect("network Skill policy")
            .iter()
            .any(|skill| {
                skill["name"].as_str() == Some("warehouse-network")
                    && skill["enabled"].as_bool() == Some(true)
            }));
        assert!(!data.to_string().contains("__OPEN_WEB_CODEX_"));
        assert!(!network.to_string().contains("__OPEN_WEB_CODEX_"));
        assert!(data.get("plugins").is_none());
        assert!(network.get("plugins").is_none());
        assert_eq!(
            assets.startup_files(&profile).expect("startup files").len(),
            5
        );
        assert!(!profile.join("config.toml").exists());
    }

    #[test]
    fn rejects_transport_cwd_in_author_role_mcp_server_policy() {
        let (_temp, assets, profile, _descriptor) = fixture();
        let mut role = parse_role_template("data", DATA_ROLE).expect("parse data Role");
        role["plugins"]["supply_chain"]["mcp_servers"]["supply_chain_data"]["cwd"] =
            value("/private/tool-source");

        let error = project_role_mcp_servers(&mut role, "data", &assets.capability_roots, &profile)
            .expect_err("author Role transport cwd must be rejected");

        assert!(matches!(
            error,
            BuiltinNetworkCopilotError::InvalidRoleTemplate { message, .. }
                if message.contains("plugins.supply_chain.mcp_servers.supply_chain_data.cwd")
        ));
    }

    #[test]
    fn acceptance_profile_disables_only_remote_model_visible_surfaces() {
        assert_eq!(
            disabled_codex_features(),
            [
                CodexFeature::Plugins,
                CodexFeature::RemotePlugin,
                CodexFeature::Apps,
                CodexFeature::ToolSuggest,
            ]
        );
        assert_eq!(
            enabled_codex_features(),
            [CodexFeature::DefaultModeRequestUserInput]
        );
    }

    #[test]
    fn warehouse_skills_define_composable_business_decisions() {
        assert!(SUPERVISOR_SKILL.contains("让 `network_agent` 定义本次分析所需的数据"));
        assert!(SUPERVISOR_SKILL.contains("Role 昵称固定为 `Wanwan`"));
        assert!(SUPERVISOR_SKILL.contains("任何仓网领域执行都必须委派给对应原生 Role"));
        assert!(SUPERVISOR_SKILL
            .contains("不得用 shell、Workspace 命令、内联代码或自身推理代替 child Tool"));
        assert!(SUPERVISOR_SKILL.contains("必须原生创建或继续 `network_agent`"));
        assert!(SUPERVISOR_SKILL.contains("不得让用户复制代码到浏览器"));
        assert!(SUPERVISOR_SKILL.contains("普通 Workspace 文件路径不是 MCP ResourceRef"));
        assert!(SUPERVISOR_SKILL.contains("必须先创建 `data_agent` 重新检查并发布当前 Resource"));
        assert!(SUPERVISOR_SKILL.contains(
            "不得要求 Network Agent 从 Workspace 文件、旧 Artifact、报告或 Resource 列表寻找引用"
        ));
        assert!(SUPERVISOR_SKILL.contains("不得降级为自制 HTML、文件、图片、文本地图或伪造成功"));
        assert!(SUPERVISOR_SKILL.contains("必须由 Root 调用原生 `request_user_input`"));
        assert!(SUPERVISOR_SKILL.contains("按能力 owner 收敛"));
        assert!(SUPERVISOR_SKILL.contains("不要先创建“只盘点文件”的 Data Task"));
        assert!(SUPERVISOR_SKILL.contains("不自行猜测、缩写或转换国家代码"));
        assert!(SUPERVISOR_SKILL.contains("不要再次询问是否纳入候选仓"));
        assert!(SUPERVISOR_SKILL.contains("一次用户请求只保留一份正式简报"));
        assert!(SUPERVISOR_SKILL.contains("typed `needs_context`"));
        assert!(SUPERVISOR_SKILL.contains("不得 list resources、读取 Resource 正文"));
        assert!(SUPERVISOR_SKILL.contains("原 `network_agent` 已处于 safe/terminal 边界"));
        assert!(SUPERVISOR_SKILL.contains("以该 typed input 为 handoff 边界"));
        assert!(SUPERVISOR_SKILL.contains("只传本次调用实际需要且 Tool 接受的 exact Resource refs"));
        assert!(SUPERVISOR_SKILL.contains("不要传 Tool 不接收的上游 refs"));
        assert!(SUPERVISOR_SKILL.contains("不要复述已经封装在 Resource 中的路线、成本、求解规则"));
        assert!(SUPERVISOR_SKILL.contains("显式使用 `fork_turns=none`"));
        assert!(SUPERVISOR_SKILL
            .contains("只有当后续工作的正确性确实依赖原 child 对话中尚未结构化的判断或上下文时"));
        assert!(SUPERVISOR_SKILL.contains("spawn 后只等待 child，不再次复述 handoff"));
        assert!(SUPERVISOR_SKILL.contains("`wait` 已返回 child `completed` 时，该 child 已是终态"));
        assert!(SUPERVISOR_SKILL.contains("不再调用 `closeAgent` 或其他终止工具"));
        assert!(SUPERVISOR_SKILL.contains("Root 只简短汇总用户要求的业务变化"));
        assert!(SUPERVISOR_SKILL
            .contains("final Tool/Artifact typed descriptor 与 Platform terminal state 是权威"));
        assert!(SUPERVISOR_SKILL.contains(
            "下游 Tool 同时接收 result 与 comparison 时，comparison 必须由同一个 exact result 产生"
        ));
        assert!(DATA_SKILL.contains("`city_id`、`city_name`、`demand_quantity`"));
        assert!(DATA_SKILL.contains("不要从行政区目录静默生成候选仓"));
        assert!(DATA_SKILL.contains("路线距离、时长和计算来源"));
        assert!(DATA_SKILL.contains("ISO 3166-1 两位大写代码"));
        assert!(DATA_SKILL.contains("不得再枚举全部 MCP Resources"));
        assert!(NETWORK_SKILL.contains("潜在接口调用量和费用"));
        assert!(NETWORK_SKILL.contains("按城市数量和按需求量加权"));
        assert!(NETWORK_SKILL.contains("`map_card_handoff.arguments` 整体原样作为调用参数"));
        assert!(NETWORK_SKILL.contains("立即向 Supervisor 返回 `needs_data`"));
        assert!(NETWORK_SKILL.contains("不得从 Workspace 文件、旧 Artifact、报告、模型文本"));
        assert!(NETWORK_SKILL.contains("默认表示对话内地图卡片"));
        assert!(NETWORK_SKILL.contains("`structuredContent.embed.code`"));
        assert!(NETWORK_SKILL.contains("不调用路线矩阵、成本矩阵、baseline"));
        assert!(NETWORK_SKILL.contains("固定集合与可选集合必须完整、不重叠"));
        assert!(NETWORK_SKILL.contains("把这些 Tool 当作可组合能力"));
        assert!(NETWORK_SKILL.contains("应主动生成对话内地图卡片"));
        assert!(NETWORK_SKILL.contains("create-new 中文 Markdown 文件"));
        assert!(NETWORK_SKILL.contains("Workspace 相对 Markdown 链接逐字原样"));
        assert!(NETWORK_SKILL.contains("只调用一次 `publish_network_planning_report`"));
        assert!(NETWORK_SKILL.contains("正式简报以该实际基线为准"));
        assert!(NETWORK_SKILL.contains("不要让用户在两种指标中二选一"));
        assert!(NETWORK_SKILL.contains("矩阵 Resource 自己携带该 scope"));
        assert!(NETWORK_SKILL.contains("完整仓网分析、模拟或规划达到可交付终态时"));
        assert!(NETWORK_SKILL.contains("不用 JSON 报告冒充 Excel"));
        assert!(NETWORK_SKILL.contains("`assess_facility_change` 当前 typed input"));
        assert!(NETWORK_SKILL.contains("只调用一次 `assess_facility_change`"));
        assert!(NETWORK_SKILL.contains(
            "不要把同一动作拆为 `evaluate_facility_scenario` 与 `compare_network_scenarios`"
        ));
        assert!(NETWORK_SKILL.contains("以准备调用的 Tool 当前 typed input 为边界"));
        assert!(NETWORK_SKILL.contains("不要携带该 Tool 不接收的上游 refs"));
        assert!(NETWORK_SKILL.contains("Tool 成功后只基于同一次 structured result"));
        assert!(NETWORK_SKILL.contains("不重新推导、不复述输入政策、验证过程或相同结论"));
        assert!(NETWORK_SKILL.contains("typed `needs_context`"));
        assert!(NETWORK_SKILL
            .contains("final Tool/Artifact typed descriptor 与 Platform terminal state 是权威"));
        assert!(NETWORK_SKILL.contains(
            "下游 Tool 同时接收 result 与 comparison 时，comparison 必须由同一个 exact result 产生"
        ));
        assert!(NETWORK_ROLE.contains("call the matching domain Tool directly"));
        assert!(NETWORK_ROLE.contains("call assess_facility_change exactly once"));
        assert!(NETWORK_ROLE.contains(
            "do not call read_mcp_resource, evaluate_facility_scenario, or compare_network_scenarios"
        ));
        assert!(NETWORK_ROLE.contains("pass its arguments unchanged"));
        for deprecated in [
            "compute_optimal_assignment",
            "evaluate_service_targets",
            "summarize_network_cost",
            "solve_service_constrained_location",
        ] {
            assert!(!NETWORK_ROLE.contains(deprecated));
        }
        for case_specific in [
            "Indonesia",
            "印尼",
            "BEKASI",
            "KENDARI",
            "MANADO",
            "6/12/18",
            "demand-cities.csv",
        ] {
            assert!(!SUPERVISOR_SKILL.contains(case_specific));
            assert!(!DATA_SKILL.contains(case_specific));
            assert!(!NETWORK_SKILL.contains(case_specific));
        }
    }

    #[test]
    fn rejects_missing_or_relative_prepared_descriptor_as_unavailable() {
        let temp = tempfile::tempdir().expect("temp dir");
        assert!(matches!(
            BuiltinNetworkCopilotAssets::resolve(Path::new("relative")),
            Err(BuiltinNetworkCopilotError::Unavailable { .. })
        ));
        assert!(matches!(
            BuiltinNetworkCopilotAssets::resolve(&temp.path().join("missing")),
            Err(BuiltinNetworkCopilotError::Unavailable { .. })
        ));
    }

    #[test]
    fn rejects_unknown_fields_and_duplicate_prepared_capability_roots() {
        let (_temp, _assets, _profile, descriptor) = fixture();
        let mut document: serde_json::Value =
            serde_json::from_slice(&fs::read(&descriptor).expect("read descriptor"))
                .expect("parse descriptor fixture");
        document["unexpected"] = serde_json::json!(true);
        fs::write(
            &descriptor,
            serde_json::to_vec(&document).expect("serialize descriptor"),
        )
        .expect("write unknown field fixture");
        assert!(matches!(
            BuiltinNetworkCopilotAssets::resolve(&descriptor),
            Err(BuiltinNetworkCopilotError::Unavailable { .. })
        ));

        document
            .as_object_mut()
            .expect("descriptor object")
            .remove("unexpected");
        document["capabilityRoots"][0]["servers"][0]["cwd"] =
            serde_json::json!("/obsolete/runtime-cwd");
        fs::write(
            &descriptor,
            serde_json::to_vec(&document).expect("serialize descriptor"),
        )
        .expect("write obsolete cwd fixture");
        assert!(matches!(
            BuiltinNetworkCopilotAssets::resolve(&descriptor),
            Err(BuiltinNetworkCopilotError::Unavailable { .. })
        ));

        document["capabilityRoots"][0]["servers"][0]
            .as_object_mut()
            .expect("prepared server object")
            .remove("cwd");
        let duplicate = document["capabilityRoots"][0].clone();
        document["capabilityRoots"]
            .as_array_mut()
            .expect("capability roots")
            .push(duplicate);
        fs::write(
            &descriptor,
            serde_json::to_vec(&document).expect("serialize descriptor"),
        )
        .expect("write duplicate root fixture");
        assert!(matches!(
            BuiltinNetworkCopilotAssets::resolve(&descriptor),
            Err(BuiltinNetworkCopilotError::Unavailable { .. })
        ));
    }

    #[test]
    fn rejects_prepared_host_binding_outside_proxy_capability() {
        let (_temp, _assets, _profile, descriptor) = fixture();
        let mut document: serde_json::Value =
            serde_json::from_slice(&fs::read(&descriptor).expect("read descriptor"))
                .expect("parse descriptor fixture");
        document["capabilityRoots"][0]["servers"][0]["envBindings"][1]["name"] =
            serde_json::json!("OPEN_WEB_CODEX_MASTER_KEY");
        fs::write(
            &descriptor,
            serde_json::to_vec(&document).expect("serialize descriptor"),
        )
        .expect("write host binding fixture");

        assert!(matches!(
            BuiltinNetworkCopilotAssets::resolve(&descriptor),
            Err(BuiltinNetworkCopilotError::Unavailable { .. })
        ));
    }

    #[cfg(unix)]
    #[test]
    fn prepared_descriptor_accepts_canonical_paths_and_rejects_non_executable_command() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().expect("temp dir");
        let real = temp.path().join("real");
        fs::create_dir_all(&real).expect("real root");
        let linked = temp.path().join("linked");
        symlink(&real, &linked).expect("symlink root");
        assert_eq!(
            canonical_directory("linked root", &linked).expect("canonical root"),
            real.canonicalize().expect("canonical real root")
        );

        let command = temp.path().join("command");
        write_file(&command, "not executable\n", false);
        assert!(matches!(
            canonical_regular_file("prepared command", &command, true),
            Err(BuiltinNetworkCopilotError::Unavailable { .. })
        ));
    }
}
