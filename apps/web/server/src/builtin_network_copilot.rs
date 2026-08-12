use std::fs;
use std::path::{Path, PathBuf};

use open_web_codex_adapter::real::ThreadSkillConfig;
use open_web_codex_profile_host::{CodexFeature, ProfileStartupFile};
use thiserror::Error;
use toml_edit::{value, Array, DocumentMut, Item};

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

const SUPPLY_CHAIN_LAUNCHER: &str = "bin/supply-chain-planner-launcher";
const MAPS_LAUNCHER: &str = "bin/maps-mcp-launcher";
const SUPPLY_CHAIN_RUNTIME_FILE: &str = "supply_chain_planner/server.py";
const SUPPLY_CHAIN_DATA_RUNTIME_FILE: &str = "supply_chain_planner/data_server.py";
const MAPS_RUNTIME_FILE: &str = "maps_mcp/server.py";
const MAPS_STYLE_SPEC_PACKAGE: &str = "node_modules/@mapbox/mapbox-gl-style-spec/package.json";

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
    supply_chain_launcher: PathBuf,
    supply_chain_venv: PathBuf,
    maps_root: PathBuf,
    maps_launcher: PathBuf,
    maps_venv: PathBuf,
}

impl BuiltinNetworkCopilotAssets {
    pub(crate) fn resolve(
        supply_chain_root: &Path,
        supply_chain_venv: &Path,
        maps_root: &Path,
        maps_venv: &Path,
    ) -> Result<Self, BuiltinNetworkCopilotError> {
        let supply_chain_root =
            required_absolute_directory("supply-chain application asset root", supply_chain_root)?;
        let supply_chain_launcher = required_regular_file(
            "supply-chain launcher",
            &supply_chain_root,
            SUPPLY_CHAIN_LAUNCHER,
            true,
        )?;
        for (component, relative) in [
            ("supply-chain runtime", SUPPLY_CHAIN_RUNTIME_FILE),
            ("supply-chain data runtime", SUPPLY_CHAIN_DATA_RUNTIME_FILE),
        ] {
            required_regular_file(component, &supply_chain_root, relative, false)?;
        }
        let supply_chain_venv =
            required_python_environment("supply-chain Python environment", supply_chain_venv)?;

        let maps_root = required_absolute_directory("maps application asset root", maps_root)?;
        let maps_launcher =
            required_regular_file("maps launcher", &maps_root, MAPS_LAUNCHER, true)?;
        required_regular_file("maps runtime", &maps_root, MAPS_RUNTIME_FILE, false)?;
        required_regular_file(
            "Mapbox Style Spec runtime",
            &maps_root,
            MAPS_STYLE_SPEC_PACKAGE,
            false,
        )?;
        let maps_venv = required_python_environment("maps Python environment", maps_venv)?;

        Ok(Self {
            supply_chain_launcher,
            supply_chain_venv,
            maps_root,
            maps_launcher,
            maps_venv,
        })
    }

    pub(crate) fn startup_files(
        &self,
        profile_home: &Path,
    ) -> Result<Vec<ProfileStartupFile>, BuiltinNetworkCopilotError> {
        let profile_home = require_absolute_path("Profile CODEX_HOME", profile_home)?;
        let profile_runtime = profile_home.join(".open-web-codex");
        let profile_logs = profile_runtime.join("logs");
        let data_role = self.render_data_role(&profile_home, &profile_runtime, &profile_logs)?;
        let network_role =
            self.render_network_role(&profile_home, &profile_runtime, &profile_logs)?;

        Ok(vec![
            ProfileStartupFile::skill("warehouse-supervisor", SUPERVISOR_SKILL.as_bytes())?,
            ProfileStartupFile::skill("warehouse-data", DATA_SKILL.as_bytes())?,
            ProfileStartupFile::skill("warehouse-network", NETWORK_SKILL.as_bytes())?,
            ProfileStartupFile::agent_role("data_agent", data_role.into_bytes())?,
            ProfileStartupFile::agent_role("network_agent", network_role.into_bytes())?,
        ])
    }

    fn render_data_role(
        &self,
        profile_home: &Path,
        profile_runtime: &Path,
        profile_logs: &Path,
    ) -> Result<String, BuiltinNetworkCopilotError> {
        let mut role = parse_role_template("data_agent", DATA_ROLE)?;
        configure_native_workspace_mcp_server(
            &mut role,
            "supply_chain",
            &self.supply_chain_launcher,
            &["--data-server"],
            &[
                ("CODEX_HOME", profile_home),
                ("OPEN_WEB_CODEX_DATA_DIR", profile_runtime),
                ("OPEN_WEB_CODEX_LOG_DIR", profile_logs),
                (
                    "OPEN_WEB_CODEX_SUPPLY_CHAIN_MCP_VENV",
                    &self.supply_chain_venv,
                ),
            ],
        )?;
        finish_role_template("data_agent", role)
    }

