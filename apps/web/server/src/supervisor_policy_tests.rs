use open_web_codex_platform_contracts::SupervisorPolicySelection;
use serde_json::{json, Value};

use super::{
    list_published, policy_content_sha256, resolve, validate_runtime_manifest,
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
    let snapshot = &policy.snapshot;

    assert_eq!(snapshot.policy_id, selected.policy_id);
    assert_eq!(snapshot.version, selected.version);
    assert_eq!(
        snapshot.content_sha256,
        policy_content_sha256(
            &snapshot.developer_instructions,
            &policy.required_runtime_roles
        )
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

#[test]
fn policy_snapshot_seals_the_referenced_runtime_role_content() {
    let published = list_published();
    let policy = resolve(&SupervisorPolicySelection {
        policy_id: published[0].policy_id.clone(),
        version: published[0].version.clone(),
    })
    .unwrap();
    let mut changed_roles = policy.required_runtime_roles.clone();
    changed_roles[0].content_sha256 = "0".repeat(64);

    assert_ne!(
        policy_content_sha256(&policy.snapshot.developer_instructions, &changed_roles),
        policy.snapshot.content_sha256
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
fn requires_the_generated_multi_agent_capability_contract() {
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
fn rejects_unknown_versions_instead_of_falling_back_to_latest() {
    let error = resolve(&SupervisorPolicySelection {
        policy_id: "enterprise-supervisor-copilot".to_string(),
        version: "9.9.9".to_string(),
    })
    .unwrap_err();

    assert_eq!(error, SupervisorPolicyError::NotFound);
}
