use open_web_codex_platform_contracts::SupervisorPolicySelection;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::{list_published, resolve, validate_runtime_manifest, SupervisorPolicyError};

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
        policy
            .required_runtime_roles
            .iter()
            .map(|role| role.name.as_str())
            .collect::<Vec<_>>(),
        vec!["data_agent", "network_planning_agent"]
    );
}

fn generated_manifest() -> Value {
    serde_json::from_str(include_str!(
        "../../contracts/codex/fixtures/capability-manifest.v1.json"
    ))
    .expect("generated capability manifest fixture")
}

fn capability_mut<'a>(manifest: &'a mut Value, id: &str) -> &'a mut Value {
    manifest["capabilities"]
        .as_array_mut()
        .expect("capabilities array")
        .iter_mut()
        .find(|capability| capability["id"] == id)
        .expect("declared capability")
}

#[test]
fn requires_the_generated_multi_agent_and_v1_backend_capability_contracts() {
    let manifest = generated_manifest();
    validate_runtime_manifest(&manifest).unwrap();
}

#[test]
fn rejects_unavailable_multi_agent_capability() {
    let mut manifest = generated_manifest();
    let capability = capability_mut(&mut manifest, "agents.multi_agent");
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
fn rejects_missing_wrong_version_or_unavailable_v1_backend_capability() {
    let mut missing = generated_manifest();
    missing["capabilities"]
        .as_array_mut()
        .expect("capabilities array")
        .retain(|capability| capability["id"] != "agents.multi_agent_v1_backend_override");
    assert_eq!(
        validate_runtime_manifest(&missing).unwrap_err(),
        SupervisorPolicyError::Capability(
            "Codex did not declare multi-agent V1 backend override support".to_string()
        )
    );

    let mut wrong_version = generated_manifest();
    capability_mut(&mut wrong_version, "agents.multi_agent_v1_backend_override")["version"] =
        json!("2.0.0");
    assert_eq!(
        validate_runtime_manifest(&wrong_version).unwrap_err(),
        SupervisorPolicyError::Capability(
            "Codex multi-agent V1 backend override capability version '2.0.0' is unsupported"
                .to_string()
        )
    );

    let mut unavailable = generated_manifest();
    let capability = capability_mut(&mut unavailable, "agents.multi_agent_v1_backend_override");
    capability["status"] = json!("experimental");
    capability["experimental"] = json!(false);
    assert_eq!(
        validate_runtime_manifest(&unavailable).unwrap_err(),
        SupervisorPolicyError::Capability(
            "Codex multi-agent V1 backend override capability is unavailable".to_string()
        )
    );
}

#[test]
fn accepts_supported_or_explicitly_enabled_experimental_v1_backend_capability() {
    let mut supported = generated_manifest();
    let capability = capability_mut(&mut supported, "agents.multi_agent_v1_backend_override");
    capability["status"] = json!("supported");
    capability["experimental"] = json!(false);
    validate_runtime_manifest(&supported).expect("supported V1 backend override");

    let mut experimental = generated_manifest();
    let capability = capability_mut(&mut experimental, "agents.multi_agent_v1_backend_override");
    capability["status"] = json!("experimental");
    capability["experimental"] = json!(true);
    validate_runtime_manifest(&experimental)
        .expect("explicitly enabled experimental V1 backend override");
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