    fn render_network_role(
        &self,
        profile_home: &Path,
        profile_runtime: &Path,
        profile_logs: &Path,
    ) -> Result<String, BuiltinNetworkCopilotError> {
        let mut role = parse_role_template("network_agent", NETWORK_ROLE)?;
        configure_native_workspace_mcp_server(
            &mut role,
            "supply_chain",
            &self.supply_chain_launcher,
            &[],
            &[
                ("CODEX_HOME", profile_home),
                ("OPEN_WEB_CODEX_DATA_DIR", profile_runtime),
                ("OPEN_WEB_CODEX_LOG_DIR", profile_logs),
                (
                    "OPEN_WEB_CODEX_SUPPLY_CHAIN_MCP_VENV",
                    &self.supply_chain_venv,
                ),
            ],
        )?;
        configure_mcp_server(
            &mut role,
            "map_utils",
            &self.maps_launcher,
            &["--workspace-root"],
            &profile_runtime.join("mcp-state/maps-mcp"),
            &self.maps_root,
            &[
                ("CODEX_HOME", profile_home),
                ("OPEN_WEB_CODEX_DATA_DIR", profile_runtime),
                ("OPEN_WEB_CODEX_LOG_DIR", profile_logs),
                ("OPEN_WEB_CODEX_MAPS_MCP_VENV", &self.maps_venv),
                ("MAPS_MCP_VENV", &self.maps_venv),
            ],
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

fn required_absolute_directory(
    component: &'static str,
    path: &Path,
) -> Result<PathBuf, BuiltinNetworkCopilotError> {
    let path = require_absolute_path(component, path)?;
    let metadata = fs::metadata(&path).map_err(|error| unavailable(component, error))?;
    if !metadata.is_dir() {
        return Err(unavailable_message(
            component,
            format!("expected a directory at {}", path.display()),
        ));
    }
    path.canonicalize()
        .map_err(|error| unavailable(component, error))
}

fn required_regular_file(
    component: &'static str,
    root: &Path,
    relative: &str,
    executable: bool,
) -> Result<PathBuf, BuiltinNetworkCopilotError> {
    let path = root.join(relative);
    let metadata = fs::metadata(&path).map_err(|error| unavailable(component, error))?;
    if !metadata.is_file() {
        return Err(unavailable_message(
            component,
            format!("expected a regular file at {}", path.display()),
        ));
    }
    if executable && !is_executable(&metadata) {
        return Err(unavailable_message(
            component,
            format!("expected an executable file at {}", path.display()),
        ));
    }
    let path = path
        .canonicalize()
        .map_err(|error| unavailable(component, error))?;
    if !path.starts_with(root) {
        return Err(unavailable_message(
            component,
            "configured file escapes its application asset root".to_string(),
        ));
    }
    Ok(path)
}

fn required_python_environment(
    component: &'static str,
    path: &Path,
) -> Result<PathBuf, BuiltinNetworkCopilotError> {
    let root = required_absolute_directory(component, path)?;
    let python = root.join("bin/python");
    let metadata = fs::metadata(&python).map_err(|error| unavailable(component, error))?;
    if !metadata.is_file() || !is_executable(&metadata) {
        return Err(unavailable_message(
            component,
            format!("expected an executable bin/python under {}", root.display()),
        ));
    }
    Ok(root)
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

fn configure_mcp_server(
    role: &mut DocumentMut,
    server_name: &'static str,
    command: &Path,
    leading_args: &[&str],
    workspace_argument: &Path,
    process_cwd: &Path,
    environment: &[(&'static str, &Path)],
) -> Result<(), BuiltinNetworkCopilotError> {
    let role_name = role
        .get("name")
        .and_then(Item::as_str)
        .unwrap_or("unknown")
        .to_string();
    let server = role
        .get_mut("mcp_servers")
        .and_then(Item::as_table_mut)
        .and_then(|servers| servers.get_mut(server_name))
        .and_then(Item::as_table_mut)
        .ok_or_else(|| BuiltinNetworkCopilotError::InvalidRoleTemplate {
            role: server_name,
            message: format!("Role {role_name} omits mcp_servers.{server_name}"),
        })?;
    server["command"] = value(path_text("MCP launcher", command)?);
    server["cwd"] = value(path_text("MCP process cwd", process_cwd)?);
    let mut args = Array::new();
    for argument in leading_args {
        args.push(*argument);
    }
    args.push(path_text("MCP workspace argument", workspace_argument)?);
    server["args"] = value(args);

    let env = server
        .get_mut("env")
        .and_then(Item::as_table_mut)
        .ok_or_else(|| BuiltinNetworkCopilotError::InvalidRoleTemplate {
            role: server_name,
            message: format!("Role {role_name} omits mcp_servers.{server_name}.env"),
        })?;
    for (name, path) in environment {
        env[name] = value(path_text("MCP environment path", path)?);
    }
    Ok(())
}

/// Configure a supply-chain MCP server to inherit Codex's native Thread cwd.
///
/// The launcher is absolute and the application assets are resolved by the
/// launcher itself.  Supplying a static MCP `cwd` here would make the asset
/// directory masquerade as the authorized business Workspace.
fn configure_native_workspace_mcp_server(
    role: &mut DocumentMut,
    server_name: &'static str,
    command: &Path,
    args: &[&str],
    environment: &[(&'static str, &Path)],
) -> Result<(), BuiltinNetworkCopilotError> {
    let role_name = role
        .get("name")
        .and_then(Item::as_str)
        .unwrap_or("unknown")
        .to_string();
    let server = role
        .get_mut("mcp_servers")
        .and_then(Item::as_table_mut)
        .and_then(|servers| servers.get_mut(server_name))
        .and_then(Item::as_table_mut)
        .ok_or_else(|| BuiltinNetworkCopilotError::InvalidRoleTemplate {
            role: server_name,
            message: format!("Role {role_name} omits mcp_servers.{server_name}"),
        })?;
    server["command"] = value(path_text("MCP launcher", command)?);
    server.remove("cwd");
    let mut rendered_args = Array::new();
    for argument in args {
        rendered_args.push(*argument);
    }
    server["args"] = value(rendered_args);

    let env = server
        .get_mut("env")
        .and_then(Item::as_table_mut)
        .ok_or_else(|| BuiltinNetworkCopilotError::InvalidRoleTemplate {
            role: server_name,
            message: format!("Role {role_name} omits mcp_servers.{server_name}.env"),
        })?;
    for (name, path) in environment {
        env[name] = value(path_text("MCP environment path", path)?);
    }
    Ok(())
}

fn finish_role_template(
    role_name: &'static str,
    role: DocumentMut,
) -> Result<String, BuiltinNetworkCopilotError> {
    let rendered = role.to_string();
    if rendered.contains("__OPEN_WEB_CODEX_") {
        return Err(BuiltinNetworkCopilotError::InvalidRoleTemplate {
            role: role_name,
            message: "unresolved deployment path placeholder".to_string(),
        });
    }
    Ok(rendered)
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

    fn fixture() -> (tempfile::TempDir, BuiltinNetworkCopilotAssets, PathBuf) {
        let temp = tempfile::tempdir().expect("temp dir");
        let supply = temp.path().join("supply chain \"资产");
        let maps = temp.path().join("maps 资产");
        let supply_venv = temp.path().join("supply venv 环境");
        let maps_venv = temp.path().join("maps venv 环境");
        let profile = temp.path().join("profile 用户");
        fs::create_dir_all(&profile).expect("profile");

        write_file(&supply.join(SUPPLY_CHAIN_LAUNCHER), "#!/bin/sh\n", true);
        for relative in [SUPPLY_CHAIN_RUNTIME_FILE, SUPPLY_CHAIN_DATA_RUNTIME_FILE] {
            write_file(&supply.join(relative), "# runtime\n", false);
        }
        write_file(&maps.join(MAPS_LAUNCHER), "#!/bin/sh\n", true);
        write_file(&maps.join(MAPS_RUNTIME_FILE), "# runtime\n", false);
        write_file(&maps.join(MAPS_STYLE_SPEC_PACKAGE), "{}\n", false);
        write_file(&supply_venv.join("bin/python"), "#!/bin/sh\n", true);
        write_file(&maps_venv.join("bin/python"), "#!/bin/sh\n", true);

        let assets = BuiltinNetworkCopilotAssets::resolve(&supply, &supply_venv, &maps, &maps_venv)
            .expect("resolve assets");
        (temp, assets, profile)
    }

    #[test]
    fn renders_native_profile_seeds_with_role_local_mcp_only() {
        let (_temp, assets, profile) = fixture();
        let runtime = profile.join(".open-web-codex");
        let logs = runtime.join("logs");
        let data = assets
            .render_data_role(&profile, &runtime, &logs)
            .expect("data role");
        let network = assets
            .render_network_role(&profile, &runtime, &logs)
            .expect("network role");

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
        assert!(data["mcp_servers"]["supply_chain"].get("cwd").is_none());
        assert_eq!(
            data["mcp_servers"]["supply_chain"]["args"]
                .as_array()
                .expect("data args")
                .iter()
                .filter_map(|item| item.as_str())
                .collect::<Vec<_>>(),
            vec!["--data-server"]
        );
        assert_eq!(
            data["mcp_servers"]["supply_chain"]["default_tools_approval_mode"].as_str(),
            Some("approve")
        );
        assert!(data["developer_instructions"]
            .as_str()
            .expect("data instructions")
            .contains("Do not use shell, Workspace command, Git, jq, or ad-hoc Python"));
        assert!(network["mcp_servers"]["supply_chain"].get("cwd").is_none());
        assert!(network["mcp_servers"]["supply_chain"]["args"]
            .as_array()
            .expect("network args")
            .is_empty());
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
        for tool in &supply_chain_tools[..12] {
            assert_eq!(
                network["mcp_servers"]["supply_chain"]["tools"][*tool]["approval_mode"].as_str(),
                Some("approve"),
                "safe Network Tool {tool} must be preapproved",
            );
        }
        for tool in &supply_chain_tools[12..] {
            assert!(
                network["mcp_servers"]["supply_chain"]["tools"]
                    .get(*tool)
                    .is_none(),
                "final Workspace Tool {tool} must inherit prompt",
            );
        }
        assert_eq!(
            network["mcp_servers"]["map_utils"]["command"].as_str(),
            Some(path_text("maps launcher", &assets.maps_launcher).expect("path"))
        );
        assert_eq!(
            network["mcp_servers"]["map_utils"]["cwd"].as_str(),
            Some(path_text("maps root", &assets.maps_root).expect("path"))
        );
        assert_eq!(
            network["mcp_servers"]["map_utils"]["args"]
                .as_array()
                .and_then(|args| args.get(1))
                .and_then(|item| item.as_str()),
            Some(
                path_text(
                    "maps state",
                    &profile.join(".open-web-codex/mcp-state/maps-mcp")
                )
                .expect("path")
            )
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
        assert!(!data.to_string().contains("__OPEN_WEB_CODEX_"));
        assert!(!network.to_string().contains("__OPEN_WEB_CODEX_"));
        assert_eq!(
            assets.startup_files(&profile).expect("startup files").len(),
            5
        );
        assert!(!profile.join("config.toml").exists());
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
        assert!(SUPERVISOR_SKILL
            .contains("当前后续 Tool 实际需要、且由前轮 Tool 返回的全部 exact refs"));
        assert!(SUPERVISOR_SKILL.contains("typed `needs_context`"));
        assert!(SUPERVISOR_SKILL.contains("不得 list resources、读取 Resource 正文"));
        assert!(SUPERVISOR_SKILL
            .contains("阶段完成 handoff 动态列出本阶段实际 Tool 返回且后续适用的 exact refs"));
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
        assert!(NETWORK_SKILL
            .contains("处理 follow-up 时只消费 Supervisor 或当前 Thread 提供的 exact refs"));
        assert!(NETWORK_SKILL.contains("当前 Tool 任一 required ref"));
        assert!(NETWORK_SKILL.contains("typed `needs_context`"));
        assert!(
            NETWORK_SKILL.contains("阶段完成时动态列出本阶段实际 Tool 返回且后续适用的 exact refs")
        );
        assert!(NETWORK_SKILL
            .contains("final Tool/Artifact typed descriptor 与 Platform terminal state 是权威"));
        assert!(NETWORK_SKILL.contains(
            "下游 Tool 同时接收 result 与 comparison 时，comparison 必须由同一个 exact result 产生"
        ));
        assert!(NETWORK_ROLE.contains("call the matching domain Tool directly"));
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
    fn rejects_missing_or_relative_deployment_assets_as_unavailable() {
        let temp = tempfile::tempdir().expect("temp dir");
        assert!(matches!(
            BuiltinNetworkCopilotAssets::resolve(
                Path::new("relative"),
                temp.path(),
                temp.path(),
                temp.path(),
            ),
            Err(BuiltinNetworkCopilotError::Unavailable { .. })
        ));
        assert!(matches!(
            BuiltinNetworkCopilotAssets::resolve(
                &temp.path().join("missing"),
                temp.path(),
                temp.path(),
                temp.path(),
            ),
            Err(BuiltinNetworkCopilotError::Unavailable { .. })
        ));
    }

    #[cfg(unix)]
    #[test]
    fn accepts_canonical_deployment_symlink_and_rejects_child_escape() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().expect("temp dir");
        let real = temp.path().join("real");
        fs::create_dir_all(&real).expect("real root");
        let linked = temp.path().join("linked");
        symlink(&real, &linked).expect("symlink root");
        assert_eq!(
            required_absolute_directory("linked root", &linked).expect("canonical root"),
            real.canonicalize().expect("canonical real root")
        );

        let outside = temp.path().join("outside");
        write_file(&outside.join("launcher"), "#!/bin/sh\n", true);
        symlink(outside.join("launcher"), real.join("launcher")).expect("escaped child");
        assert!(matches!(
            required_regular_file(
                "escaped launcher",
                &real.canonicalize().expect("canonical root"),
                "launcher",
                true,
            ),
            Err(BuiltinNetworkCopilotError::Unavailable { .. })
        ));
    }
}
