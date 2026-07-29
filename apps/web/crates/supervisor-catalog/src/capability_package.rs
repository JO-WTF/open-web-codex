use std::collections::BTreeMap;

use open_web_codex_platform_contracts::CapabilityPackageSummary;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::supervisor::SupervisorCatalogError;
use crate::validation::is_safe_definition_id;

const HELLO_AGENT_MANIFEST: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../tools/hello-agent/.codex-plugin/plugin.json"
));
const HELLO_AGENT_MCP: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../tools/hello-agent/.mcp.json"
));
const MAP_UTILS_MANIFEST: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../tools/maps-mcp/.codex-plugin/plugin.json"
));
const MAP_UTILS_MCP: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../tools/maps-mcp/.mcp.json"
));
const SUPPLY_CHAIN_MANIFEST: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../tools/supply-chain-network-planner/.codex-plugin/plugin.json"
));
const SUPPLY_CHAIN_MCP: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../tools/supply-chain-network-planner/.mcp.json"
));

struct CapabilityPackageResource {
    manifest: &'static str,
    mcp: &'static str,
    capability_root_id: &'static str,
}

const CAPABILITY_PACKAGE_RESOURCES: [CapabilityPackageResource; 3] = [
    CapabilityPackageResource {
        manifest: HELLO_AGENT_MANIFEST,
        mcp: HELLO_AGENT_MCP,
        capability_root_id: "local-hello-agent",
    },
    CapabilityPackageResource {
        manifest: MAP_UTILS_MANIFEST,
        mcp: MAP_UTILS_MCP,
        capability_root_id: "local-maps-mcp",
    },
    CapabilityPackageResource {
        manifest: SUPPLY_CHAIN_MANIFEST,
        mcp: SUPPLY_CHAIN_MCP,
        capability_root_id: "local-supply-chain-network-planner",
    },
];

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PluginManifest {
    name: String,
    version: String,
    description: String,
    skills: Option<String>,
    mcp_servers: String,
    interface: PluginInterface,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PluginInterface {
    display_name: String,
    capabilities: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct McpManifest {
    mcp_servers: BTreeMap<String, serde_json::Value>,
}

pub fn list_published() -> Result<Vec<CapabilityPackageSummary>, SupervisorCatalogError> {
    let mut packages = CAPABILITY_PACKAGE_RESOURCES
        .iter()
        .map(resolve_resource)
        .collect::<Result<Vec<_>, _>>()?;
    packages.sort_by(|left, right| left.display_name.cmp(&right.display_name));
    Ok(packages)
}

fn resolve_resource(
    resource: &CapabilityPackageResource,
) -> Result<CapabilityPackageSummary, SupervisorCatalogError> {
    let manifest = serde_json::from_str::<PluginManifest>(resource.manifest)
        .map_err(|_| SupervisorCatalogError::Invalid("capability package manifest is invalid"))?;
    let mcp = serde_json::from_str::<McpManifest>(resource.mcp).map_err(|_| {
        SupervisorCatalogError::Invalid("capability package MCP manifest is invalid")
    })?;
    if !is_safe_definition_id(&manifest.name)
        || !is_safe_package_version(&manifest.version)
        || manifest.description.trim().is_empty()
        || manifest.description.len() > 512
        || manifest.interface.display_name.trim().is_empty()
        || manifest.interface.display_name.len() > 256
        || manifest.interface.capabilities.is_empty()
        || manifest.interface.capabilities.len() > 32
        || manifest
            .interface
            .capabilities
            .iter()
            .any(|capability| capability.trim().is_empty() || capability.len() > 256)
        || manifest.mcp_servers != "./.mcp.json"
        || mcp.mcp_servers.is_empty()
        || mcp.mcp_servers.len() > 32
        || mcp.mcp_servers.keys().any(|name| {
            name.is_empty()
                || name.len() > 96
                || !name
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        })
    {
        return Err(SupervisorCatalogError::Invalid(
            "capability package declaration is invalid",
        ));
    }
    Ok(CapabilityPackageSummary {
        release_id: None,
        workspace_id: None,
        capability_root_id: resource.capability_root_id.to_string(),
        package_id: manifest.name,
        version: manifest.version,
        display_name: manifest.interface.display_name,
        description: manifest.description,
        capabilities: manifest.interface.capabilities,
        mcp_server_names: mcp.mcp_servers.into_keys().collect(),
        tool_names: Vec::new(),
        input_artifact_types: Vec::new(),
        output_artifact_types: Vec::new(),
        includes_skills: manifest.skills.as_deref() == Some("./skills/"),
        source: "repository".to_string(),
        content_sha256: {
            let mut digest = Sha256::new();
            digest.update(resource.manifest.as_bytes());
            digest.update(resource.mcp.as_bytes());
            hex::encode(digest.finalize())
        },
    })
}

fn is_safe_package_version(version: &str) -> bool {
    !version.is_empty()
        && version.len() <= 96
        && version
            .as_bytes()
            .first()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && version
            .as_bytes()
            .last()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && version.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'.' | b'-' | b'_' | b'+')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_map_utils_from_its_checked_in_package_declaration() {
        let packages = list_published().unwrap();
        let maps = packages
            .iter()
            .find(|package| package.package_id == "map-utils")
            .unwrap();
        assert_eq!(maps.capability_root_id, "local-maps-mcp");
        assert_eq!(maps.mcp_server_names, vec!["map_utils"]);
        assert!(maps.includes_skills);
    }
}
