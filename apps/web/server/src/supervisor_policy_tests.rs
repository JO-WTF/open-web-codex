use open_web_codex_platform_contracts::SupervisorPolicySelection;
use sha2::{Digest, Sha256};

use serde_json::json;

use super::{
    list_published, resolve, validate_runtime_config, validate_runtime_manifest,
    SupervisorPolicyError,
};

#[test]
fn resolves_only_the_exact_published_version_and_seals_its_content() {
    let published = list_published();
    let selected = SupervisorPolicySelection {
        policy_id: published[0].policy_id.clone(),
        version: published[0].version.clone(),
    };

    let policy = resolve(&selected).unwrap();
    let snapshot = policy.snapshot;

    assert_eq!(snapshot.policy_id, selected.policy_id);
    assert_eq!(snapshot.version, selected.version);
    assert_eq!(
        snapshot.content_sha256,
        hex::encode(Sha256::digest(snapshot.developer_instructions.as_bytes()))
    );
    assert_eq!(
        policy.required_runtime_roles,
        vec!["data_agent", "network_planning_agent"]
    );
}

#[test]
fn requires_exact_runtime_roles_without_a_generic_agent_fallback() {
    let required = vec![
        "data_agent".to_string(),
        "network_planning_agent".to_string(),
    ];
    let valid = json!({
        "config": {
            "features": { "multi_agent": true },
            "agents": {
                "max_concurrent_threads_per_session": 2,
                "max_depth": 1,
                "data_agent": { "config_file": "agents/data_agent.toml" },
                "network_planning_agent": {
                    "config_file": "agents/network_planning_agent.toml"
                }
            }
        }
    });
    validate_runtime_config(&valid, &required).unwrap();

    let mut disabled = valid.clone();
    disabled["config"]["agents"]["enabled"] = json!(false);
    let error = validate_runtime_config(&disabled, &required).unwrap_err();
    assert_eq!(
        error,
        SupervisorPolicyError::Capability("Codex multi-agent support is disabled".to_string())
    );

    let missing_network = json!({
        "config": {
            "features": { "multi_agent": true },
            "agents": {
                "max_concurrent_threads_per_session": 2,
                "max_depth": 1,
                "data_agent": { "config_file": "agents/data_agent.toml" },
                "default": { "config_file": "agents/default.toml" }
            }
        }
    });
    let error = validate_runtime_config(&missing_network, &required).unwrap_err();
    assert_eq!(
        error,
        SupervisorPolicyError::Capability(
            "required Runtime Roles are not configured: network_planning_agent".to_string()
        )
    );
}

#[test]
fn requires_the_generated_multi_agent_capability_contract() {
    let mut manifest: serde_json::Value = serde_json::from_str(include_str!(
        "../../contracts/codex/fixtures/capability-manifest.v1.json"
    ))
    .unwrap();
    validate_runtime_manifest(&manifest).unwrap();

    let capability = manifest["capabilities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|capability| capability["id"] == "agents.multi_agent")
        .unwrap();
    capability["status"] = json!("unsupported");
    let error = validate_runtime_manifest(&manifest).unwrap_err();
    assert_eq!(
        error,
        SupervisorPolicyError::Capability(
            "Codex multi-agent capability is unavailable".to_string()
        )
    );
}

#[test]
fn rejects_unknown_versions_instead_of_falling_back_to_latest() {
    let error = resolve(&SupervisorPolicySelection {
        policy_id: "enterprise-supervisor-copilot".to_string(),
        version: "9.9.9".to_string(),
    })
    .unwrap_err();

    assert_eq!(error, SupervisorPolicyError::NotFound);
}
